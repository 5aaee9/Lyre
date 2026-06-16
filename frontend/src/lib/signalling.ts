import type { RoomSnapshot, ServerMediaIceCandidate, UserProfile } from "./api";
import { apiBaseUrl } from "./api";

export type SignalPayload =
  | { type: "offer"; sdp: string }
  | { type: "answer"; sdp: string }
  | { type: "ice-candidate"; candidate: string; sdp_mid?: string; sdp_m_line_index?: number }
  | {
      type: "server-media-ice-candidate";
      candidate: string;
      sdp_mid?: string | null;
      sdp_mline_index?: number | null;
      username_fragment?: string | null;
    }
  | { type: "server-media-ice-candidates-request" }
  | { type: "server-media-ice-candidates"; candidates: ServerMediaIceCandidate[] }
  | { type: "user-joined"; user: UserProfile }
  | { type: "user-left"; user_id: string }
  | { type: "room-snapshot"; room: RoomSnapshot }
  | { type: "error"; message: string };

export type SignalMessage = {
  type: SignalPayload["type"];
  room_id: string;
  sender_id: string;
  recipient_id?: string;
  payload: SignalPayload;
};

export type PresenceState = {
  room?: RoomSnapshot;
  error?: string;
};

export function roomSocketUrl(roomId: string, userId: string, accessToken: string): string {
  const url = new URL(apiBaseUrl());
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  url.pathname = `/api/rooms/${encodeURIComponent(roomId)}/ws`;
  url.search = new URLSearchParams({ user_id: userId, access_token: accessToken }).toString();
  return url.toString();
}

export function createRoomSocket(roomId: string, userId: string, accessToken: string): WebSocket {
  return new WebSocket(roomSocketUrl(roomId, userId, accessToken));
}

export function encodeOffer(roomId: string, senderId: string, sdp: string, recipientId?: string): SignalMessage {
  return message(roomId, senderId, recipientId, { type: "offer", sdp });
}

export function encodeAnswer(roomId: string, senderId: string, sdp: string, recipientId?: string): SignalMessage {
  return message(roomId, senderId, recipientId, { type: "answer", sdp });
}

export function encodeIceCandidate(
  roomId: string,
  senderId: string,
  candidate: RTCIceCandidateInit,
  recipientId?: string
): SignalMessage {
  return message(roomId, senderId, recipientId, {
    type: "ice-candidate",
    candidate: candidate.candidate ?? "",
    sdp_mid: candidate.sdpMid ?? undefined,
    sdp_m_line_index: candidate.sdpMLineIndex ?? undefined
  });
}

export function encodeServerMediaIceCandidate(
  roomId: string,
  senderId: string,
  candidate: RTCIceCandidateInit
): SignalMessage {
  return message(roomId, senderId, senderId, {
    type: "server-media-ice-candidate",
    candidate: candidate.candidate ?? "",
    sdp_mid: candidate.sdpMid ?? null,
    sdp_mline_index: candidate.sdpMLineIndex ?? null,
    username_fragment: candidate.usernameFragment ?? null
  });
}

export function encodeServerMediaIceCandidatesRequest(roomId: string, senderId: string): SignalMessage {
  return message(roomId, senderId, senderId, { type: "server-media-ice-candidates-request" });
}

export function reducePresence(state: PresenceState, signal: SignalMessage): PresenceState {
  const payload = signal.payload;
  switch (payload.type) {
    case "room-snapshot":
      return { ...state, room: payload.room };
    case "user-joined": {
      const users = state.room?.users.filter((user) => user.id !== payload.user.id) ?? [];
      return state.room
        ? { ...state, room: { ...state.room, users: [...users, payload.user] } }
        : state;
    }
    case "user-left":
      return state.room
        ? {
            ...state,
            room: {
              ...state.room,
              users: state.room.users.filter((user) => user.id !== payload.user_id)
            }
          }
        : state;
    case "error":
      return { ...state, error: payload.message };
    case "offer":
    case "answer":
    case "ice-candidate":
    case "server-media-ice-candidate":
    case "server-media-ice-candidates-request":
    case "server-media-ice-candidates":
      return state;
  }
}

function message(
  roomId: string,
  senderId: string,
  recipientId: string | undefined,
  payload: SignalPayload
): SignalMessage {
  return {
    type: payload.type,
    room_id: roomId,
    sender_id: senderId,
    recipient_id: recipientId,
    payload
  };
}
