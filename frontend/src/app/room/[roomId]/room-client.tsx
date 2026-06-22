"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { RoomView } from "./room-view";
import {
  closeServerMediaSession,
  getMediaRelay,
  getIceServers,
  joinRoom,
  leaveRoom,
  registerMediaParticipant,
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
import { readAudioDeviceConfig, readAudioProcessingConfig, readNickname, readNoiseConfig } from "@/lib/storage";
import { useSettingsStore, type SettingsSnapshot, type UserAudioSettings } from "@/lib/settings-store";
import { VoiceActivityDetector } from "@/lib/voice-activity";
import { isMissingAudioInputError, openLocalAudioStream } from "@/lib/webrtc";

type RoomSession = {
  roomId: string;
  user: UserProfile;
  accessToken: string;
};

type ServerRelayAudioConnectionOptions = {
  updateRelay: boolean;
  sourceIds?: string[];
  audioSettings?: Record<string, UserAudioSettings>;
};

type RelayParticipantRefresh = {
  participantIds: string[];
  sourceIds: string[];
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
  const [listenOnly, setListenOnly] = useState(false);
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
  const listenOnlyRef = useRef(false);
  const relayStartedRef = useRef(false);
  const audioConnectionInFlightRef = useRef(false);
  const pendingAudioConnectionRef = useRef<ServerRelayAudioConnectionOptions | null>(null);
  const reconnectingAudioRef = useRef(false);
  const reconnectRetryRef = useRef<number | null>(null);
  const socketReconnectRetryRef = useRef<number | null>(null);
  const relaySourceRefreshRetryRef = useRef<number | null>(null);
  const reconnectServerRelayAudioRef = useRef<() => void>(() => undefined);
  const reconnectRoomSocketRef = useRef<() => void>(() => undefined);
  const serverMediaCleanupNeededRef = useRef(false);
  const lastSubscribedSourceIdsRef = useRef<string[]>([]);
  const appliedNoiseRef = useRef(readNoiseConfig());
  const appliedAudioProcessingRef = useRef(readAudioProcessingConfig());
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

  const refreshRelaySourceIds = useCallback(async (): Promise<RelayParticipantRefresh> => {
    if (!currentUser) {
      return { participantIds: [], sourceIds: [] };
    }
    const status = await getMediaRelay(roomId);
    const participantIds = status.participants
      .map((participant) => participant.user_id)
      .filter((userId) => userId !== currentUser.id);
    const sourceIds = status.participants
      .filter((participant) => participant.tracks.some((track) => track.kind === "audio"))
      .map((participant) => participant.user_id)
      .filter((userId) => userId !== currentUser.id);
    setRelaySourceIds((current) => arraysEqual(current, sourceIds) ? current : sourceIds);
    return { participantIds, sourceIds };
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
      listenOnlyRef.current = false;
      relayStartedRef.current = false;
      audioConnectionInFlightRef.current = false;
      pendingAudioConnectionRef.current = null;
      reconnectingAudioRef.current = false;
      reconnectRoomSocketRef.current = () => undefined;
      clearReconnectRetry();
      clearSocketReconnectRetry();
      clearRelaySourceRefreshRetry();
      serverMediaCleanupNeededRef.current = false;
      lastSubscribedSourceIdsRef.current = [];
      setRelaySourceIds([]);
      setAudioStarted(false);
      setListenOnly(false);
      setSocketOpen(false);
      clearRoomSession();
      socketRef.current?.close();
      socketRef.current = null;
    };
  }, [clearReconnectRetry, clearRelaySourceRefreshRetry, clearSocketReconnectRetry, closeAudioSessions, roomId]);

  const connectServerRelayAudio = useCallback(async (initialOptions: ServerRelayAudioConnectionOptions) => {
    if (!currentUser || !accessToken) {
      return;
    }
    if (audioStartedRef.current && !initialOptions.updateRelay) {
      return;
    }
    if (audioConnectionInFlightRef.current) {
      if (initialOptions.updateRelay) {
        pendingAudioConnectionRef.current = initialOptions;
      }
      return;
    }
    audioConnectionInFlightRef.current = true;
    try {
      let options: ServerRelayAudioConnectionOptions | null = initialOptions;
      while (options) {
        pendingAudioConnectionRef.current = null;
        const { updateRelay, sourceIds, audioSettings = userAudio } = options;
        options = null;
        let stream: MediaStream | null = null;
        let cleanupNeeded = false;
        try {
          audioStartedRef.current = true;
          setAudioStarted(true);
          const iceServers = await getIceServers();
          let listenOnlySession = false;
          try {
            stream = await openLocalAudioStream();
          } catch (error) {
            if (!isMissingAudioInputError(error)) {
              throw error;
            }
            stream = new MediaStream();
            listenOnlySession = true;
          }
          const noise = readNoiseConfig();
          const audioDevices = readAudioDeviceConfig();
          const shouldStartRelay = !updateRelay && !relayStartedRef.current;
          const shouldRegisterLocalMedia = shouldStartRelay || listenOnlyRef.current !== listenOnlySession;
          if (shouldStartRelay) {
            await startMediaRelay(roomId, noise, accessToken);
            cleanupNeeded = true;
            serverMediaCleanupNeededRef.current = true;
          }
          if (shouldRegisterLocalMedia) {
            if (listenOnlySession) {
              await registerMediaParticipant(roomId, currentUser.id, accessToken);
            } else {
              await registerMediaTrack(roomId, currentUser.id, "audio-main", "audio", accessToken);
            }
            relayStartedRef.current = true;
          }
          const activeRelayParticipants = await refreshRelaySourceIds();
          const waitingForRelaySources = remoteUsers.some((user) => !activeRelayParticipants.participantIds.includes(user.id));
          if (waitingForRelaySources) {
            scheduleRelaySourceRefreshRetry();
          } else {
            clearRelaySourceRefreshRetry();
          }
          const nextSourceIds = sourceIds
            ? sourceIds.filter((sourceId) => activeRelayParticipants.sourceIds.includes(sourceId))
            : subscribedSourceIdsFromRelaySources(activeRelayParticipants.sourceIds, audioSettings);
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
            listenOnly: listenOnlySession,
            outputDeviceId: audioDevices.outputDeviceId,
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
          if (listenOnlySession) {
            localVoiceActivityRef.current = null;
          } else {
            localVoiceActivityRef.current = new VoiceActivityDetector(stream, (speaking) => {
              setUserSpeaking(currentUser.id, speaking);
            });
            localVoiceActivityRef.current.start();
          }
          stream = null;
          clearReconnectRetry();
          setAudioDiagnosticsRefreshKey((key) => key + 1);
          listenOnlyRef.current = listenOnlySession;
          setListenOnly(listenOnlySession);
          setStatus(listenOnlySession ? "Listening without microphone" : "Server relay audio connected");
          appliedNoiseRef.current = noise;
          appliedAudioProcessingRef.current = readAudioProcessingConfig();
        } catch (error) {
          const message = error instanceof Error ? error.message : "Audio connection failed";
          const waitingForSocket = message === AUDIO_SIGNALLING_SOCKET_ERROR;
          if (!waitingForSocket) {
            listenOnlyRef.current = false;
            setListenOnly(false);
          }
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
        const queuedOptions = pendingAudioConnectionRef.current;
        pendingAudioConnectionRef.current = null;
        if (!queuedOptions || !audioStartedRef.current || socketRef.current?.readyState !== WebSocket.OPEN) {
          return;
        }
        closeAudioSessions();
        options = queuedOptions;
      }
    } finally {
      audioConnectionInFlightRef.current = false;
    }
  }, [
    accessToken,
    closeAudioSessions,
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
      if (audioConnectionInFlightRef.current) {
        pendingAudioConnectionRef.current = { updateRelay: true };
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
    if (listenOnly) {
      return;
    }
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
    if (
      JSON.stringify(appliedNoiseRef.current) === JSON.stringify(settings.noise) &&
      JSON.stringify(appliedAudioProcessingRef.current) === JSON.stringify(settings.audioProcessing)
    ) {
      return;
    }
    await updateMediaRelaySettings(roomId, currentUser.id, settings.noise, accessToken);
    if (audioConnectionInFlightRef.current) {
      pendingAudioConnectionRef.current = { updateRelay: true };
      return;
    }
    closeAudioSessions();
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
      : (await refreshRelaySourceIds()).sourceIds;
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
        const reconnectOptions = {
          updateRelay: true,
          sourceIds: nextSourceIds,
          audioSettings: nextUserAudio
        };
        if (audioConnectionInFlightRef.current) {
          pendingAudioConnectionRef.current = reconnectOptions;
          return;
        }
        closeAudioSessions();
        await connectServerRelayAudio(reconnectOptions);
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
        const activeRelayParticipants = await refreshRelaySourceIds();
        const nextSourceIds = subscribedSourceIdsFromRelaySources(activeRelayParticipants.sourceIds);
        const waitingForRelaySources = remoteUsers.some((user) => !activeRelayParticipants.participantIds.includes(user.id));
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
        if (audioConnectionInFlightRef.current) {
          pendingAudioConnectionRef.current = { updateRelay: true, sourceIds: nextSourceIds };
          return;
        }
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
    listenOnlyRef.current = false;
    relayStartedRef.current = false;
    audioConnectionInFlightRef.current = false;
    pendingAudioConnectionRef.current = null;
    setAudioStarted(false);
    setListenOnly(false);
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
    <RoomView
      accessToken={accessToken}
      audioDiagnosticsEnabled={audioDiagnosticsEnabled}
      audioDiagnosticsRefreshKey={audioDiagnosticsRefreshKey}
      audioStarted={audioStarted}
      listenOnly={listenOnly}
      currentUser={currentUser}
      link={link}
      loadAudioDiagnostics={loadAudioDiagnostics}
      muted={muted}
      onApplyUserAudioSettings={applyUserAudioSettings}
      onLeave={leave}
      onSaveSettings={saveSettings}
      onSettingsOpenChange={setSettingsOpen}
      onToggleMuted={toggleMuted}
      relaySourceIds={relaySourceIds}
      room={room}
      roomId={roomId}
      settingsOpen={settingsOpen}
      speakingUserIds={speakingUserIds}
      status={status}
      subscribedSourceIds={subscribedSourceIds}
      userAudio={userAudio}
    />
  );
}

function arraysEqual(left: string[], right: string[]): boolean {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}
