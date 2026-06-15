# Processed Audio WebRTC Egress Design

## Scope

This increment sends server-processed audio frames back toward negotiated server-media WebRTC peers. Lyre already negotiates browser-to-server audio, decodes incoming Opus RTP to PCM, runs the server media runtime, and publishes processed PCM frames internally. This increment adds the first internal egress path that encodes those processed PCM frames to Opus RTP and writes them to a server-owned WebRTC audio track for recipient peers in the same room.

This increment does not switch the frontend from mesh mode to server-media mode, add browser playback UI, add jitter buffering, packet loss concealment, DeepFilterNet, simulcast, adaptive bitrate, or public RTP/PCM debug endpoints.

## Problem

The current server-media path stops after processed frames are stored and broadcast inside `lyre-web`. `ProcessedAudioEgressFanout` can decide which participants should receive a processed frame, but nothing packetizes or writes the frame to a WebRTC peer. That means server-side noise cancellation is not yet deliverable to clients through the media path.

## Design

Add a minimal WebRTC egress boundary in `lyre-webrtc`, then connect it from `lyre-web`.

### WebRTC Egress Track

Extend `WebRtcPeerConnectionHandle` so each server-media peer connection owns one server-to-client local Opus audio track. The existing PeerConnection should be configured for bidirectional audio rather than receive-only:

- keep receiving browser audio as today,
- add a local static RTP Opus track with payload type 111,
- expose a Lyre-owned method to write already-encoded or encoded-from-PCM RTP packets to that local track.

Do not expose direct `webrtc`, `rtc`, or `TrackLocalStaticRTP` types outside `lyre-webrtc`.

Add Lyre-owned egress DTOs and helpers:

```rust
pub struct ServerMediaProcessedAudioFrame {
    pub sequence: u64,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

pub struct ServerMediaEgressRtpPacket {
    pub sequence_number: u16,
    pub timestamp: u32,
    pub payload_type: u8,
    pub payload: Vec<u8>,
}
```

`WebRtcPeerConnectionHandle::send_processed_audio_frame` should:

- require 48 kHz mono PCM,
- require non-empty samples whose length is compatible with the existing 20 ms Opus frame size,
- encode one 20 ms frame at a time with `opus-rs`,
- generate RTP sequence numbers and timestamps internally per peer,
- write RTP packets to the local audio track,
- return a count or snapshot of sent packets for tests.

Keep the implementation small. A per-peer `Mutex` around encoder/sequence/timestamp state is acceptable for this increment because writes are local to the peer handle.

### Negotiator Integration

Extend `ServerMediaNegotiator` with a method that sends a processed frame to one session key:

```rust
pub async fn send_processed_audio_frame(
    &self,
    key: &ServerMediaSessionKey,
    frame: ServerMediaProcessedAudioFrame,
) -> Result<usize, ServerMediaEgressError>;
```

If the peer handle is missing, return an error that preserves the session key context. Do not silently drop frames.

### Web Runtime Egress Pump

Add a `ProcessedAudioWebRtcEgressPump` in `crates/lyre-web`. It subscribes to processed frames for a room, uses the existing `ProcessedAudioEgressFanout` to calculate recipients, and sends each processed frame to each recipient's negotiated server-media peer through `ServerMediaNegotiator`.

Lifecycle:

- Start the room egress pump when a media relay starts for a room.
- Replace an existing room pump if the relay is restarted.
- Stop the room egress pump when the media relay stops or server-media sessions for the room are closed.
- Use `tokio_util::sync::CancellationToken` for graceful cancellation, matching the server media runtime pump.

The pump should keep running after per-frame fanout or send errors. Log errors with room id, source user id, recipient user id when known, and the complete error chain (`{error:#}` or equivalent).

### AppState Integration

`AppState` should own:

```rust
pub processed_audio_webrtc_egress_pump: Arc<ProcessedAudioWebRtcEgressPump>
```

`AppState::start_media_relay` should start or replace the room egress pump after the relay is active. The existing route handler should call this AppState method instead of directly calling `media_relays.start`.

`AppState::stop_media_relay` and `AppState::close_server_media_sessions_for_room` should cancel the room egress pump. Stop the egress pump before clearing processed frame room state so the pump cannot continue consuming a room that has been stopped.

The public product API should not expose egress pump counts or raw RTP. Tests may use `#[cfg(test)]` internal counters or sent-packet snapshots.

## Error Handling

Use explicit error types for egress:

- invalid PCM shape,
- Opus encoder initialization or encode failure,
- RTP track write failure,
- missing peer session.

Do not flatten lower-level causes. Logs crossing runtime task boundaries should include the full error chain. HTTP response shapes do not change in this increment.

## Acceptance Criteria

- Negotiated server-media peer connections include a local server-to-client Opus audio track while preserving current incoming Opus RTP decode behavior.
- A processed 48 kHz mono 20 ms frame can be encoded and written to the server egress track for a peer.
- `ProcessedAudioWebRtcEgressPump` subscribes to processed frames and sends them to active audio-capable room recipients using existing fanout rules.
- Source users do not receive their own processed frame.
- Missing recipient peer handles produce logged send errors but do not stop the room egress pump.
- Starting a media relay starts/replaces the room egress pump; stopping a relay or closing server-media sessions stops it.
- No public REST endpoint exposes egress pump state, RTP packets, decoded PCM, or encode failures.
- Existing server-media ingress, RNNoise processing, frontend, packaging, and CLI tests continue to pass.
- Changed Rust files remain below 400 LOC. Split egress pump, WebRTC egress, and tests into focused modules if needed.

## Tests

Add focused tests covering:

- `lyre-webrtc` can send a valid processed PCM frame through a negotiated peer and records test-visible sent RTP metadata without exposing public raw RTP endpoints.
- Invalid processed PCM shape returns an egress error with context.
- `ServerMediaNegotiator` routes processed frames to the requested session and errors on missing session.
- `ProcessedAudioWebRtcEgressPump` starts/replaces/stops by room and uses cancellation tokens.
- AppState starts egress pumping when media relay starts and stops it on relay stop/session close.
- A real processed frame from server-media ingress/RNNoise is fanned out to a second negotiated server-media peer.
- No public egress pump/RTP/encode-failure debug route exists.

Run:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`
- `cd frontend && npm run generate:webrpc && npm test -- --run && npm run typecheck && npm run lint && npm run build`
- `git diff --check`
- LOC checks for changed Rust files

## Documentation

Update `MEMORY.md` and `docs/roadmap.md` after implementation approval. Record that processed server audio now has an internal WebRTC egress path, while frontend server-media mode/browser playback, jitter buffering, packet loss concealment, and DeepFilterNet remain future work.
