# RNNoise Opus Frame Alignment Design

## Scope

This increment makes the existing server-side RNNoise runtime process decoded Opus PCM frames produced by real server-media WebRTC ingress. The previous increment decodes incoming Opus RTP into 48 kHz mono PCM frames with 960 samples per packet. `nnnoiseless` RNNoise processes 48 kHz mono frames in 480-sample chunks, so the current runtime treats the real decoded Opus frame shape as invalid and falls back to passthrough.

This increment only adapts RNNoise frame sizing. It does not implement DeepFilterNet, jitter buffering, packet loss concealment, processed RTP/RTCP egress, browser playback of processed server audio, or a background media pump.

## Problem

Lyre can now decode real incoming server-media Opus RTP into PCM and manually feed those frames into `WebMediaRuntime`, but RNNoise does not actually process those decoded frames because they are 960 samples long. This blocks the next server-side noise-cancellation path: real WebRTC media reaches the runtime, yet RNNoise-compatible noise cancellation is bypassed for the exact frame shape produced by the Opus decode bridge.

## Design

Keep the existing public `NoiseCanceller` API unchanged:

```rust
pub trait NoiseCanceller: Send {
    fn process_frame(
        &mut self,
        frame: NoiseFrame<'_>,
    ) -> Result<NoiseFrameOutput, NoiseCancellationError>;
}
```

Update `RnnoiseNoiseCanceller` so it accepts any non-empty 48 kHz mono frame whose sample count is an exact multiple of `RNNOISE_FRAME_SIZE` (480). It should process the input in consecutive 480-sample chunks through the same `DenoiseState`, concatenate the processed chunks, and return one `NoiseFrameOutput` with the same number of samples as input.

Voice activity metadata remains a single optional value. For multi-chunk inputs, return the arithmetic mean of the per-chunk RNNoise VAD values. For one 480-sample chunk, behavior remains equivalent to the current implementation.

Invalid RNNoise input should still return `NoiseCancellationError::InvalidFrameShape`, preserving the existing error type. The error should report:

- the actual sample count,
- expected sample rate and channels,
- `expected_samples: RNNOISE_FRAME_SIZE`,
- and a message that makes the multiple-of-480 requirement clear.

The `NoiseCancellingAudioFrameProcessor` adapter should not change its public behavior. For RNNoise-compatible 960-sample decoded Opus frames, it should now return processed 960-sample output instead of logging and passing through. Unsupported DeepFilterNet should still pass through with warning logs at the adapter boundary until a real backend is implemented.

`crates/lyre-noise-cancelling/src/lib.rs` is already close to the repository's 400 LOC limit. This increment must split tests or implementation into smaller Rust modules as part of the change. A minimal acceptable split is to move the existing `#[cfg(test)]` tests into `crates/lyre-noise-cancelling/src/tests.rs` and declare it from `lib.rs`; production APIs should remain exported from the crate root. Every changed Rust file must remain under 400 lines.

## Acceptance Criteria

- RNNoise still processes 480-sample, 48 kHz mono frames and returns 480 output samples.
- RNNoise processes 960-sample, 48 kHz mono frames by running two consecutive 480-sample chunks through the same denoise state and returns 960 output samples.
- Multi-chunk RNNoise output is not byte-for-byte identical to the input for a non-silent test frame, proving the RNNoise path was used instead of passthrough.
- Multi-chunk RNNoise output returns `Some(vad)` with a value in `0.0..=1.0`.
- RNNoise rejects wrong sample rate, wrong channel count, empty samples, and non-multiple-of-480 sample counts with `NoiseCancellationError::InvalidFrameShape`.
- `NoiseCancellingAudioFrameProcessor` processes a 960-sample, 48 kHz mono `AudioFrame` with `NoiseProvider::Rnnoise` and returns 960 samples, not passthrough.
- The existing `lyre-web` real server-media AppState test covers RNNoise processing for a decoded 960-sample Opus frame by starting the media relay with RNNoise, registering the decoded track, processing drained server-media PCM frames, and asserting the processed frame has `NoiseProvider::Rnnoise`, 960 samples, and samples that differ from the decoded PCM input.
- DeepFilterNet remains unsupported and is not silently advertised as implemented.
- No public API surface changes are required.
- `crates/lyre-noise-cancelling` is split so all changed Rust files stay under 400 LOC.
- Existing Rust and frontend tests continue to pass.

## Tests

Add focused Rust tests:

- `rnnoise_processes_960_sample_mono_frame_in_chunks_and_reports_average_vad`
- `rnnoise_rejects_empty_or_non_multiple_frame_size`
- `audio_frame_processor_adapter_uses_rnnoise_for_decoded_opus_frame`
- Extend the existing real `lyre-web` server media runtime test to start the relay with RNNoise and assert the processed server-media frame is no longer passthrough.
- Include a LOC check for `crates/lyre-noise-cancelling/src/lib.rs`, the new split test/module file, and changed `lyre-web` test files.

Run:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`
- `cd frontend && npm run generate:webrpc && npm test -- --run && npm run typecheck && npm run lint && npm run build`
- `git diff --check`
- LOC checks for changed Rust files

## Documentation

Update `MEMORY.md` and `docs/roadmap.md` after implementation approval. Record that server-side RNNoise now handles real decoded 20 ms Opus PCM frames by chunking them into RNNoise frames, while DeepFilterNet, automatic server-media pumping, jitter buffering, processed RTP/RTCP egress, and browser playback remain future work.
