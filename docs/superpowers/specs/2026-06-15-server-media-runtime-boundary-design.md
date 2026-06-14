# Server Media Runtime Boundary Design

## Goal

Add a tested server-side media runtime boundary that can accept decoded audio frames, run the configured noise processor, and deliver processed frames to a sink abstraction.

## Context

Lyre now has:

- a browser P2P mesh audio path for current calls,
- a media relay REST/WebRPC state skeleton,
- an explicit topology API that says server-side audio processing is not active,
- a `lyre-noise-cancelling` crate with a `NoiseCanceller` trait and passthrough implementation.

The next production step is a real WebRTC media relay/SFU-like runtime, but adding actual WebRTC termination, RTP/RTCP, DTLS-SRTP, Opus decode/encode, jitter buffers, and congestion control is too large for one safe increment. This increment creates the narrow runtime boundary that a future WebRTC stack will call after it has decoded audio to PCM.

## Scope

Implement a core server media runtime boundary in `lyre-core`:

- Add `crates/lyre-core/src/media_runtime.rs`.
- Define `AudioFrame` as decoded PCM frame metadata:
  - `room_id`
  - `user_id`
  - `track_id`
  - `sample_rate_hz`
  - `channels`
  - `sequence`
  - `samples`
- Define `ProcessedAudioFrame` with the same routing metadata plus processed samples and `noise` config used.
- Define `AudioFrameProcessor` trait:
  - `fn process(&self, frame: &AudioFrame, noise: &NoiseCancellationConfig) -> Vec<f32>`
- Define `PassthroughAudioFrameProcessor` for tests and current runtime.
- Define `ProcessedAudioSink` trait:
  - `fn publish(&self, frame: ProcessedAudioFrame)`
- Define `MediaRuntime`:
  - constructed from `Arc<MediaRelayRegistry>`, processor, and sink
  - `process_frame(frame)` checks the media relay room is active through a read-only relay lookup
  - finds the participant/track registered in `MediaRelayRegistry` without creating or mutating room state
  - accepts only tracks registered as `MediaTrackKind::Audio`
  - runs the processor with the room's intended noise config
  - publishes a processed frame to the sink
  - returns errors for inactive relay, unknown participant, unknown track, and non-audio track
- Add read-only relay lookup support to `MediaRelayRegistry`:
  - query active state without creating a room
  - return the room noise config for active relay rooms
  - verify a participant and track without mutating state
  - expose the registered `MediaTrackKind` needed by the runtime
- Add `RecordingProcessedAudioSink` test helper or equivalent test-only sink.

## Non-Goals

- No WebRTC media termination.
- No RTP/RTCP, DTLS-SRTP, ICE transport, Opus decode/encode, jitter buffering, packet loss handling, or congestion control.
- No server-side broadcast over WebSocket or WebRTC; the sink is only an internal boundary.
- No native RNNoise or DeepFilterNet bindings.
- No frontend behavior changes.
- No claim that `/api/webrtc/topology` has active server-side audio processing.
- No async runtime, worker pool, backpressure, or stream scheduling.

## Behavior

`MediaRuntime::process_frame(frame)` is a synchronous boundary for already-decoded PCM frames.

If the relay room is inactive, return the existing inactive relay error.

If the room is active but the frame's `user_id` is not registered as a relay participant, return:

```rust
MediaRelayError::ParticipantNotFound {
    room_id,
    user_id,
}
```

If the participant exists but `track_id` is not registered, return:

```rust
MediaRelayError::TrackNotFound {
    room_id,
    user_id,
    track_id,
}
```

If the participant and track exist but the track is not registered as `MediaTrackKind::Audio`, return:

```rust
MediaRelayError::UnsupportedTrackKind {
    room_id,
    user_id,
    track_id,
    kind,
}
```

If the participant and track exist, call the configured processor with the frame and current room noise config, then publish a `ProcessedAudioFrame` containing:

- original routing metadata,
- processed samples,
- the noise config used.

The runtime must not mutate media relay state while processing a frame. Unknown rooms must remain unknown after a failed read-only processing attempt.

## Data Ownership

The runtime lives in `lyre-core` because it coordinates `RoomId`, `UserId`, relay state, and core audio frame DTOs. It must not depend on `lyre-noise-cancelling`, because that crate already depends on `lyre-core`; future integration can adapt `lyre-noise-cancelling::NoiseCanceller` behind `AudioFrameProcessor` in another crate or at the web runtime boundary.

## Tests

Add focused Rust unit tests:

- inactive relay rejects frame processing with `MediaRelayError::Inactive`.
- active relay rejects unknown participant.
- active relay rejects unknown track.
- active relay rejects a registered video track as `UnsupportedTrackKind`.
- active relay processes registered audio track and publishes exactly one processed frame.
- processor receives the noise config selected when the relay was started.
- published frame preserves room/user/track/sample metadata and uses processed samples.
- read-only processing for an unknown room does not create a media relay room entry.

## Acceptance Criteria

- `lyre-core` exposes a media runtime boundary for decoded audio frames.
- Frame processing is gated by active media relay state and registered participant/track metadata.
- Processing uses the room's intended noise configuration.
- Processed frames are delivered to a sink abstraction.
- Current topology/API still reports no active server-side audio processing.
- `MEMORY.md` records that this is a decoded-PCM runtime boundary, not real WebRTC termination or RNNoise/DeepFilterNet processing yet.
- `docs/roadmap.md` moves the runtime boundary to Completed while keeping real WebRTC termination, RNNoise binding, DeepFilterNet binding, and real broadcast in Next.
