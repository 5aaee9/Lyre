# RNNoise Opus Frame Alignment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make server-side RNNoise process real decoded 20 ms Opus PCM frames instead of falling back to passthrough.

**Architecture:** Keep the `NoiseCanceller` and `NoiseCancellingAudioFrameProcessor` public APIs unchanged. Split the near-limit `lyre-noise-cancelling` tests into a focused test module, then update `RnnoiseNoiseCanceller` to process 48 kHz mono sample buffers in 480-sample chunks and concatenate output. Extend the real server-media runtime test to prove decoded 960-sample Opus PCM is processed under RNNoise.

**Tech Stack:** Rust, `nnnoiseless`, existing `lyre-core` media runtime, existing `lyre-web` server-media test support, `cargo nextest`.

---

## Reviewed Spec

Implement against:

- `docs/superpowers/specs/2026-06-15-rnnoise-opus-frame-alignment-design.md`

Boundaries:

- No public API changes.
- DeepFilterNet remains unsupported.
- No automatic server-media pump, RTP/RTCP egress, jitter buffering, packet loss concealment, or browser playback in this increment.
- Every changed Rust file must remain under 400 lines.

## File Structure

- Modify `crates/lyre-noise-cancelling/src/lib.rs`: remove inline test module, declare split test module, and implement chunked RNNoise processing.
- Create `crates/lyre-noise-cancelling/src/tests.rs`: moved existing tests plus new RNNoise 960-sample and invalid-shape tests.
- Modify `crates/lyre-web/src/server_media_runtime_tests.rs`: start the real server-media runtime test with RNNoise and assert output differs from the decoded input.
- Modify `MEMORY.md` and `docs/roadmap.md` only after independent implementation review approves.

## Task 1: Split Noise-Cancelling Tests Without Behavior Change

**Files:**
- Modify: `crates/lyre-noise-cancelling/src/lib.rs`
- Create: `crates/lyre-noise-cancelling/src/tests.rs`

- [x] **Step 1: Move the existing inline tests**

Move the entire current `#[cfg(test)] mod tests { ... }` block from `crates/lyre-noise-cancelling/src/lib.rs` into `crates/lyre-noise-cancelling/src/tests.rs`.

At the top of `crates/lyre-noise-cancelling/src/tests.rs`, use:

```rust
use super::*;
use lyre_core::{AudioFrame, AudioFrameProcessor, RoomId, UserId};
```

At the bottom of `crates/lyre-noise-cancelling/src/lib.rs`, replace the removed inline module with:

```rust
#[cfg(test)]
mod tests;
```

- [x] **Step 2: Verify the split preserves behavior**

Run:

```bash
cargo test -p lyre-noise-cancelling
wc -l crates/lyre-noise-cancelling/src/lib.rs crates/lyre-noise-cancelling/src/tests.rs
```

Expected: tests PASS and both files are under 400 LOC.

## Task 2: Add Failing RNNoise 960-Sample Tests

**Files:**
- Modify: `crates/lyre-noise-cancelling/src/tests.rs`

- [x] **Step 1: Add a 960-sample helper**

Add this helper near `rnnoise_frame()`:

```rust
fn decoded_opus_frame_samples() -> Vec<f32> {
    (0..RNNOISE_FRAME_SIZE * 2)
        .map(|index| ((index as f32) / 12.0).sin() * 120.0)
        .collect()
}
```

- [x] **Step 2: Add failing RNNoise chunking tests**

Add:

```rust
#[test]
fn rnnoise_processes_960_sample_mono_frame_in_chunks_and_reports_average_vad() {
    let mut canceller = build_noise_canceller(config(NoiseProvider::Rnnoise)).unwrap();
    let input = decoded_opus_frame_samples();

    let output = canceller
        .process_frame(NoiseFrame {
            sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
            channels: RNNOISE_CHANNELS,
            samples: &input,
        })
        .unwrap();

    assert_eq!(output.samples.len(), RNNOISE_FRAME_SIZE * 2);
    assert_ne!(output.samples, input);
    let vad = output.voice_activity_probability.unwrap();
    assert!((0.0..=1.0).contains(&vad));
}

#[test]
fn rnnoise_rejects_empty_or_non_multiple_frame_size() {
    let mut canceller = build_noise_canceller(config(NoiseProvider::Rnnoise)).unwrap();

    for samples in [Vec::new(), vec![0.0; RNNOISE_FRAME_SIZE + 1]] {
        let error = canceller
                .process_frame(NoiseFrame {
                    sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
                    channels: RNNOISE_CHANNELS,
                    samples: &samples,
                })
                .unwrap_err();
        assert_eq!(
            error,
            NoiseCancellationError::InvalidFrameShape {
                provider: NoiseProvider::Rnnoise,
                sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
                channels: RNNOISE_CHANNELS,
                samples: samples.len(),
                expected_sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
                expected_channels: RNNOISE_CHANNELS,
                expected_samples: RNNOISE_FRAME_SIZE,
            }
        );
        assert!(error.to_string().contains("non-empty multiple of 480 samples"));
    }
}

#[test]
fn audio_frame_processor_adapter_uses_rnnoise_for_decoded_opus_frame() {
    let processor = NoiseCancellingAudioFrameProcessor::default();
    let input = decoded_opus_frame_samples();
    let frame = AudioFrame {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
        track_id: "audio-main".to_owned(),
        sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
        channels: RNNOISE_CHANNELS,
        sequence: 1,
        samples: input.clone(),
    };

    let output = processor.process(&frame, &config(NoiseProvider::Rnnoise));

    assert_eq!(output.len(), RNNOISE_FRAME_SIZE * 2);
    assert_ne!(output, input);
}
```

- [x] **Step 3: Verify tests fail before implementation**

Run:

```bash
cargo test -p lyre-noise-cancelling rnnoise_processes_960_sample_mono_frame_in_chunks_and_reports_average_vad
cargo test -p lyre-noise-cancelling audio_frame_processor_adapter_uses_rnnoise_for_decoded_opus_frame
```

Expected: FAIL because RNNoise currently rejects 960-sample frames or adapter returns passthrough.

## Task 3: Implement Chunked RNNoise Processing

**Files:**
- Modify: `crates/lyre-noise-cancelling/src/lib.rs`

- [x] **Step 1: Update invalid frame-shape display text**

Update the `NoiseCancellationError::InvalidFrameShape` `#[error(...)]` text to:

```rust
#[error(
    "noise provider `{provider:?}` requires {expected_sample_rate_hz} Hz, {expected_channels} channel(s), and a non-empty multiple of {expected_samples} samples, got {sample_rate_hz} Hz, {channels} channel(s), and {samples} samples"
)]
```

- [x] **Step 2: Update RNNoise frame validation**

Replace `validate_rnnoise_frame` with:

```rust
fn validate_rnnoise_frame(frame: NoiseFrame<'_>) -> Result<(), NoiseCancellationError> {
    if frame.sample_rate_hz == RNNOISE_SAMPLE_RATE_HZ
        && frame.channels == RNNOISE_CHANNELS
        && !frame.samples.is_empty()
        && frame.samples.len() % RNNOISE_FRAME_SIZE == 0
    {
        return Ok(());
    }

    Err(NoiseCancellationError::InvalidFrameShape {
        provider: NoiseProvider::Rnnoise,
        sample_rate_hz: frame.sample_rate_hz,
        channels: frame.channels,
        samples: frame.samples.len(),
        expected_sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
        expected_channels: RNNOISE_CHANNELS,
        expected_samples: RNNOISE_FRAME_SIZE,
    })
}
```

- [x] **Step 3: Process chunks through one denoise state**

Replace the body after `validate_rnnoise_frame(frame)?;` in `RnnoiseNoiseCanceller::process_frame` with:

```rust
let mut samples = Vec::with_capacity(frame.samples.len());
let mut vad_total = 0.0;
let mut chunks = 0;

for chunk in frame.samples.chunks_exact(RNNOISE_FRAME_SIZE) {
    let mut output = vec![0.0; RNNOISE_FRAME_SIZE];
    vad_total += self.state.process_frame(&mut output, chunk);
    samples.extend(output);
    chunks += 1;
}

Ok(NoiseFrameOutput {
    samples,
    voice_activity_probability: Some(vad_total / chunks as f32),
})
```

- [x] **Step 4: Run focused noise tests**

Run:

```bash
cargo test -p lyre-noise-cancelling
```

Expected: PASS.

## Task 4: Prove Real Server-Media Frames Use RNNoise

**Files:**
- Modify: `crates/lyre-web/src/server_media_runtime_tests.rs`

- [x] **Step 1: Change the real decoded batch test to start RNNoise**

In `app_state_processes_real_drained_server_media_pcm_batch`, change the relay start call from:

```rust
state
    .media_relays
    .start(key.room_id.clone(), StartMediaRelayRequest::default());
```

to:

```rust
state.media_relays.start(
    key.room_id.clone(),
    StartMediaRelayRequest {
        noise: Some(lyre_core::NoiseCancellationConfig {
            provider: lyre_core::NoiseProvider::Rnnoise,
            intensity: 0.5,
            voice_activity_threshold: 0.35,
        }),
    },
);
```

- [x] **Step 2: Capture decoded PCM before processing**

Before the processing loop, wait for and capture one decoded PCM frame without losing it:

```rust
let decoded_frames = loop {
    let frames = state.drain_server_media_pcm_frames(&key);
    if !frames.is_empty() {
        break frames;
    }
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
};
let decoded_samples = decoded_frames[0].samples.clone();
let processed = server_media_runtime::process_pcm_frame_batch(
    &state.media_runtime,
    &key,
    decoded_frames,
)
.unwrap();
```

Then assert `processed > 0`.

- [x] **Step 3: Assert processed server-media frame is RNNoise output**

Replace the existing processing-loop assertion in this test with:

```rust
assert!(processed > 0);
let frames = state.processed_media_frames(&key.room_id);
assert_eq!(frames.len(), processed);
assert!(frames.iter().any(|frame| {
    frame.user_id == key.user_id
        && frame.track_id == "audio"
        && frame.sequence == 42
        && frame.sample_rate_hz == 48_000
        && frame.channels == 1
        && frame.noise.provider == lyre_core::NoiseProvider::Rnnoise
        && frame.samples.len() == lyre_webrtc::SERVER_MEDIA_OPUS_FRAME_SIZE
        && frame.samples != decoded_samples
}));
assert_eq!(state.process_server_media_pcm_frames(&key), Ok(0));
```

- [x] **Step 4: Run focused web runtime test**

Run:

```bash
cargo test -p lyre-web server_media_runtime_tests::app_state_processes_real_drained_server_media_pcm_batch
```

Expected: PASS.

## Task 5: Verification and Implementation Review Gate

**Files:**
- No docs changes in this task.
- No product code changes unless verification exposes a defect.

- [x] **Step 1: Run Rust checks**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: PASS.

- [x] **Step 2: Run frontend checks**

Run:

```bash
cd frontend && npm run generate:webrpc && npm test -- --run && npm run typecheck && npm run lint && npm run build
```

Expected: PASS.

- [x] **Step 3: Run static checks**

Run:

```bash
git diff --check
wc -l crates/lyre-noise-cancelling/src/lib.rs crates/lyre-noise-cancelling/src/tests.rs crates/lyre-web/src/server_media_runtime_tests.rs
```

Expected: `git diff --check` exits 0 and all listed Rust files are below 400 lines.

- [x] **Step 4: Dispatch independent implementation review**

Before updating docs, dispatch an independent implementation reviewer with:

- reviewed spec path,
- this reviewed plan path,
- full diff,
- verification output from Steps 1-3,
- SDD implementation verdict format.

Expected: reviewer returns `VERDICT: APPROVE`. If it returns `REVISE`, fix gaps, rerun relevant verification, and re-review.

## Task 6: Post-Review Documentation, Final Verification, Commit, and Push

**Files:**
- Modify: `MEMORY.md`
- Modify: `docs/roadmap.md`
- Modify only docs after Task 5 receives implementation `VERDICT: APPROVE`.

- [x] **Step 1: Update `MEMORY.md`**

Add:

```markdown
## 2026-06-15 RNNoise Opus Frame Alignment

- Updated server-side RNNoise to process real decoded 20 ms Opus PCM frames by chunking 960-sample input into two 480-sample RNNoise frames.
- Kept the public noise-cancelling API unchanged and split `lyre-noise-cancelling` tests out of `lib.rs` to keep Rust files below 400 LOC.
- Verified real server-media decoded PCM can be processed through RNNoise when the media relay is configured for RNNoise.
- DeepFilterNet, automatic server-media pumping, jitter buffering, processed RTP/RTCP egress, and browser playback remain future work.
```

- [x] **Step 2: Update `docs/roadmap.md`**

Move the server-side RNNoise handling for decoded WebRTC tracks into Completed. Keep Next focused on automatic server-media pumping, DeepFilterNet, jitter/PLC, and egress/playback.

- [x] **Step 3: Run final verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
cd frontend && npm run generate:webrpc && npm test -- --run && npm run typecheck && npm run lint && npm run build
git diff --check
wc -l crates/lyre-noise-cancelling/src/lib.rs crates/lyre-noise-cancelling/src/tests.rs crates/lyre-web/src/server_media_runtime_tests.rs
```

Expected: all checks PASS and all listed Rust files stay below 400 lines.

- [x] **Step 4: Review final diff**

Run:

```bash
git status --short
git diff --stat
git diff -- crates/lyre-noise-cancelling/src/lib.rs crates/lyre-noise-cancelling/src/tests.rs crates/lyre-web/src/server_media_runtime_tests.rs MEMORY.md docs/roadmap.md docs/superpowers/specs/2026-06-15-rnnoise-opus-frame-alignment-design.md docs/superpowers/plans/2026-06-15-rnnoise-opus-frame-alignment.md
```

Expected: only intended files changed.

- [ ] **Step 5: Commit and push**

Commit with Lore protocol:

```bash
git add crates/lyre-noise-cancelling/src/lib.rs crates/lyre-noise-cancelling/src/tests.rs crates/lyre-web/src/server_media_runtime_tests.rs MEMORY.md docs/roadmap.md docs/superpowers/specs/2026-06-15-rnnoise-opus-frame-alignment-design.md docs/superpowers/plans/2026-06-15-rnnoise-opus-frame-alignment.md
git commit -m "Align RNNoise with decoded Opus frame size" -m "Constraint: Real server-media Opus decode produces 960-sample 48 kHz mono PCM while nnnoiseless processes 480-sample frames.
Rejected: Treating 960-sample decoded Opus frames as invalid passthrough | It bypasses server-side RNNoise for the real ingress shape.
Confidence: medium
Scope-risk: narrow
Directive: Keep DeepFilterNet, automatic server-media pumping, RTP/RTCP egress, and browser playback as separate increments.
Tested: cargo fmt --all --check; cargo clippy --workspace --all-targets -- -D warnings; cargo nextest run --manifest-path Cargo.toml --workspace; frontend generate/test/typecheck/lint/build; git diff --check; LOC check
Not-tested: Browser end-to-end server media playback; DeepFilterNet processing"
git push
```
