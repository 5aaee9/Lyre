# Mesh Audio Negotiation Design

## Goal

Harden the current browser WebRTC path so one room client can negotiate audio with multiple other room users at the same time.

## Context

Lyre already has room presence, WebSocket signalling, ICE server configuration, and a user-triggered `Connect audio` action. The backend signalling hub can broadcast messages or deliver targeted messages by `recipient_id`.

The current frontend room client owns a single `RTCPeerConnection`. That is enough for one remote peer, but it is not a real room mesh: a second remote peer would reuse the same connection, offers and answers can overwrite each other, and ICE candidates are not scoped to a specific remote participant.

This increment keeps the current peer-to-peer topology. It does not add SFU/media relay behavior, server-side media processing, audio device UI, authentication, or persistence.

## Scope

Implement frontend-side multi-peer mesh negotiation:

- Keep one local microphone stream per room client after the user clicks `Connect audio`.
- Keep one `RTCPeerConnection` per remote user id.
- Target all offer, answer, and ICE candidate messages to a specific remote user through the existing `recipient_id` field.
- When audio is already started and a new user joins, the current user creates a peer connection for that user and sends a targeted offer.
- When audio starts, the current user creates offers for all users currently present in the room except itself.
- When a targeted or broadcast offer arrives after audio has started, create or reuse the peer connection for the sender, set the remote offer, create an answer, and send the answer back to the sender.
- When a targeted answer arrives after audio has started, apply it only to that sender's connection.
- When a targeted or broadcast ICE candidate arrives after audio has started, apply it only to that sender's connection.
- When a remote user leaves, close and remove that user's peer connection.
- When the component unmounts or the user leaves the room, close all peer connections, stop local audio tracks, and close the socket.

## Non-Goals

- Real end-to-end browser audio verification in automated tests.
- Renegotiation after track changes.
- Perfect glare handling for simultaneous offer collisions.
- Multiple local audio devices or mute controls.
- Server-side relay, SFU, RNNoise, DeepFilterNet, Opus/PCM processing, or media broadcast.
- WebSocket protocol schema changes.

## Design

Add a small frontend mesh controller module, `frontend/src/lib/mesh-audio.ts`, that owns browser media objects outside React state:

- `MeshAudioSession`
  - constructed from `roomId`, `currentUserId`, ICE servers, local media stream, and a signal send callback
  - owns `Map<string, RTCPeerConnection>` keyed by remote user id
  - exposes `connectToUsers(users)`, `handleSignal(signal)`, `removePeer(userId)`, and `close()`

The React room client owns the session in a ref:

- `audioSessionRef` replaces the single `peerRef`.
- `audioStartedRef` continues to gate signal handling.
- `connectAudio()` fetches ICE servers, opens the microphone once, constructs `MeshAudioSession`, and connects to every current room user except the current user.
- Presence updates continue to use `reducePresence`, but `user-joined` and `user-left` also drive the mesh session when audio is already active.

`createAudioPeerConnection()` in `frontend/src/lib/webrtc.ts` should be split so tests and the mesh controller can open media once and create multiple peer connections from the same stream:

- `openLocalAudioStream(): Promise<MediaStream>`
- `createPeerConnection(iceServers, stream): RTCPeerConnection`
- keep `createAudioPeerConnection(iceServers)` as a small compatibility helper that opens one stream and creates one connection, if existing tests still use it.

## Message Direction

All locally generated offer, answer, and ICE candidate messages from the mesh controller must include `recipient_id`.

Incoming WebSocket messages are already delivered by the backend. The frontend must ignore media messages where `recipient_id` exists and does not match the current user. This prevents stale broadcast or misdelivered targeted messages from mutating the wrong peer.

Broadcast incoming media messages without `recipient_id` remain accepted for compatibility with existing clients and tests, but new frontend-generated messages are targeted.

## Ordering

For this increment, the initiator rule is simple:

- On audio start, connect to all existing remote users.
- On `user-joined`, an already-connected audio user initiates an offer to the new user.
- On receiving an offer, answer it.

This can create crossed offers if two clients start audio at the same time. Full WebRTC perfect-negotiation glare handling is explicitly future work.

## Error Handling

If ICE server loading or microphone access fails during `Connect audio`, do not create a mesh session and show the error message in the room status.

If a peer-specific operation fails after audio has started, show the error message in the room status and keep the rest of the mesh session alive. Do not silently swallow the error.

## Testing

Add Vitest coverage for:

- `openLocalAudioStream()` calls `navigator.mediaDevices.getUserMedia({ audio: true })`.
- `createPeerConnection()` builds `RTCPeerConnection` with the supplied ICE servers and adds local audio tracks.
- Starting audio with multiple room users creates one offer per remote user and sends targeted offers.
- Incoming offers create or reuse the sender connection and send targeted answers.
- ICE candidates are routed to the sender's connection, not a global connection.
- `user-joined` after audio start triggers a targeted offer to that new user.
- `user-left` closes that user's peer connection.
- Unmount closes all peer connections and stops local tracks.
- Signals before audio start are ignored.
- ICE server fetch failure still prevents microphone access.

## Acceptance Criteria

- A room client can maintain separate local `RTCPeerConnection` objects for multiple remote users.
- Frontend-generated media signalling is targeted with `recipient_id`.
- Presence changes add and remove mesh peers after audio start.
- Existing room join, leave, share link, settings, ICE server, and WebSocket signalling behavior remains intact.
- No backend WebSocket schema changes are required.
- Documentation records that multi-peer P2P mesh negotiation is hardened, while server-side media relay/noise processing remains future work.
