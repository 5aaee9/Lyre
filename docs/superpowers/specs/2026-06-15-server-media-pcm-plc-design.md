# Server Media PCM Packet Loss Concealment Design

## Scope

Add Lyre-owned PCM packet loss concealment for server-media ingress after jitter-buffer loss detection.

This increment covers:

- Creating a small PCM concealment synthesizer inside `lyre-webrtc`.
- Keeping the current jitter buffer as the source of missing-sequence events.
- Generating a deterministic 48 kHz mono 960-sample replacement PCM frame for missing packets once a previous decoded frame exists.
- Feeding synthesized frames into the same internal PCM frame path as decoded Opus frames.
- Preserving internal decode-failure recording when no prior frame exists.
- Keeping REST, WebRPC, frontend, Docker, and GitHub Actions unchanged.

This increment does not implement Opus-native PLC or FEC. The current `opus-rs` decoder path still does not expose a PLC/FEC API. The synthesized output is a Lyre PCM fallback based on the last decoded PCM frame; it must not be described as Opus decoder PLC.

## Current Behavior

`ServerMediaJitterBuffer` emits `ConcealmentRequired` events for missing RTP sequence numbers after the configured jitter depth is exceeded.

`stack_audio_ingress::handle_audio_rtp_packet` currently converts each event into a `ServerMediaDecodeFailure` with:

```text
packet loss concealment required but not available with current Opus decoder
```

That makes loss visible in tests and internal snapshots, but no PCM reaches the server media runtime for lost packets.

## Proposed Behavior

Add `ServerMediaPcmConcealer` in `lyre-webrtc`.

The concealer should:

- Track the most recent successfully emitted `ServerMediaPcmFrame`.
- Accept a `ServerMediaConcealmentRequired` event.
- Return `None` if no prior PCM frame exists.
- Return a synthetic `ServerMediaPcmFrame` when a prior frame exists.
- Set the synthetic frame fields from the loss event and previous frame:
  - `track_id`: event track id.
  - `sequence_number`: event sequence number.
  - `rtp_timestamp`: event RTP timestamp.
  - `sample_rate_hz`: previous frame sample rate.
  - `channels`: previous frame channels.
  - `samples`: 960 mono samples for the current Lyre Opus frame shape.
- Use the last decoded/synthesized frame as state for later concealment events.

The synthesis algorithm is intentionally simple and deterministic:

- Take the most recent frame's samples.
- Reverse the last frame so the replacement begins near the prior frame tail.
- Apply a linear fade-out from `0.60` on the first sample to `0.0` on the final sample.
- Clamp resulting samples to `[-1.0, 1.0]`.
- If the previous frame sample count is shorter than the current Opus frame size, repeat from the reversed previous samples until 960 samples are filled.
- If the previous frame sample count is longer than 960, use only the last 960 samples before reversing.

Because Lyre's server-media Opus decode path is currently 48 kHz mono with `SERVER_MEDIA_OPUS_FRAME_SIZE == 960` and `SERVER_MEDIA_OPUS_CHANNELS == 1`, this increment should only synthesize for that frame shape. If the previous frame has a different sample rate, channel count, or empty samples, the concealer should return `None` and the existing decode-failure path should record the loss.

## Integration

In `stack_audio_ingress`:

- Instantiate a `ServerMediaPcmConcealer` per audio track task.
- When a real packet decodes successfully, record the PCM frame and update the concealer with that frame.
- When a concealment event arrives:
  - Ask the concealer to synthesize a PCM frame.
  - If synthesis succeeds, record the synthetic PCM frame instead of a decode failure.
  - If synthesis fails because no usable prior frame exists, keep recording the existing decode failure message.
- When decode of a real packet fails, do not update the concealer.

`WebRtcStack::on_track` should keep owning one jitter buffer and one concealer per audio track task. Keep the new state inside the spawned task so concurrent tracks do not share concealment state.

No public API or frontend behavior changes are required.

## Testing

Add unit tests in `lyre-webrtc` for the PCM concealer:

- No prior frame returns `None`.
- A prior 960-sample mono frame produces a synthetic frame with the event sequence number and timestamp.
- The synthetic samples have length 960 and fade toward zero.
- Multi-loss events can use the prior synthetic frame as the next concealment seed.
- Invalid prior frame shape returns `None` and does not update concealment output.

Add stack-level tests proving:

- A single missing RTP packet after one decoded packet produces a PCM frame for the missing sequence instead of only a decode failure.
- Multiple missing packets produce multiple PCM frames with incrementing deterministic RTP timestamps.
- If loss happens before any decoded frame baseline exists, the server still records the existing concealment-unavailable decode failure.
- Malformed real Opus packets still record decoder failures and do not seed PLC state.

Tests should not assert audio quality. They should assert deterministic shape, metadata, non-empty output, and fallback behavior.

## Documentation

After implementation review approval:

- Update `MEMORY.md` to record that Lyre now has deterministic PCM fallback concealment for server-media ingress, not Opus-native PLC/FEC.
- Update `docs/roadmap.md`:
  - Move real PCM packet loss concealment synthesis to Completed.
  - Keep full DeepFilterNet model inference, client-side Rust WASM noise cancellation, auth, persistence, observability, and generated WebRPC Rust runtime in Next.

## Acceptance Criteria

- Missing RTP packets after a decoded baseline produce synthetic PCM frames with deterministic sequence numbers and timestamps.
- Synthetic frames match the existing server-media PCM frame shape: 48 kHz, mono, 960 samples.
- Missing packets before any usable prior PCM frame still produce the existing internal decode failure.
- Malformed real packets still produce decoder failures and do not seed the concealer.
- The implementation does not claim Opus-native PLC or FEC.
- No REST, WebRPC, frontend, packaging, or GitHub Actions behavior changes.
- Rust formatting, clippy, and workspace tests pass.
