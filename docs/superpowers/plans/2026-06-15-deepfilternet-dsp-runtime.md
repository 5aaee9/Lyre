# DeepFilterNet DSP Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the `deepfilternet` noise provider use a real Rust libDF `DFState` DSP frame path instead of being unsupported.

**Architecture:** Add `deep_filter` as a workspace dependency, introduce a `DeepFilterNetNoiseCanceller` beside the existing RNNoise canceller, and route it through the existing `NoiseCancellingAudioFrameProcessor` cache. This is libDF STFT/ISTFT provider wiring only; full pretrained neural DeepFilterNet inference remains future work.

**Tech Stack:** Rust, `deep_filter = "0.2.5"`, existing `lyre-core::AudioFrameProcessor`, cargo tests/clippy/nextest.

**Reviewed Spec:** `docs/superpowers/specs/2026-06-15-deepfilternet-dsp-runtime-design.md`

---

## File Structure

- Modify `Cargo.toml`: add `deep_filter` to `[workspace.dependencies]`.
- Modify `crates/lyre-noise-cancelling/Cargo.toml`: depend on workspace `deep_filter`.
- Modify `crates/lyre-noise-cancelling/src/lib.rs`: add DeepFilterNet constants, frame validation, and `DeepFilterNetNoiseCanceller`.
- Modify `crates/lyre-noise-cancelling/src/tests.rs`: replace unsupported-provider tests with libDF DSP provider tests.
- Modify `crates/lyre-web/src/api_media_tests.rs`: add an AppState media-runtime test for DeepFilterNet-configured relay processing.
- Modify after implementation review approval: `MEMORY.md` and `docs/roadmap.md`.

Keep Rust source files below 400 LOC. If `crates/lyre-noise-cancelling/src/tests.rs` approaches the limit, keep test additions concise rather than splitting unrelated tests.

## Task 0: SDD Pre-Implementation Gate

**Files:**
- Read: `docs/superpowers/specs/2026-06-15-deepfilternet-dsp-runtime-design.md`
- Read: `docs/superpowers/plans/2026-06-15-deepfilternet-dsp-runtime.md`

- [x] **Step 1: Confirm approved spec review exists**

Required evidence before implementation:

```text
VERDICT: APPROVE
ISSUES:
- None
REQUIRED_CHANGES:
- None
```

- [x] **Step 2: Dispatch independent plan reviewer**

Dispatch an independent reviewer with the approved spec and this plan. Required verdict:

```text
VERDICT: APPROVE
```

Do not edit implementation files until the plan review approves.

## Task 1: Add DeepFilterNet Dependency and Provider Tests

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/lyre-noise-cancelling/Cargo.toml`
- Modify: `crates/lyre-noise-cancelling/src/tests.rs`

- [x] **Step 1: Add dependency declarations**

Add to root `Cargo.toml` under `[workspace.dependencies]`:

```toml
deep_filter = { version = "0.2.5", default-features = false }
```

Add to `crates/lyre-noise-cancelling/Cargo.toml`:

```toml
deep_filter.workspace = true
```

- [x] **Step 2: Replace the unsupported DeepFilterNet test**

Replace `factory_rejects_deepfilternet_until_real_backend_exists` with:

```rust
#[test]
fn factory_builds_deepfilternet_dsp_runtime() {
    let mut canceller = build_noise_canceller(config(NoiseProvider::Deepfilternet)).unwrap();

    let output = canceller
        .process_frame(NoiseFrame {
            sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
            channels: DEEPFILTERNET_CHANNELS,
            samples: &[0.25; DEEPFILTERNET_FRAME_SIZE],
        })
        .unwrap();

    assert_eq!(output.samples.len(), DEEPFILTERNET_FRAME_SIZE);
    assert!(output.samples.iter().all(|sample| sample.is_finite()));
    assert_eq!(output.voice_activity_probability, None);
}
```

- [x] **Step 3: Add invalid-frame tests**

Add:

```rust
#[test]
fn deepfilternet_rejects_wrong_sample_rate_channels_and_frame_size() {
    let mut canceller = build_noise_canceller(config(NoiseProvider::Deepfilternet)).unwrap();

    assert_eq!(
        canceller
            .process_frame(NoiseFrame {
                sample_rate_hz: 44_100,
                channels: 2,
                samples: &[0.0; 32],
            })
            .unwrap_err(),
        NoiseCancellationError::InvalidFrameShape {
            provider: NoiseProvider::Deepfilternet,
            sample_rate_hz: 44_100,
            channels: 2,
            samples: 32,
            expected_sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
            expected_channels: DEEPFILTERNET_CHANNELS,
            expected_samples: DEEPFILTERNET_FRAME_SIZE,
        }
    );
}

#[test]
fn deepfilternet_rejects_empty_or_non_multiple_frame_size() {
    let mut canceller = build_noise_canceller(config(NoiseProvider::Deepfilternet)).unwrap();

    for samples in [Vec::new(), vec![0.0; DEEPFILTERNET_FRAME_SIZE + 1]] {
        assert_eq!(
            canceller
                .process_frame(NoiseFrame {
                    sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
                    channels: DEEPFILTERNET_CHANNELS,
                    samples: &samples,
                })
                .unwrap_err(),
            NoiseCancellationError::InvalidFrameShape {
                provider: NoiseProvider::Deepfilternet,
                sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
                channels: DEEPFILTERNET_CHANNELS,
                samples: samples.len(),
                expected_sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
                expected_channels: DEEPFILTERNET_CHANNELS,
                expected_samples: DEEPFILTERNET_FRAME_SIZE,
            }
        );
    }
}
```

- [x] **Step 4: Add decoded-Opus-size and adapter tests**

Add:

```rust
#[test]
fn deepfilternet_processes_960_sample_mono_frame_in_chunks() {
    let mut canceller = build_noise_canceller(config(NoiseProvider::Deepfilternet)).unwrap();
    let input = decoded_opus_frame_samples();

    let output = canceller
        .process_frame(NoiseFrame {
            sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
            channels: DEEPFILTERNET_CHANNELS,
            samples: &input,
        })
        .unwrap();

    assert_eq!(output.samples.len(), DEEPFILTERNET_FRAME_SIZE * 2);
    assert!(output.samples.iter().all(|sample| sample.is_finite()));
    assert_eq!(output.voice_activity_probability, None);
}

#[test]
fn audio_frame_processor_adapter_uses_deepfilternet_for_valid_audio() {
    let processor = NoiseCancellingAudioFrameProcessor::default();
    let input = decoded_opus_frame_samples();
    let frame = AudioFrame {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
        track_id: "audio-main".to_owned(),
        sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
        channels: DEEPFILTERNET_CHANNELS,
        sequence: 1,
        samples: input,
    };

    let output = processor.process(&frame, &config(NoiseProvider::Deepfilternet));

    assert_eq!(output.len(), DEEPFILTERNET_FRAME_SIZE * 2);
    assert!(output.iter().all(|sample| sample.is_finite()));
}
```

- [x] **Step 5: Run tests and observe failure**

Run:

```bash
cargo test -p lyre-noise-cancelling deepfilternet
```

Expected before implementation: compilation fails because `DEEPFILTERNET_*` constants and implementation do not exist.

## Task 2: Implement DeepFilterNet libDF DSP Runtime

**Files:**
- Modify: `crates/lyre-noise-cancelling/src/lib.rs`

- [x] **Step 1: Import DFState and constants**

Add near existing imports:

```rust
use df::DFState;
```

Add constants near RNNoise constants:

```rust
pub const DEEPFILTERNET_SAMPLE_RATE_HZ: u32 = 48_000;
pub const DEEPFILTERNET_CHANNELS: u16 = 1;
pub const DEEPFILTERNET_FRAME_SIZE: usize = 480;
```

- [x] **Step 2: Route provider construction**

Change `build_noise_canceller`:

```rust
NoiseProvider::Deepfilternet => Ok(Box::new(DeepFilterNetNoiseCanceller::new(config))),
```

- [x] **Step 3: Add DeepFilterNet canceller**

Add after `RnnoiseNoiseCanceller`:

```rust
pub struct DeepFilterNetNoiseCanceller {
    config: NoiseCancellationConfig,
    state: DFState,
}

impl DeepFilterNetNoiseCanceller {
    pub fn new(config: NoiseCancellationConfig) -> Self {
        Self {
            config,
            state: DFState::default(),
        }
    }

    pub fn config(&self) -> &NoiseCancellationConfig {
        &self.config
    }
}

impl NoiseCanceller for DeepFilterNetNoiseCanceller {
    fn process_frame(
        &mut self,
        frame: NoiseFrame<'_>,
    ) -> Result<NoiseFrameOutput, NoiseCancellationError> {
        validate_deepfilternet_frame(frame)?;

        let mut samples = Vec::with_capacity(frame.samples.len());
        for chunk in frame.samples.chunks_exact(DEEPFILTERNET_FRAME_SIZE) {
            let mut output = vec![0.0; DEEPFILTERNET_FRAME_SIZE];
            self.state.process_frame(chunk, &mut output);
            samples.extend(output);
        }

        Ok(NoiseFrameOutput {
            samples,
            voice_activity_probability: None,
        })
    }
}
```

- [x] **Step 4: Add frame validation**

Add near `validate_rnnoise_frame`:

```rust
fn validate_deepfilternet_frame(frame: NoiseFrame<'_>) -> Result<(), NoiseCancellationError> {
    if frame.sample_rate_hz == DEEPFILTERNET_SAMPLE_RATE_HZ
        && frame.channels == DEEPFILTERNET_CHANNELS
        && !frame.samples.is_empty()
        && frame.samples.len().is_multiple_of(DEEPFILTERNET_FRAME_SIZE)
    {
        return Ok(());
    }

    Err(NoiseCancellationError::InvalidFrameShape {
        provider: NoiseProvider::Deepfilternet,
        sample_rate_hz: frame.sample_rate_hz,
        channels: frame.channels,
        samples: frame.samples.len(),
        expected_sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
        expected_channels: DEEPFILTERNET_CHANNELS,
        expected_samples: DEEPFILTERNET_FRAME_SIZE,
    })
}
```

- [x] **Step 5: Verify noise-cancelling tests**

Run:

```bash
cargo test -p lyre-noise-cancelling deepfilternet
cargo test -p lyre-noise-cancelling
```

Expected: tests pass.

## Task 3: Prove Web Runtime Uses DeepFilterNet Provider

**Files:**
- Modify: `crates/lyre-web/src/api_media_tests.rs`

- [x] **Step 1: Add AppState test**

Add near `app_state_process_media_frame_runs_rnnoise_for_valid_audio`:

```rust
#[test]
fn app_state_process_media_frame_runs_deepfilternet_for_valid_audio() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_01");
    start_relay_with_track(
        &state,
        room_id.clone(),
        user_id.clone(),
        NoiseProvider::Deepfilternet,
    );

    process_samples(&state, &room_id, &user_id, vec![120.0; 960]);

    let frames = state.processed_media_frames(&room_id);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].samples.len(), 960);
    assert!(frames[0].samples.iter().all(|sample| sample.is_finite()));
    assert_eq!(frames[0].noise.provider, NoiseProvider::Deepfilternet);
}
```

- [x] **Step 2: Verify web test**

Run:

```bash
cargo test -p lyre-web api_media_tests::app_state_process_media_frame_runs_deepfilternet_for_valid_audio
```

Expected: pass.

## Task 4: Implementation Review, Docs, Final Verification, Commit

**Files:**
- Modify after implementation review approval: `MEMORY.md`
- Modify after implementation review approval: `docs/roadmap.md`

- [x] **Step 1: Run pre-review verification**

Run:

```bash
cargo fmt --all --check
cargo test -p lyre-noise-cancelling deepfilternet
cargo test -p lyre-noise-cancelling
cargo test -p lyre-web api_media_tests::app_state_process_media_frame_runs_deepfilternet_for_valid_audio
```

- [x] **Step 2: Independent implementation review**

Dispatch a fresh reviewer with the approved spec, this plan, diff, and verification output. Required verdict:

```text
VERDICT: APPROVE
```

Fix and re-review until approved.

- [x] **Step 3: Update docs**

Update `MEMORY.md` with:

- DeepFilterNet now builds a server-side Rust libDF DSP runtime via `df::DFState` from the `deep_filter` package.
- This path runs STFT/ISTFT frame reconstruction only and does not include pretrained model inference or proven noise attenuation.
- Full DeepFilterNet model inference/configuration remains future work.

Update `docs/roadmap.md`:

- Move first DeepFilterNet libDF DSP provider wiring to Completed.
- Keep full DeepFilterNet model inference/configuration in Next.

- [ ] **Step 4: Final verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path Cargo.toml --workspace
cd frontend
npm test -- --run
npm run typecheck
npm run lint
npm run build
cd ..
git diff --check
```

Expected: all pass.

- [ ] **Step 5: Commit and push**

Stage only this increment's files plus this reviewed spec/plan. Leave unrelated untracked SDD artifacts untouched. Create a Lore-format commit and push current branch/upstream.
