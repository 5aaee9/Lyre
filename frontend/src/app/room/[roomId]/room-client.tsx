"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { joinRoom, leaveRoom, shareRoomUrl, type RoomSnapshot, type UserProfile } from "@/lib/api";
import {
  createRoomSocket,
  encodeAnswer,
  encodeIceCandidate,
  encodeOffer,
  reducePresence,
  type PresenceState,
  type SignalMessage
} from "@/lib/signalling";
import { readNickname, readNoiseConfig } from "@/lib/storage";
import { createAudioPeerConnection } from "@/lib/webrtc";

export function RoomClient({ roomId }: { roomId: string }) {
  const [currentUser, setCurrentUser] = useState<UserProfile | null>(null);
  const [room, setRoom] = useState<RoomSnapshot | null>(null);
  const [status, setStatus] = useState("Joining");
  const socketRef = useRef<WebSocket | null>(null);
  const peerRef = useRef<RTCPeerConnection | null>(null);
  const audioStartedRef = useRef(false);
  const link = useMemo(() => shareRoomUrl(roomId), [roomId]);

  const handleSignal = useCallback(
    async (signal: SignalMessage, user: UserProfile) => {
      if (!audioStartedRef.current || !peerRef.current) {
        return;
      }
      if (signal.payload.type === "offer") {
        await peerRef.current.setRemoteDescription({ type: "offer", sdp: signal.payload.sdp });
        const answer = await peerRef.current.createAnswer();
        await peerRef.current.setLocalDescription(answer);
        socketRef.current?.send(
          JSON.stringify(encodeAnswer(roomId, user.id, answer.sdp ?? "", signal.sender_id))
        );
        setStatus("Audio answer sent");
      }
      if (signal.payload.type === "answer") {
        await peerRef.current.setRemoteDescription({ type: "answer", sdp: signal.payload.sdp });
        setStatus("Audio connected");
      }
      if (signal.payload.type === "ice-candidate") {
        await peerRef.current.addIceCandidate({
          candidate: signal.payload.candidate,
          sdpMid: signal.payload.sdp_mid,
          sdpMLineIndex: signal.payload.sdp_m_line_index
        });
      }
    },
    [roomId]
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
        void handleSignal(signal, user);
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
      socketRef.current?.close();
    };
  }, [handleSignal, roomId]);

  async function connectAudio() {
    if (!currentUser) {
      return;
    }
    try {
      audioStartedRef.current = true;
      const connection = await createAudioPeerConnection();
      peerRef.current = connection;
      connection.onicecandidate = (event) => {
        if (event.candidate) {
          socketRef.current?.send(JSON.stringify(encodeIceCandidate(roomId, currentUser.id, event.candidate.toJSON())));
        }
      };
      const offer = await connection.createOffer();
      await connection.setLocalDescription(offer);
      socketRef.current?.send(JSON.stringify(encodeOffer(roomId, currentUser.id, offer.sdp ?? "")));
      setStatus("Audio offer sent");
    } catch (error) {
      audioStartedRef.current = false;
      setStatus(error instanceof Error ? error.message : "Audio connection failed");
    }
  }

  async function leave() {
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
