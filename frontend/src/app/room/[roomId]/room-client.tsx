"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Settings } from "lucide-react";
import { SettingsDialog } from "@/components/settings-dialog";
import { Button } from "@/components/ui/button";
import { RoomAudioDiagnostics } from "./room-audio-diagnostics";
import {
  closeServerMediaSession,
  getMediaRelay,
  getIceServers,
  joinRoom,
  leaveRoom,
  registerMediaTrack,
  shareRoomUrl,
  startMediaRelay,
  updateMediaRelaySettings,
  updateMediaRelaySubscriptions,
  type RoomSnapshot,
  type UserProfile
} from "@/lib/api";
import { ServerMediaAudioSession } from "@/lib/server-media-audio";
import {
  createRoomSocket,
  reducePresence,
  type PresenceState,
  type SignalMessage
} from "@/lib/signalling";
import { readNickname, readNoiseConfig } from "@/lib/storage";
import { useSettingsStore, type SettingsSnapshot, type UserAudioSettings } from "@/lib/settings-store";
import { VoiceActivityDetector } from "@/lib/voice-activity";
import { openLocalAudioStream } from "@/lib/webrtc";

type RoomSession = {
  roomId: string;
  user: UserProfile;
  accessToken: string;
};

function isStoredRoomSession(input: unknown, roomId: string): input is RoomSession {
  if (!input || typeof input !== "object") {
    return false;
  }
  const session = input as Partial<RoomSession>;
  return (
    session.roomId === roomId &&
    typeof session.accessToken === "string" &&
    session.accessToken.length > 0 &&
    !!session.user &&
    typeof session.user.id === "string" &&
    session.user.id.length > 0 &&
    typeof session.user.nickname === "string" &&
    typeof session.user.joined_at === "string"
  );
}

function readRoomSession(roomId: string): RoomSession | null {
  const stored = sessionStorage.getItem("lyre.roomSession");
  if (!stored) {
    return null;
  }
  try {
    const parsed = JSON.parse(stored) as unknown;
    if (!isStoredRoomSession(parsed, roomId)) {
      return null;
    }
    return parsed;
  } catch {
    return null;
  }
}

function clearRoomSession() {
  sessionStorage.removeItem("lyre.roomSession");
}

const AUDIO_RECONNECT_RETRY_MS = 1_000;
const RELAY_SOURCE_REFRESH_RETRY_MS = 1_000;
const SOCKET_RECONNECT_RETRY_MS = 1_000;
const AUDIO_SIGNALLING_SOCKET_ERROR = "Audio signalling websocket is not connected";
const DEFAULT_USER_AUDIO_SETTINGS: UserAudioSettings = { muted: false, volumePercent: 100 };

export function RoomClient({ roomId }: { roomId: string }) {
  const [currentUser, setCurrentUser] = useState<UserProfile | null>(null);
  const [accessToken, setAccessToken] = useState<string | null>(null);
  const [room, setRoom] = useState<RoomSnapshot | null>(null);
  const [status, setStatus] = useState("Joining");
  const [audioStarted, setAudioStarted] = useState(false);
  const [muted, setMuted] = useState(false);
  const [socketOpen, setSocketOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [relaySourceIds, setRelaySourceIds] = useState<string[]>([]);
  const [audioDiagnosticsRefreshKey, setAudioDiagnosticsRefreshKey] = useState(0);
  const [speakingUserIds, setSpeakingUserIds] = useState<Set<string>>(() => new Set());
  const audioDiagnosticsEnabled = useSettingsStore((state) => state.audioDiagnosticsEnabled);
  const userAudio = useSettingsStore((state) => state.userAudio);
  const setUserAudioSettings = useSettingsStore((state) => state.setUserAudioSettings);
  const socketRef = useRef<WebSocket | null>(null);
  const serverAudioSessionRef = useRef<ServerMediaAudioSession | null>(null);
  const localVoiceActivityRef = useRef<VoiceActivityDetector | null>(null);
  const audioStartedRef = useRef(false);
  const relayStartedRef = useRef(false);
  const reconnectingAudioRef = useRef(false);
  const reconnectRetryRef = useRef<number | null>(null);
  const socketReconnectRetryRef = useRef<number | null>(null);
  const relaySourceRefreshRetryRef = useRef<number | null>(null);
  const reconnectServerRelayAudioRef = useRef<() => void>(() => undefined);
  const reconnectRoomSocketRef = useRef<() => void>(() => undefined);
  const serverMediaCleanupNeededRef = useRef(false);
  const lastSubscribedSourceIdsRef = useRef<string[]>([]);
  const [relaySourceRefreshTick, setRelaySourceRefreshTick] = useState(0);
  const link = useMemo(() => shareRoomUrl(roomId), [roomId]);
  const remoteUsers = useMemo(
    () => (room?.users ?? []).filter((user) => user.id !== currentUser?.id),
    [currentUser?.id, room?.users]
  );
  const subscribedSourceIds = useMemo(
    () => remoteUsers
      .filter((user) => relaySourceIds.includes(user.id))
      .filter((user) => !userAudio[user.id]?.muted)
      .map((user) => user.id),
    [relaySourceIds, remoteUsers, userAudio]
  );

  const refreshRelaySourceIds = useCallback(async () => {
    if (!currentUser) {
      return [];
    }
    const status = await getMediaRelay(roomId);
    const sourceIds = status.participants
      .map((participant) => participant.user_id)
      .filter((userId) => userId !== currentUser.id);
    setRelaySourceIds((current) => arraysEqual(current, sourceIds) ? current : sourceIds);
    return sourceIds;
  }, [currentUser, roomId]);

  const subscribedSourceIdsFromRelaySources = useCallback((
    sourceIds: string[],
    audioSettings: Record<string, UserAudioSettings> = userAudio
  ) => sourceIds.filter((userId) => !audioSettings[userId]?.muted), [userAudio]);

  const setUserSpeaking = useCallback((userId: string, speaking: boolean) => {
    setSpeakingUserIds((current) => {
      const next = new Set(current);
      if (speaking) {
        next.add(userId);
      } else {
        next.delete(userId);
      }
      return next;
    });
  }, []);

  const clearSpeaking = useCallback(() => {
    setSpeakingUserIds(new Set());
  }, []);

  const closeAudioSessions = useCallback(() => {
    localVoiceActivityRef.current?.stop();
    localVoiceActivityRef.current = null;
    serverAudioSessionRef.current?.close();
    serverAudioSessionRef.current = null;
    clearSpeaking();
  }, [clearSpeaking]);

  const clearReconnectRetry = useCallback(() => {
    if (reconnectRetryRef.current !== null) {
      window.clearTimeout(reconnectRetryRef.current);
      reconnectRetryRef.current = null;
    }
  }, []);

  const clearSocketReconnectRetry = useCallback(() => {
    if (socketReconnectRetryRef.current !== null) {
      window.clearTimeout(socketReconnectRetryRef.current);
      socketReconnectRetryRef.current = null;
    }
  }, []);

  const clearRelaySourceRefreshRetry = useCallback(() => {
    if (relaySourceRefreshRetryRef.current !== null) {
      window.clearTimeout(relaySourceRefreshRetryRef.current);
      relaySourceRefreshRetryRef.current = null;
    }
  }, []);

  const handleAudioError = useCallback((message: string) => {
    setStatus(message);
    if (message !== AUDIO_SIGNALLING_SOCKET_ERROR) {
      return;
    }
    socketRef.current = null;
    setSocketOpen(false);
    closeAudioSessions();
    reconnectRoomSocketRef.current();
  }, [closeAudioSessions]);

  const scheduleRelaySourceRefreshRetry = useCallback(() => {
    if (relaySourceRefreshRetryRef.current !== null) {
      return;
    }
    relaySourceRefreshRetryRef.current = window.setTimeout(() => {
      relaySourceRefreshRetryRef.current = null;
      setRelaySourceRefreshTick((tick) => tick + 1);
    }, RELAY_SOURCE_REFRESH_RETRY_MS);
  }, []);

  useEffect(() => {
    let cancelled = false;
    let intentionalClose = false;

    function connectSocket(session: RoomSession) {
      const socket = createRoomSocket(roomId, session.user.id, session.accessToken);
      socketRef.current = socket;
      socket.onopen = () => {
        if (cancelled || socketRef.current !== socket) {
          return;
        }
        setStatus("Connected");
        setSocketOpen(true);
        if (audioStartedRef.current && !serverAudioSessionRef.current) {
          reconnectServerRelayAudioRef.current();
        }
      };
      socket.onmessage = (event) => {
        if (socketRef.current !== socket) {
          return;
        }
        const signal = JSON.parse(event.data as string) as SignalMessage;
        void serverAudioSessionRef.current?.handleSignal(signal);
        setRoom((current) => {
          const next: PresenceState = reducePresence({ room: current ?? undefined }, signal);
          if (next.error) {
            setStatus(next.error);
          }
          return next.room ?? current;
        });
      };
      socket.onclose = () => {
        if (cancelled || intentionalClose || socketRef.current !== socket) {
          return;
        }
        socketRef.current = null;
        setSocketOpen(false);
        closeAudioSessions();
        setStatus("Reconnecting");
        reconnectRoomSocketRef.current();
      };
    }

    async function enterRoom() {
      let session = readRoomSession(roomId);
      if (!session) {
        const response = await joinRoom(roomId, { nickname: readNickname(), noise: readNoiseConfig() });
        session = { roomId, user: response.user, accessToken: response.access_token };
        setRoom(response.room);
        sessionStorage.setItem("lyre.roomSession", JSON.stringify(session));
      }
      if (cancelled) {
        return;
      }
      setCurrentUser(session.user);
      setAccessToken(session.accessToken);
      reconnectRoomSocketRef.current = () => {
        if (socketReconnectRetryRef.current !== null) {
          return;
        }
        socketReconnectRetryRef.current = window.setTimeout(() => {
          socketReconnectRetryRef.current = null;
          if (!cancelled) {
            connectSocket(session);
          }
        }, SOCKET_RECONNECT_RETRY_MS);
      };
      connectSocket(session);
    }

    void enterRoom();

    return () => {
      cancelled = true;
      intentionalClose = true;
      closeAudioSessions();
      audioStartedRef.current = false;
      relayStartedRef.current = false;
      reconnectingAudioRef.current = false;
      reconnectRoomSocketRef.current = () => undefined;
      clearReconnectRetry();
      clearSocketReconnectRetry();
      clearRelaySourceRefreshRetry();
      serverMediaCleanupNeededRef.current = false;
      lastSubscribedSourceIdsRef.current = [];
      setRelaySourceIds([]);
      setAudioStarted(false);
      setSocketOpen(false);
      clearRoomSession();
      socketRef.current?.close();
      socketRef.current = null;
    };
  }, [clearReconnectRetry, clearRelaySourceRefreshRetry, clearSocketReconnectRetry, closeAudioSessions, roomId]);

  const connectServerRelayAudio = useCallback(async ({
    updateRelay,
    sourceIds,
    audioSettings = userAudio
  }: {
    updateRelay: boolean;
    sourceIds?: string[];
    audioSettings?: Record<string, UserAudioSettings>;
  }) => {
    if (!currentUser || !accessToken) {
      return;
    }
    if (audioStartedRef.current && !updateRelay) {
      return;
    }
    let stream: MediaStream | null = null;
    let cleanupNeeded = false;
    try {
      audioStartedRef.current = true;
      setAudioStarted(true);
      const iceServers = await getIceServers();
      stream = await openLocalAudioStream();
      const noise = readNoiseConfig();
      const shouldStartRelay = !updateRelay && !relayStartedRef.current;
      if (shouldStartRelay) {
        await startMediaRelay(roomId, noise, accessToken);
        cleanupNeeded = true;
        serverMediaCleanupNeededRef.current = true;
        await registerMediaTrack(roomId, currentUser.id, "audio-main", "audio", accessToken);
        relayStartedRef.current = true;
      }
      const activeSourceIds = await refreshRelaySourceIds();
      const waitingForRelaySources = remoteUsers.some((user) => !activeSourceIds.includes(user.id));
      if (waitingForRelaySources) {
        scheduleRelaySourceRefreshRetry();
      } else {
        clearRelaySourceRefreshRetry();
      }
      const nextSourceIds = sourceIds
        ? sourceIds.filter((sourceId) => activeSourceIds.includes(sourceId))
        : subscribedSourceIdsFromRelaySources(activeSourceIds, audioSettings);
      await updateMediaRelaySubscriptions(roomId, currentUser.id, nextSourceIds, accessToken);
      lastSubscribedSourceIdsRef.current = nextSourceIds;
      const socket = socketRef.current;
      if (!socket || socket.readyState !== WebSocket.OPEN) {
        if (socketRef.current === socket) {
          socketRef.current = null;
        }
        setSocketOpen(false);
        reconnectRoomSocketRef.current();
        throw new Error(AUDIO_SIGNALLING_SOCKET_ERROR);
      }
      const session = new ServerMediaAudioSession({
        roomId,
        userId: currentUser.id,
        accessToken,
        socket,
        iceServers,
        stream,
        userAudio: audioSettings,
        onError: handleAudioError,
        onConnectionInterrupted: () => reconnectServerRelayAudioRef.current(),
        onRemoteTrack: () => setAudioDiagnosticsRefreshKey((key) => key + 1),
        onRemoteSpeakingChange: setUserSpeaking
      });
      session.setMuted(muted);
      serverAudioSessionRef.current = session;
      await session.start();
      localVoiceActivityRef.current?.stop();
      localVoiceActivityRef.current = new VoiceActivityDetector(stream, (speaking) => {
        setUserSpeaking(currentUser.id, speaking);
      });
      localVoiceActivityRef.current.start();
      stream = null;
      clearReconnectRetry();
      setAudioDiagnosticsRefreshKey((key) => key + 1);
      setStatus("Server relay audio connected");
    } catch (error) {
      const message = error instanceof Error ? error.message : "Audio connection failed";
      const waitingForSocket = message === AUDIO_SIGNALLING_SOCKET_ERROR;
      if (!updateRelay && !waitingForSocket) {
        audioStartedRef.current = false;
        setAudioStarted(false);
      }
      serverAudioSessionRef.current?.close();
      serverAudioSessionRef.current = null;
      if (stream) {
        for (const track of stream.getAudioTracks()) {
          track.stop();
        }
      }
      if (cleanupNeeded) {
        try {
          await closeServerMediaSession(roomId, currentUser.id, accessToken);
        } catch {
          // Keep the original startup error visible.
        }
        serverMediaCleanupNeededRef.current = false;
        relayStartedRef.current = false;
        lastSubscribedSourceIdsRef.current = [];
      }
      setStatus(message);
    }
  }, [
    accessToken,
    clearReconnectRetry,
    clearRelaySourceRefreshRetry,
    currentUser,
    handleAudioError,
    muted,
    refreshRelaySourceIds,
    remoteUsers,
    roomId,
    scheduleRelaySourceRefreshRetry,
    setUserSpeaking,
    subscribedSourceIdsFromRelaySources,
    userAudio
  ]);

  useEffect(() => {
    reconnectServerRelayAudioRef.current = () => {
      if (reconnectingAudioRef.current || !audioStartedRef.current) {
        return;
      }
      reconnectingAudioRef.current = true;
      setStatus("Reconnecting audio");
      closeAudioSessions();
      void connectServerRelayAudio({ updateRelay: true }).finally(() => {
        reconnectingAudioRef.current = false;
        if (audioStartedRef.current && !serverAudioSessionRef.current && socketRef.current?.readyState === WebSocket.OPEN) {
          reconnectRetryRef.current = window.setTimeout(() => {
            reconnectRetryRef.current = null;
            reconnectServerRelayAudioRef.current();
          }, AUDIO_RECONNECT_RETRY_MS);
        }
      });
    };
  }, [closeAudioSessions, connectServerRelayAudio]);

  useEffect(() => {
    if (!currentUser || !accessToken || !socketOpen || audioStartedRef.current) {
      return;
    }
    void connectServerRelayAudio({ updateRelay: false });
  }, [accessToken, connectServerRelayAudio, currentUser, socketOpen]);

  function toggleMuted() {
    const nextMuted = !muted;
    setMuted(nextMuted);
    serverAudioSessionRef.current?.setMuted(nextMuted);
  }

  const loadAudioDiagnostics = useCallback(
    () => serverAudioSessionRef.current?.diagnostics() ?? Promise.resolve(null),
    []
  );

  async function saveSettings(settings: SettingsSnapshot) {
    if (!audioStartedRef.current || !currentUser || !accessToken) {
      return;
    }
    closeAudioSessions();
    await updateMediaRelaySettings(roomId, currentUser.id, settings.noise, accessToken);
    await connectServerRelayAudio({ updateRelay: true });
  }

  async function applyUserAudioSettings(userId: string, settings: Partial<UserAudioSettings>) {
    const current = userAudio[userId] ?? DEFAULT_USER_AUDIO_SETTINGS;
    const next = {
      ...current,
      ...settings
    };
    if (settings.muted === undefined) {
      setUserAudioSettings(userId, settings);
      serverAudioSessionRef.current?.setUserAudioSettings(userId, next);
      return;
    }
    if (!currentUser || !accessToken) {
      setUserAudioSettings(userId, settings);
      serverAudioSessionRef.current?.setUserAudioSettings(userId, next);
      return;
    }
    const nextRelaySourceIds = relaySourceIds.includes(userId)
      ? relaySourceIds
      : await refreshRelaySourceIds();
    const nextUserAudio = {
      ...userAudio,
      [userId]: next
    };
    const nextSourceIds = subscribedSourceIdsFromRelaySources(nextRelaySourceIds, nextUserAudio);
    try {
      await updateMediaRelaySubscriptions(roomId, currentUser.id, nextSourceIds, accessToken);
      lastSubscribedSourceIdsRef.current = nextSourceIds;
      setUserAudioSettings(userId, settings);
      serverAudioSessionRef.current?.setUserAudioSettings(userId, next);
      if (audioStartedRef.current) {
        closeAudioSessions();
        await connectServerRelayAudio({
          updateRelay: true,
          sourceIds: nextSourceIds,
          audioSettings: nextUserAudio
        });
      }
    } catch (error) {
      setStatus(error instanceof Error ? error.message : "Failed to update audio subscription");
    }
  }

  useEffect(() => {
    if (!currentUser || !accessToken || !audioStartedRef.current || !socketOpen || !serverAudioSessionRef.current) {
      return;
    }
    void (async () => {
      try {
        const activeSourceIds = await refreshRelaySourceIds();
        const nextSourceIds = subscribedSourceIdsFromRelaySources(activeSourceIds);
        const waitingForRelaySources = remoteUsers.some((user) => !activeSourceIds.includes(user.id));
        if (waitingForRelaySources) {
          scheduleRelaySourceRefreshRetry();
        } else {
          clearRelaySourceRefreshRetry();
        }
        if (arraysEqual(lastSubscribedSourceIdsRef.current, nextSourceIds)) {
          return;
        }
        await updateMediaRelaySubscriptions(roomId, currentUser.id, nextSourceIds, accessToken);
        lastSubscribedSourceIdsRef.current = nextSourceIds;
        closeAudioSessions();
        await connectServerRelayAudio({ updateRelay: true, sourceIds: nextSourceIds });
      } catch (error) {
        setStatus(error instanceof Error ? error.message : "Failed to update audio subscription");
      }
    })();
  }, [
    accessToken,
    closeAudioSessions,
    clearRelaySourceRefreshRetry,
    connectServerRelayAudio,
    currentUser,
    refreshRelaySourceIds,
    remoteUsers,
    relaySourceRefreshTick,
    roomId,
    scheduleRelaySourceRefreshRetry,
    socketOpen,
    subscribedSourceIds,
    subscribedSourceIdsFromRelaySources
  ]);

  async function leave() {
    const shouldCloseServerMedia = serverMediaCleanupNeededRef.current && currentUser;
    clearReconnectRetry();
    clearRelaySourceRefreshRetry();
    closeAudioSessions();
    audioStartedRef.current = false;
    relayStartedRef.current = false;
    setAudioStarted(false);
    if (currentUser && accessToken) {
      if (shouldCloseServerMedia) {
        await closeServerMediaSession(roomId, currentUser.id, accessToken);
        serverMediaCleanupNeededRef.current = false;
      }
      await leaveRoom(roomId, currentUser.id, accessToken);
    }
    clearRoomSession();
    socketRef.current?.close();
    socketRef.current = null;
    window.location.href = "/";
  }

  return (
    <section className="grid gap-5">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold">{roomId}</h1>
          <p className="mt-1 text-sm text-[#5c6a61]">{status}</p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button aria-label="Settings" onClick={() => setSettingsOpen(true)} variant="outline">
            <Settings className="h-4 w-4" />
            <span className="ml-2">Settings</span>
          </Button>
          <Button disabled={!audioStarted} onClick={toggleMuted}>{muted ? "Unmute" : "Mute"}</Button>
          <Button onClick={leave} variant="destructive">Leave</Button>
        </div>
      </div>
      <SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} onSave={saveSettings} />
      <div className="rounded-md border border-[#d8ded6] bg-white p-4">
        <div className="text-xs text-[#5c6a61]">Share</div>
        <div className="mt-1 break-all text-sm">{link}</div>
      </div>
      {audioDiagnosticsEnabled ? (
        <RoomAudioDiagnostics
          loadDiagnostics={loadAudioDiagnostics}
          relaySourceIds={relaySourceIds}
          refreshKey={audioDiagnosticsRefreshKey}
          subscribedSourceIds={subscribedSourceIds}
        />
      ) : null}
      <div className="rounded-md border border-[#d8ded6] bg-white">
        <div className="border-b border-[#d8ded6] px-4 py-3 text-sm font-semibold">Users</div>
        <ul className="divide-y divide-[#edf0ec]">
          {(room?.users ?? []).map((user) => (
            <li className="flex flex-wrap items-center justify-between gap-3 px-4 py-3 text-sm" key={user.id}>
              <span className="flex min-w-0 items-center gap-2">
                <span>{user.nickname}</span>
                {speakingUserIds.has(user.id) ? (
                  <span
                    aria-label={`${user.nickname} is speaking`}
                    className="size-2 rounded-full bg-[#2f8f46]"
                  />
                ) : null}
              </span>
              {user.id !== currentUser?.id ? (
                <div className="flex items-center gap-2">
                  <Button
                    onClick={() => void applyUserAudioSettings(user.id, { muted: !(userAudio[user.id]?.muted ?? false) })}
                    size="sm"
                    variant="outline"
                  >
                    {userAudio[user.id]?.muted ? `Unmute ${user.nickname}` : `Mute ${user.nickname}`}
                  </Button>
                  <label className="flex items-center gap-2 text-xs text-[#5c6a61]">
                    <span>{userAudio[user.id]?.volumePercent ?? 100}%</span>
                    <input
                      aria-label={`${user.nickname} volume`}
                      className="w-28"
                      max={150}
                      min={0}
                      onChange={(event) =>
                        void applyUserAudioSettings(user.id, { volumePercent: Number(event.target.value) })
                      }
                      type="range"
                      value={userAudio[user.id]?.volumePercent ?? 100}
                    />
                  </label>
                </div>
              ) : null}
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}

function arraysEqual(left: string[], right: string[]): boolean {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}
