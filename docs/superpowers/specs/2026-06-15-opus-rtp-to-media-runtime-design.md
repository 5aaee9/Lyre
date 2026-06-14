# Opus RTP to Media Runtime Design

## Scope

This increment connects the existing server-side WebRTC audio RTP ingress boundary to Lyre's decoded-PCM media runtime. It decodes incoming Opus RTP payloads inside `crates/lyre-webrtc`, exposes decoded PCM through Lyre-owned DTOs, and lets `crates/lyre-web` drain those frames into the existing `WebMediaRuntime`.

This increment does not implement processed RTP/RTCP egress, browser playback of processed server audio, packet loss concealment, jitter buffering, resampling, mixing, or DeepFilterNet support. It also does not replace the current frontend P2P mesh path.

## Problem

The server can now negotiate an Opus-capable recvonly WebRTC path and record incoming audio RTP payloads, but the noise-cancelling runtime consumes decoded float PCM `AudioFrame` values. The missing boundary is a small, verified bridge that turns valid Opus RTP payloads from negotiated server media sessions into `AudioFrame` input for the existing server media runtime.

## Dependency Choice

Use `opus-rs = "0.1.22"` in `crates/lyre-webrtc`.

Reasons:

- It is a pure Rust Opus codec crate, avoiding a new system `libopus` requirement for Docker and CI.
- Crates.io reports BSD-3-Clause licensing.
- The current 0.1.22 API exposes `OpusDecoder::new(sampling_rate, channels)` and `decode(input, frame_size, output)` for float PCM output.

Keep `opus-rs` isolated inside `crates/lyre-webrtc`. Other crates must only see Lyre-owned PCM DTOs.

## Design

Add a Lyre-owned decoded PCM DTO:

```rust
pub struct ServerMediaPcmFrame {
    pub track_id: String,
    pub sequence_number: u16,
    pub rtp_timestamp: u32,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}
```

Extend the media ingress recorder to retain decoded PCM frames separately from raw RTP packet snapshots. Add a drain operation so `lyre-web` can process each decoded frame once:

```rust
pub fn drain_pcm_frames(&self) -> Vec<ServerMediaPcmFrame>
```

Decode only audio RTP packets for this increment:

- Create one Opus decoder per remote audio track.
- Configure the decoder for 48 kHz mono output.
- Decode valid RTP payloads with a 20 ms frame size of 960 samples per channel.
- Preserve the original RTP sequence number and timestamp in `ServerMediaPcmFrame`.
- Continue recording raw RTP packet snapshots for diagnostics.
- On decode failure, do not publish a PCM frame. The error should be observable in tests through a fallible decoder boundary, but malformed network packets must not crash the WebRTC receive task.

Add a small decoder wrapper in `lyre-webrtc` so tests can cover valid and invalid payload behavior without constructing a full peer connection:

```rust
pub struct ServerMediaOpusDecoder;

impl ServerMediaOpusDecoder {
    pub fn decode_packet(
        &mut self,
        packet: &ServerMediaRtpPacket,
    ) -> Result<ServerMediaPcmFrame, ServerMediaDecodeError>;
}
```

The wrapper is allowed to own `opus_rs::OpusDecoder`, but its public API must use only Lyre DTOs and Lyre errors.

Define `ServerMediaDecodeError` as a Lyre error that preserves the underlying `opus-rs` message as context:

```rust
pub enum ServerMediaDecodeError {
    InvalidDecoderConfig { message: String },
    Decode { message: String },
}
```

`opus-rs` currently returns `&'static str` errors. Store those messages in `message` instead of replacing them with a generic Lyre message. Do not name the field `source`, because these strings are context messages, not concrete `std::error::Error` sources. When the WebRTC receive task sees a decode error, record it in the same media ingress recorder as an internal `ServerMediaDecodeFailure` snapshot and log it with the original message. Do not expose decode failures through a public HTTP endpoint in this increment.

```rust
pub struct ServerMediaDecodeFailure {
    pub track_id: String,
    pub sequence_number: u16,
    pub rtp_timestamp: u32,
    pub error: String,
}

pub fn drain_decode_failures(&self) -> Vec<ServerMediaDecodeFailure>
```

Extend `ServerMediaNegotiator` with:

```rust
pub fn drain_pcm_frames(&self, key: &ServerMediaSessionKey) -> Vec<ServerMediaPcmFrame>
```

Extend `lyre-web::AppState` with:

```rust
pub fn drain_server_media_pcm_frames(
    &self,
    key: &ServerMediaSessionKey,
) -> Vec<ServerMediaPcmFrame>

pub fn process_server_media_pcm_frames(
    &self,
    key: &ServerMediaSessionKey,
) -> Result<usize, MediaRelayError>
```

`process_server_media_pcm_frames` drains decoded PCM frames for one server media session and converts each frame into `lyre_core::AudioFrame` using the session key's room/user and the PCM frame's track id, sample rate, channels, sequence number, and samples. The method returns the number of frames successfully submitted to `WebMediaRuntime`.

If `WebMediaRuntime::process_frame` returns `MediaRelayError`, stop processing and return that error. Because the method drains a snapshot before processing, the successfully processed frames, the failing frame, and any later frames in that drained batch are discarded from the server media session by design. This keeps the first bridge simple and avoids pretending to have retry semantics.

## Acceptance Criteria

- `opus-rs` is added only to `lyre-webrtc`.
- Direct `opus_rs`, `webrtc`, and `rtc` media/codec/RTP usage remains isolated inside `crates/lyre-webrtc`.
- `lyre-webrtc` exposes a Lyre-owned `ServerMediaPcmFrame` DTO and `ServerMediaDecodeError`.
- A valid Opus payload can be decoded into a 48 kHz mono float PCM frame through `ServerMediaOpusDecoder`.
- Invalid or empty payloads return a decode error and do not create PCM frames.
- Decode errors preserve the original `opus-rs` error message as context and are recorded as internal decode failure snapshots.
- The WebRTC audio RTP receive task records decoded PCM frames for successfully decoded audio RTP packets while preserving the existing raw RTP snapshot behavior.
- `WebRtcPeerConnectionHandle` and `ServerMediaNegotiator` expose drain methods for decoded PCM frames.
- Draining decoded PCM frames is one-shot: a second drain returns empty until more packets arrive.
- `lyre-web::AppState` can drain and process decoded server media PCM frames into the existing `WebMediaRuntime`.
- Processing decoded server media PCM frames requires an active media relay and a registered audio track, using existing `MediaRuntime` validation.
- If processing a drained decoded PCM batch returns `MediaRelayError`, the failing frame and later frames from that drained batch are discarded rather than requeued.
- No public REST endpoint exposes raw RTP or decoded PCM frames in this increment.
- Existing server media negotiation, ICE, RTP ingress, media runtime, and frontend tests continue to pass.

## Tests

Add focused Rust tests covering:

- `ServerMediaOpusDecoder` decodes a known-valid Opus payload generated by `opus-rs` test encoder or checked-in test fixture into a non-empty 48 kHz mono PCM frame.
- `ServerMediaOpusDecoder` rejects empty or malformed payloads with `ServerMediaDecodeError` preserving the original decoder error message as context.
- `MediaIngressRecorder` records and drains PCM frame and decode failure snapshots once.
- The real local WebRTC offerer test that sends Opus RTP eventually produces both raw RTP snapshots and decoded PCM frames.
- `ServerMediaNegotiator::drain_pcm_frames` returns empty for missing or closed sessions and drains frames once for an existing handle.
- `lyre-web::AppState::process_server_media_pcm_frames` processes decoded frames into `processed_media_frames` when the media relay is active and the audio track is registered.
- `lyre-web::AppState::process_server_media_pcm_frames` returns the existing `MediaRelayError` when the relay or track is missing, and tests assert the drained batch is not reprocessed on a second call.
- No public decoded PCM REST endpoint exists.

Run full verification after implementation:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`
- `cd frontend && npm run generate:webrpc && npm test -- --run && npm run typecheck && npm run lint && npm run build`
- `git diff --check`
- static dependency leak checks confirming direct `webrtc::`, `rtc::`, and `opus_rs::` usage remains isolated to `crates/lyre-webrtc`
- `cargo rustdoc -p lyre-webrtc --lib -- -D warnings`
- LOC checks confirming changed Rust files remain under 400 lines

## Documentation

Update `MEMORY.md` and `docs/roadmap.md`. Record that valid incoming Opus RTP can now be decoded to PCM and injected into the existing server media runtime, while packet loss handling, jitter buffering, processed RTP/RTCP egress, browser playback, and DeepFilterNet remain future work.

This increment must follow the repository's `$sdd-workflow`: independent spec review, independent plan review, implementation, independent implementation review, documentation update, fresh verification, Lore commit, and push.
