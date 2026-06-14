# Processed Audio Egress Fanout Design

## Scope

This increment adds an internal processed-audio egress fanout contract in `lyre-web`. It does not implement WebRTC media termination, RTP/Opus packetization, browser playback, a public PCM API, or delivery over WebSocket/WebRTC data channels.

## Problem

`WebMediaRuntime` can process decoded PCM frames and publish room-scoped processed frames. The next server media-forwarding layer needs a deterministic fanout contract that converts each processed source frame into per-recipient egress events for the other registered room participants. That contract should be testable without real WebRTC transport.

## Design

Add a new `crates/lyre-web/src/media_egress.rs` module with:

- `ProcessedAudioEgressFrame`, a cloneable internal event containing the source `ProcessedAudioFrame` and the `recipient_id`.
- `ProcessedAudioEgressFanout`, a cloneable service that owns shared `Arc<MediaRelayRegistry>` and has `fanout(&ProcessedAudioFrame) -> Result<Vec<ProcessedAudioEgressFrame>, MediaRelayError>`.

Fanout rules:

- The source frame's room must have an active media relay.
- The source user and track must still be registered as an audio track. This uses the existing `MediaRelayRegistry::require_track` behavior.
- `fanout` calls `require_track`, then rejects a non-audio source track by returning `MediaRelayError::UnsupportedTrackKind` with the source room, user, track, and returned track kind.
- Recipients are all other participants currently registered in the media relay room with at least one audio track.
- The source user is excluded.
- Each recipient is returned at most once, even if they have multiple registered tracks.
- Recipient order is deterministic by `UserId`, matching relay snapshots.
- If the relay is inactive or the source participant/track is no longer registered, `fanout` returns the existing `MediaRelayError`.

Add a read-only participant listing method to `MediaRelayRegistry` only if needed by the fanout service. Because `crates/lyre-core/src/media.rs` is already over 400 lines, touching it in this increment requires moving the existing media unit tests into a separate `crates/lyre-core/src/media_tests.rs` module so the production file is brought back under the 400 line rule. New fanout behavior tests should stay in `lyre-web`.

Wire `AppState` with a `ProcessedAudioEgressFanout` sharing the same `MediaRelayRegistry` as the runtime, and expose an internal method:

```rust
pub fn processed_audio_egress_frames(
    &self,
    frame: &ProcessedAudioFrame,
) -> Result<Vec<ProcessedAudioEgressFrame>, MediaRelayError>
```

This method is for future WebRTC/SFU delivery code and tests only.

## Acceptance Criteria

- Given an active relay with a source user and two other audio-capable registered participants, a processed frame fans out to exactly the two other users.
- The source user never receives their own processed frame.
- A recipient with multiple tracks appears once.
- A participant with only video tracks does not receive processed-audio egress frames.
- Recipients are sorted deterministically by `UserId`.
- Inactive relay, unknown source participant, unknown source track, and non-audio source track errors are returned.
- Stopping the relay prevents future egress fanout for previously processed frames.
- No public REST/WebSocket/frontend behavior changes.

## Tests

Add focused Rust tests under `crates/lyre-web/src/` covering:

- multi-recipient fanout and source exclusion,
- recipient de-duplication when a user has multiple tracks,
- video-only participants are excluded from audio egress,
- deterministic recipient ordering,
- inactive or stale source frame errors,
- stop relay prevents later fanout.

Run full Rust verification and frontend verification after implementation.

## Documentation

Update `MEMORY.md` and `docs/roadmap.md`. The roadmap should mark the internal processed-audio egress fanout contract complete while keeping real WebRTC media termination and client delivery as future work.
