"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  closeServerMediaSession,
  getIceServers,
  joinRoom,
  leaveRoom,
  registerMediaTrack,
  shareRoomUrl,
  startMediaRelay,
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

export function RoomClient({ roomId }: { roomId: string }) {
  const [currentUser, setCurrentUser] = useState<UserProfile | null>(null);
  const [accessToken, setAccessToken] = useState<string | null>(null);
  const [room, setRoom] = useState<RoomSnapshot | null>(null);
  const [status, setStatus] = useState("Joining");
  const [audioStarted, setAudioStarted] = useState(false);
  const socketRef = useRef<WebSocket | null>(null);
  const serverAudioSessionRef = useRef<ServerMediaAudioSession | null>(null);
  const audioStartedRef = useRef(false);
  const serverMediaCleanupNeededRef = useRef(false);
  const link = useMemo(() => shareRoomUrl(roomId), [roomId]);

  const closeAudioSessions = useCallback(() => {
    serverAudioSessionRef.current?.close();
    serverAudioSessionRef.current = null;
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
      socket.onopen = () => setStatus("Connected");
      socket.onmessage = (event) => {
        const signal = JSON.parse(event.data as string) as SignalMessage;
        setRoom((current) => {
          const next: PresenceState = reducePresence({ room: current ?? undefined }, signal);
          if (next.error) {
            setStatus(next.error);
          }
          return next.room ?? current;
        });
      };
      socket.onclose = () => setStatus("Disconnected");
    }

    void enterRoom();

    return () => {
      cancelled = true;
      closeAudioSessions();
      audioStartedRef.current = false;
      serverMediaCleanupNeededRef.current = false;
      setAudioStarted(false);
      socketRef.current?.close();
      socketRef.current = null;
    };
  }, [closeAudioSessions, roomId]);

  async function connectAudio() {
    await connectServerRelayAudio();
  }

  async function connectServerRelayAudio() {
    if (!currentUser || !accessToken) {
      return;
    }
    if (audioStartedRef.current) {
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
      await startMediaRelay(roomId, noise, accessToken);
      cleanupNeeded = true;
      serverMediaCleanupNeededRef.current = true;
      await registerMediaTrack(roomId, currentUser.id, "audio-main", "audio", accessToken);
      const session = new ServerMediaAudioSession({
        roomId,
        userId: currentUser.id,
        accessToken,
        iceServers,
        stream,
        onError: setStatus
      });
      serverAudioSessionRef.current = session;
      stream = null;
      await session.start();
      setStatus("Server relay audio connected");
    } catch (error) {
      audioStartedRef.current = false;
      setAudioStarted(false);
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
      }
      setStatus(error instanceof Error ? error.message : "Audio connection failed");
    }
  }

  async function leave() {
    const shouldCloseServerMedia = serverMediaCleanupNeededRef.current && currentUser;
    closeAudioSessions();
    audioStartedRef.current = false;
    setAudioStarted(false);
    socketRef.current?.close();
    socketRef.current = null;
    if (currentUser && accessToken) {
      if (shouldCloseServerMedia) {
        await closeServerMediaSession(roomId, currentUser.id, accessToken);
        serverMediaCleanupNeededRef.current = false;
      }
      await leaveRoom(roomId, currentUser.id, accessToken);
    }
    sessionStorage.removeItem("lyre.roomSession");
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
          <Button disabled={audioStarted} onClick={connectAudio}>Connect audio</Button>
          <Button className="bg-[#7a2f2f]" onClick={leave}>Leave</Button>
        </div>
      </div>
      <div className="rounded-md border border-[#d8ded6] bg-white p-4">
        <div className="text-xs text-[#5c6a61]">Share</div>
        <div className="mt-1 break-all text-sm">{link}</div>
      </div>
      <div className="rounded-md border border-[#d8ded6] bg-white">
        <div className="border-b border-[#d8ded6] px-4 py-3 text-sm font-semibold">Users</div>
        <ul className="divide-y divide-[#edf0ec]">
          {(room?.users ?? []).map((user) => (
            <li className="flex items-center justify-between px-4 py-3 text-sm" key={user.id}>
              <span>{user.nickname}</span>
              <span className="text-xs text-[#5c6a61]">{user.noise.provider}</span>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}
