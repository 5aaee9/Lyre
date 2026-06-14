# Server Audio RTP Ingress Boundary Design

## Scope

This increment adds the first server-side WebRTC media ingress boundary for audio RTP. It lets `lyre-webrtc` configure negotiated server peer connections to receive browser audio tracks, observe remote audio tracks through the `webrtc` event handler, and record received RTP packet metadata and payload bytes behind Lyre-owned DTOs.

This increment does not implement Opus decode, decoded PCM conversion, RNNoise/DeepFilterNet ingestion from real WebRTC tracks, RTP/RTCP forwarding, processed audio browser playback, or switching the room UI from the current P2P mesh to server media relay.

## Problem

Lyre can negotiate server media WebRTC sessions and exchange ICE candidates, but there is still no verified boundary proving media packets can arrive at the Rust server. The next step toward server-side noise cancellation is to terminate the WebRTC receive path enough to register an audio transceiver, receive remote audio track events, and retain incoming RTP packets for the later Opus decode and PCM processing pipeline.

## Design

Keep all direct `webrtc` crate usage in `crates/lyre-webrtc`.

Add Lyre-owned media ingress DTOs:

```rust
pub struct ServerMediaRemoteTrack {
    pub track_id: String,
    pub kind: ServerMediaTrackKind,
    pub mime_type: Option<String>,
}

pub enum ServerMediaTrackKind {
    Audio,
    Video,
    Unknown,
}

pub struct ServerMediaRtpPacket {
    pub track_id: String,
    pub sequence_number: u16,
    pub timestamp: u32,
    pub marker: bool,
    pub payload_type: u8,
    pub payload: Vec<u8>,
}
```

Extend the `lyre-webrtc` peer connection handler so it:

1. Records local ICE candidates as it does today.
2. Records remote track metadata on `on_track`.
3. For audio tracks, spawns a task that polls `TrackRemoteEvent::OnRtpPacket` and records received RTP packet DTOs.
4. Ignores video RTP packets for this increment, while still recording that a video track was observed if one appears.

Register an Opus receive path before answering browser offers:

- Configure the `webrtc` `MediaEngine` with an Opus audio codec payload type 111, 48 kHz, 2 channels.
- Add a recvonly audio transceiver to server peer connections so browsers can negotiate a sendonly/sendrecv audio track.
- Keep the existing data-channel test offer helper working.

Expose only Lyre-owned methods on `WebRtcPeerConnectionHandle`:

```rust
pub fn remote_tracks(&self) -> Vec<ServerMediaRemoteTrack>
pub fn received_rtp_packets(&self) -> Vec<ServerMediaRtpPacket>
```

Extend `ServerMediaNegotiator` with room/user keyed query methods:

```rust
pub fn remote_tracks(&self, key: &ServerMediaSessionKey) -> Vec<ServerMediaRemoteTrack>
pub fn received_rtp_packets(&self, key: &ServerMediaSessionKey) -> Vec<ServerMediaRtpPacket>
```

Extend `lyre-web::AppState` with matching internal query methods for tests and later runtime wiring:

```rust
pub fn server_media_remote_tracks(&self, key: &ServerMediaSessionKey) -> Vec<ServerMediaRemoteTrack>
pub fn server_media_received_rtp_packets(&self, key: &ServerMediaSessionKey) -> Vec<ServerMediaRtpPacket>
```

Do not add public HTTP endpoints for raw RTP payloads in this increment. These are internal diagnostics and integration boundaries; exposing packet payloads over REST would be a debugging interface, not product behavior.

## Acceptance Criteria

- `lyre-webrtc` creates server peer connections with an audio recvonly transceiver and Opus codec support.
- Direct external `webrtc` media/track/RTP types stay isolated inside `lyre-webrtc`.
- `WebRtcPeerConnectionHandle` records remote track metadata behind Lyre-owned DTOs.
- `WebRtcPeerConnectionHandle` records incoming audio RTP packet metadata and payload bytes behind Lyre-owned DTOs.
- `ServerMediaNegotiator` exposes room/user keyed remote track and RTP packet query methods.
- `lyre-web::AppState` exposes internal remote track and RTP packet query methods for tests/future runtime wiring.
- Closing one session or a room removes the stored peer handle and therefore clears the queryable track/RTP packet data for that room/user.
- Existing offer/answer and ICE candidate behavior remains unchanged.
- No docs or API claims imply Opus decode, PCM processing, RNNoise/DeepFilterNet ingestion from WebRTC tracks, RTP/RTCP forwarding, or browser playback.

## Tests

Add focused Rust tests covering:

- A local `webrtc` offerer with an Opus audio track can negotiate with `WebRtcPeerConnectionHandle::answer_remote_offer`.
- After sending an RTP packet from the local offerer, `WebRtcPeerConnectionHandle::remote_tracks()` includes an audio track and `received_rtp_packets()` includes the sent packet metadata/payload.
- The public `lyre-webrtc` API exposes only Lyre-owned remote track/RTP DTOs, not concrete `webrtc` crate types.
- `ServerMediaNegotiator::remote_tracks` and `received_rtp_packets` are keyed by room/user and return empty for missing or closed sessions.
- `lyre-web::AppState` exposes the negotiator's remote track and RTP packet snapshots without adding public raw-packet REST endpoints.
- Existing server media offer and ICE candidate tests continue to pass.

Run full verification after implementation:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`
- `cd frontend && npm run generate:webrpc && npm test -- --run && npm run typecheck && npm run lint && npm run build`
- `git diff --check`
- static dependency leak checks confirming direct `webrtc::` usage remains isolated to `crates/lyre-webrtc`
- LOC checks confirming changed Rust files remain under 400 lines

## Documentation

Update `MEMORY.md` and `docs/roadmap.md`. Record that server audio RTP ingress exists, while Opus decode, decoded PCM processing, RNNoise/DeepFilterNet ingestion from real tracks, processed audio broadcast, and browser playback remain future work.

This increment must follow the repository's `$sdd-workflow`: independent spec review, independent plan review, implementation, independent implementation review, documentation update, fresh verification, Lore commit, and push.
