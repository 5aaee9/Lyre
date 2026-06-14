"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { getIceServers, joinRoom, leaveRoom, shareRoomUrl, type RoomSnapshot, type UserProfile } from "@/lib/api";
import { MeshAudioSession } from "@/lib/mesh-audio";
import {
  createRoomSocket,
  reducePresence,
  type PresenceState,
  type SignalMessage
} from "@/lib/signalling";
import { readNickname, readNoiseConfig } from "@/lib/storage";
import { openLocalAudioStream } from "@/lib/webrtc";

export function RoomClient({ roomId }: { roomId: string }) {
  const [currentUser, setCurrentUser] = useState<UserProfile | null>(null);
  const [room, setRoom] = useState<RoomSnapshot | null>(null);
  const [status, setStatus] = useState("Joining");
  const socketRef = useRef<WebSocket | null>(null);
  const audioSessionRef = useRef<MeshAudioSession | null>(null);
  const audioStartedRef = useRef(false);
  const link = useMemo(() => shareRoomUrl(roomId), [roomId]);

  const handleSignal = useCallback(
    async (signal: SignalMessage) => {
      if (signal.payload.type === "offer" || signal.payload.type === "answer" || signal.payload.type === "ice-candidate") {
        if (!audioStartedRef.current || !audioSessionRef.current) {
          return;
        }
        await audioSessionRef.current.handleSignal(signal);
      }
      if (signal.payload.type === "user-joined" && audioStartedRef.current && audioSessionRef.current) {
        await audioSessionRef.current.connectToUsers([signal.payload.user]);
      }
      if (signal.payload.type === "user-left") {
        audioSessionRef.current?.removePeer(signal.payload.user_id);
      }
    },
    []
  );

  useEffect(() => {
    let cancelled = false;

    async function enterRoom() {
      const stored = sessionStorage.getItem("lyre.currentUser");
      let user = stored ? (JSON.parse(stored) as UserProfile) : null;
      if (!user) {
        const response = await joinRoom(roomId, { nickname: readNickname(), noise: readNoiseConfig() });
        user = response.user;
        setRoom(response.room);
        sessionStorage.setItem("lyre.currentUser", JSON.stringify(user));
      }
      if (cancelled) {
        return;
      }
      setCurrentUser(user);
      const socket = createRoomSocket(roomId, user.id);
      socketRef.current = socket;
      socket.onopen = () => setStatus("Connected");
      socket.onmessage = (event) => {
        const signal = JSON.parse(event.data as string) as SignalMessage;
        void handleSignal(signal);
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
      audioSessionRef.current?.close();
      audioSessionRef.current = null;
      audioStartedRef.current = false;
      socketRef.current?.close();
      socketRef.current = null;
    };
  }, [handleSignal, roomId]);

  async function connectAudio() {
    if (!currentUser) {
      return;
    }
    if (audioStartedRef.current) {
      return;
    }
    try {
      audioStartedRef.current = true;
      const iceServers = await getIceServers();
      const stream = await openLocalAudioStream();
      const session = new MeshAudioSession({
        roomId,
        currentUserId: currentUser.id,
        iceServers,
        stream,
        send: (message) => socketRef.current?.send(JSON.stringify(message)),
        onError: setStatus
      });
      audioSessionRef.current = session;
      const connected = await session.connectToUsers(room?.users ?? []);
      if (connected) {
        setStatus("Audio offers sent");
      }
    } catch (error) {
      audioStartedRef.current = false;
      audioSessionRef.current?.close();
      audioSessionRef.current = null;
      setStatus(error instanceof Error ? error.message : "Audio connection failed");
    }
  }

  async function leave() {
    audioSessionRef.current?.close();
    audioSessionRef.current = null;
    audioStartedRef.current = false;
    socketRef.current?.close();
    socketRef.current = null;
    if (currentUser) {
      await leaveRoom(roomId, currentUser.id);
    }
    sessionStorage.removeItem("lyre.currentUser");
    window.location.href = "/";
  }

  return (
    <section className="grid gap-5">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold">{roomId}</h1>
          <p className="mt-1 text-sm text-[#5c6a61]">{status}</p>
        </div>
        <div className="flex gap-2">
          <Button onClick={connectAudio}>Connect audio</Button>
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
