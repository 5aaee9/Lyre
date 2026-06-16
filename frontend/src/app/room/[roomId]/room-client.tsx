"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Settings } from "lucide-react";
import { SettingsDialog } from "@/components/settings-dialog";
import { Button } from "@/components/ui/button";
import {
  closeServerMediaSession,
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
  const userAudio = useSettingsStore((state) => state.userAudio);
  const setUserAudioSettings = useSettingsStore((state) => state.setUserAudioSettings);
  const socketRef = useRef<WebSocket | null>(null);
  const serverAudioSessionRef = useRef<ServerMediaAudioSession | null>(null);
  const audioStartedRef = useRef(false);
  const relayStartedRef = useRef(false);
  const reconnectingAudioRef = useRef(false);
  const reconnectRetryRef = useRef<number | null>(null);
  const reconnectServerRelayAudioRef = useRef<() => void>(() => undefined);
  const serverMediaCleanupNeededRef = useRef(false);
  const lastSubscribedSourceIdsRef = useRef<string[]>([]);
  const link = useMemo(() => shareRoomUrl(roomId), [roomId]);
  const remoteUsers = useMemo(
    () => (room?.users ?? []).filter((user) => user.id !== currentUser?.id),
    [currentUser?.id, room?.users]
  );
  const subscribedSourceIds = useMemo(
    () => remoteUsers.filter((user) => !userAudio[user.id]?.muted).map((user) => user.id),
    [remoteUsers, userAudio]
  );

  const closeAudioSessions = useCallback(() => {
    serverAudioSessionRef.current?.close();
    serverAudioSessionRef.current = null;
  }, []);

  const clearReconnectRetry = useCallback(() => {
    if (reconnectRetryRef.current !== null) {
      window.clearTimeout(reconnectRetryRef.current);
      reconnectRetryRef.current = null;
    }
  }, []);

  useEffect(() => {
    let cancelled = false;

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
      const socket = createRoomSocket(roomId, session.user.id, session.accessToken);
      socketRef.current = socket;
      socket.onopen = () => {
        setStatus("Connected");
        setSocketOpen(true);
      };
      socket.onmessage = (event) => {
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
        setSocketOpen(false);
        clearRoomSession();
        setStatus("Disconnected");
      };
    }

    void enterRoom();

    return () => {
      cancelled = true;
      closeAudioSessions();
      audioStartedRef.current = false;
      relayStartedRef.current = false;
      reconnectingAudioRef.current = false;
      clearReconnectRetry();
      serverMediaCleanupNeededRef.current = false;
      lastSubscribedSourceIdsRef.current = [];
      setAudioStarted(false);
      setSocketOpen(false);
      clearRoomSession();
      socketRef.current?.close();
      socketRef.current = null;
    };
  }, [clearReconnectRetry, closeAudioSessions, roomId]);

  const connectServerRelayAudio = useCallback(async ({
    updateRelay,
    sourceIds = subscribedSourceIds,
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
      await updateMediaRelaySubscriptions(roomId, currentUser.id, sourceIds, accessToken);
      lastSubscribedSourceIdsRef.current = sourceIds;
      const socket = socketRef.current;
      if (!socket || socket.readyState !== WebSocket.OPEN) {
        throw new Error("Audio signalling websocket is not connected");
      }
      const session = new ServerMediaAudioSession({
        roomId,
        userId: currentUser.id,
        accessToken,
        socket,
        iceServers,
        stream,
        userAudio: audioSettings,
        onError: setStatus,
        onConnectionInterrupted: () => reconnectServerRelayAudioRef.current()
      });
      session.setMuted(muted);
      serverAudioSessionRef.current = session;
      stream = null;
      await session.start();
      clearReconnectRetry();
      setStatus("Server relay audio connected");
    } catch (error) {
      if (!updateRelay) {
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
      setStatus(error instanceof Error ? error.message : "Audio connection failed");
    }
  }, [accessToken, clearReconnectRetry, currentUser, muted, roomId, subscribedSourceIds, userAudio]);

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
    const nextSourceIds = remoteUsers
      .filter((user) => user.id !== userId || !next.muted)
      .filter((user) => user.id === userId || !userAudio[user.id]?.muted)
      .map((user) => user.id);
    try {
      await updateMediaRelaySubscriptions(roomId, currentUser.id, nextSourceIds, accessToken);
      lastSubscribedSourceIdsRef.current = nextSourceIds;
      const nextUserAudio = {
        ...userAudio,
        [userId]: next
      };
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
    if (!currentUser || !accessToken || !audioStartedRef.current) {
      return;
    }
    if (arraysEqual(lastSubscribedSourceIdsRef.current, subscribedSourceIds)) {
      return;
    }
    void (async () => {
      try {
        await updateMediaRelaySubscriptions(roomId, currentUser.id, subscribedSourceIds, accessToken);
        lastSubscribedSourceIdsRef.current = subscribedSourceIds;
        closeAudioSessions();
        await connectServerRelayAudio({ updateRelay: true });
      } catch (error) {
        setStatus(error instanceof Error ? error.message : "Failed to update audio subscription");
      }
    })();
  }, [accessToken, closeAudioSessions, connectServerRelayAudio, currentUser, roomId, subscribedSourceIds]);

  async function leave() {
    const shouldCloseServerMedia = serverMediaCleanupNeededRef.current && currentUser;
    clearReconnectRetry();
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
      <div className="rounded-md border border-[#d8ded6] bg-white">
        <div className="border-b border-[#d8ded6] px-4 py-3 text-sm font-semibold">Users</div>
        <ul className="divide-y divide-[#edf0ec]">
          {(room?.users ?? []).map((user) => (
            <li className="flex flex-wrap items-center justify-between gap-3 px-4 py-3 text-sm" key={user.id}>
              <span>{user.nickname}</span>
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
