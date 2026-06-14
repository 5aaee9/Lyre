# Noise Provider Runtime Implementation Plan

> **For agentic workers:** REQUIRED WORKFLOW: Execute this plan as `$sdd-workflow`. Treat Task 1 through Task 4 and Task 6 documentation updates as bounded SDD implementation subtasks: each subtask keeps to its listed files, follows the approved spec and plan, and is not considered complete until its changes have either passed the listed verification command or are included in the final independent implementation review. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Add a concrete `lyre-noise-cancelling` provider runtime with real RNNoise-compatible processing through `nnnoiseless`, explicit DeepFilterNet unsupported errors, and a core audio-frame adapter.

**Architecture:** Keep provider-specific processing inside `lyre-noise-cancelling`. The direct noise API is fallible and does not hide unsupported providers or invalid RNNoise frame shapes. The adapter implements the existing infallible `lyre-core::AudioFrameProcessor` with a mutex-protected cache and structured warning logs when it must passthrough.

**Tech Stack:** Rust 2021, `nnnoiseless` with default features disabled, `thiserror`, `tracing`, `lyre-core::AudioFrameProcessor`.

---

## File Structure

- Modify `Cargo.toml`: add workspace dependency `nnnoiseless = { version = "0.5.2", default-features = false }`.
- Modify `crates/lyre-noise-cancelling/Cargo.toml`: add `nnnoiseless`, `thiserror`, and `tracing` workspace dependencies.
- Modify `crates/lyre-noise-cancelling/src/lib.rs`: replace passthrough-only implementation with provider runtime, errors, RNNoise processor, adapter, and tests. This file stays under 400 LOC target; split only if implementation exceeds that.
- Modify `README.md`, `MEMORY.md`, and `docs/roadmap.md` after implementation approval.

## Task 0: SDD Gates

**Files:**
- Read: `docs/superpowers/specs/2026-06-15-noise-provider-runtime-design.md`
- Modify: `docs/superpowers/plans/2026-06-15-noise-provider-runtime.md`

- [x] **Step 1: Confirm approved spec review exists**

Expected evidence:

```text
VERDICT: APPROVE
ISSUES:
- None
REQUIRED_CHANGES:
- None
```

- [x] **Step 2: Dispatch independent plan reviewer**

Review this plan against `docs/superpowers/specs/2026-06-15-noise-provider-runtime-design.md`.

Expected verdict:

```text
VERDICT: APPROVE
ISSUES:
- None
REQUIRED_CHANGES:
- None
```

- [x] **Step 3: Stop before code edits unless plan is approved**

Do not modify Rust, Cargo, README, MEMORY, or roadmap files until the plan reviewer returns `VERDICT: APPROVE`.

## Task 1: Add Failing Noise Runtime Tests

**SDD subtask gate:** This task is a bounded `$sdd-workflow` implementation subtask. It may edit only `crates/lyre-noise-cancelling/src/lib.rs`, and its result must be covered by the final independent implementation review.

**Files:**
- Modify: `crates/lyre-noise-cancelling/src/lib.rs`

- [x] **Step 1: Replace existing tests with behavior tests for the new API**

Add tests equivalent to this module shape:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use lyre_core::{
        AudioFrame, AudioFrameProcessor, NoiseCancellationConfig, NoiseProvider, RoomId, UserId,
    };

    fn config(provider: NoiseProvider) -> NoiseCancellationConfig {
        NoiseCancellationConfig {
            provider,
            intensity: 0.5,
            voice_activity_threshold: 0.35,
        }
    }

    fn rnnoise_frame() -> NoiseFrame<'static> {
        NoiseFrame {
            sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
            channels: RNNOISE_CHANNELS,
            samples: &[120.0; RNNOISE_FRAME_SIZE],
        }
    }

    #[test]
    fn factory_builds_off_passthrough() {
        let mut canceller = build_noise_canceller(config(NoiseProvider::Off)).unwrap();

        let output = canceller
            .process_frame(NoiseFrame {
                sample_rate_hz: 44_100,
                channels: 2,
                samples: &[0.25, -0.5, 0.75],
            })
            .unwrap();

        assert_eq!(output.samples, vec![0.25, -0.5, 0.75]);
        assert_eq!(output.voice_activity_probability, None);
    }

    #[test]
    fn factory_rejects_deepfilternet_until_real_backend_exists() {
        assert_eq!(
            build_noise_canceller(config(NoiseProvider::Deepfilternet)).unwrap_err(),
            NoiseCancellationError::UnsupportedProvider {
                provider: NoiseProvider::Deepfilternet,
            }
        );
    }

    #[test]
    fn rnnoise_rejects_wrong_sample_rate_channels_and_frame_size() {
        let mut canceller = build_noise_canceller(config(NoiseProvider::Rnnoise)).unwrap();

        assert_eq!(
            canceller
                .process_frame(NoiseFrame {
                    sample_rate_hz: 44_100,
                    channels: 2,
                    samples: &[0.0; 32],
                })
                .unwrap_err(),
            NoiseCancellationError::InvalidFrameShape {
                provider: NoiseProvider::Rnnoise,
                sample_rate_hz: 44_100,
                channels: 2,
                samples: 32,
                expected_sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
                expected_channels: RNNOISE_CHANNELS,
                expected_samples: RNNOISE_FRAME_SIZE,
            }
        );
    }

    #[test]
    fn rnnoise_processes_480_sample_mono_frame_and_reports_vad() {
        let mut canceller = build_noise_canceller(config(NoiseProvider::Rnnoise)).unwrap();

        let output = canceller.process_frame(rnnoise_frame()).unwrap();

        assert_eq!(output.samples.len(), RNNOISE_FRAME_SIZE);
        let vad = output.voice_activity_probability.unwrap();
        assert!((0.0..=1.0).contains(&vad));
    }

    #[test]
    fn audio_frame_processor_adapter_uses_rnnoise_for_valid_audio() {
        let processor = NoiseCancellingAudioFrameProcessor::default();
        let frame = AudioFrame {
            room_id: RoomId::default_room(),
            user_id: UserId::from_external("user_01"),
            track_id: "audio-main".to_owned(),
            sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
            channels: RNNOISE_CHANNELS,
            sequence: 1,
            samples: vec![120.0; RNNOISE_FRAME_SIZE],
        };

        let output = processor.process(&frame, &config(NoiseProvider::Rnnoise));

        assert_eq!(output.len(), RNNOISE_FRAME_SIZE);
    }

    #[test]
    fn audio_frame_processor_adapter_passthroughs_invalid_or_unsupported_frames() {
        let processor = NoiseCancellingAudioFrameProcessor::default();
        let frame = AudioFrame {
            room_id: RoomId::default_room(),
            user_id: UserId::from_external("user_01"),
            track_id: "audio-main".to_owned(),
            sample_rate_hz: 44_100,
            channels: 2,
            sequence: 1,
            samples: vec![0.1, -0.2, 0.3],
        };

        assert_eq!(
            processor.process(&frame, &config(NoiseProvider::Rnnoise)),
            frame.samples
        );
        assert_eq!(
            processor.process(&frame, &config(NoiseProvider::Deepfilternet)),
            frame.samples
        );
    }

    #[test]
    fn audio_frame_processor_adapter_preserves_state_per_config_key() {
        let first = NoiseConfigKey::from(&NoiseCancellationConfig {
            provider: NoiseProvider::Rnnoise,
            intensity: 0.5,
            voice_activity_threshold: 0.35,
        });
        let second = NoiseConfigKey::from(&NoiseCancellationConfig {
            provider: NoiseProvider::Rnnoise,
            intensity: f32::from_bits(0.5f32.to_bits()),
            voice_activity_threshold: 0.35,
        });
        let third = NoiseConfigKey::from(&NoiseCancellationConfig {
            provider: NoiseProvider::Rnnoise,
            intensity: 0.6,
            voice_activity_threshold: 0.35,
        });

        assert_eq!(first, second);
        assert_ne!(first, third);
    }
}
```

- [x] **Step 2: Run tests and confirm they fail before implementation**

Run:

```bash
cargo test -p lyre-noise-cancelling -- --nocapture
```

Expected: compile failures for missing `NoiseFrame`, `NoiseFrameOutput`, `NoiseCancellationError`, `build_noise_canceller`, `NoiseCancellingAudioFrameProcessor`, `NoiseConfigKey`, and RNNoise constants.

## Task 2: Add Dependencies

**SDD subtask gate:** This task is a bounded `$sdd-workflow` implementation subtask. It may edit only `Cargo.toml` and `crates/lyre-noise-cancelling/Cargo.toml`, and its result must be covered by dependency graph verification plus the final independent implementation review.

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/lyre-noise-cancelling/Cargo.toml`

- [x] **Step 1: Add workspace dependency**

In `[workspace.dependencies]` add:

```toml
nnnoiseless = { version = "0.5.2", default-features = false }
```

- [x] **Step 2: Add crate dependencies**

In `crates/lyre-noise-cancelling/Cargo.toml` add:

```toml
nnnoiseless.workspace = true
thiserror.workspace = true
tracing.workspace = true
```

- [x] **Step 3: Verify dependency graph stays focused**

Run:

```bash
cargo tree -p lyre-noise-cancelling | rg "nnnoiseless|hound|clap|dasp"
```

Expected: output contains `nnnoiseless v0.5.2` and does not contain `hound`, `clap`, or `dasp`.

## Task 3: Implement Direct Provider Runtime

**SDD subtask gate:** This task is a bounded `$sdd-workflow` implementation subtask. It may edit only `crates/lyre-noise-cancelling/src/lib.rs`, must satisfy the direct provider tests, and must be covered by the final independent implementation review.

**Files:**
- Modify: `crates/lyre-noise-cancelling/src/lib.rs`

- [x] **Step 1: Add imports, constants, frame DTOs, and errors**

Replace the top of the file with:

```rust
pub use lyre_core::{NoiseCancellationConfig, NoiseProvider};

use lyre_core::{AudioFrame, AudioFrameProcessor};
use nnnoiseless::DenoiseState;
use std::{
    collections::HashMap,
    sync::Mutex,
};
use thiserror::Error;

pub const RNNOISE_SAMPLE_RATE_HZ: u32 = 48_000;
pub const RNNOISE_CHANNELS: u16 = 1;
pub const RNNOISE_FRAME_SIZE: usize = DenoiseState::FRAME_SIZE;

#[derive(Debug, Clone, Copy)]
pub struct NoiseFrame<'a> {
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub samples: &'a [f32],
}

#[derive(Debug, Clone, PartialEq)]
pub struct NoiseFrameOutput {
    pub samples: Vec<f32>,
    pub voice_activity_probability: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum NoiseCancellationError {
    #[error("noise provider `{provider:?}` is not supported by the server runtime")]
    UnsupportedProvider { provider: NoiseProvider },
    #[error("noise provider `{provider:?}` requires {expected_sample_rate_hz} Hz, {expected_channels} channel(s), and {expected_samples} samples, got {sample_rate_hz} Hz, {channels} channel(s), and {samples} samples")]
    InvalidFrameShape {
        provider: NoiseProvider,
        sample_rate_hz: u32,
        channels: u16,
        samples: usize,
        expected_sample_rate_hz: u32,
        expected_channels: u16,
        expected_samples: usize,
    },
}
```

- [x] **Step 2: Add direct trait and factory**

Add:

```rust
pub trait NoiseCanceller: Send {
    fn process_frame(
        &mut self,
        frame: NoiseFrame<'_>,
    ) -> Result<NoiseFrameOutput, NoiseCancellationError>;
}

pub fn build_noise_canceller(
    config: NoiseCancellationConfig,
) -> Result<Box<dyn NoiseCanceller + Send>, NoiseCancellationError> {
    match config.provider {
        NoiseProvider::Off => Ok(Box::new(PassthroughNoiseCanceller::new(config))),
        NoiseProvider::Rnnoise => Ok(Box::new(RnnoiseNoiseCanceller::new(config))),
        NoiseProvider::Deepfilternet => Err(NoiseCancellationError::UnsupportedProvider {
            provider: NoiseProvider::Deepfilternet,
        }),
    }
}
```

- [x] **Step 3: Implement passthrough provider**

Add:

```rust
#[derive(Debug, Clone)]
pub struct PassthroughNoiseCanceller {
    config: NoiseCancellationConfig,
}

impl PassthroughNoiseCanceller {
    pub fn new(config: NoiseCancellationConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &NoiseCancellationConfig {
        &self.config
    }
}

impl Default for PassthroughNoiseCanceller {
    fn default() -> Self {
        Self::new(NoiseCancellationConfig::default())
    }
}

impl NoiseCanceller for PassthroughNoiseCanceller {
    fn process_frame(
        &mut self,
        frame: NoiseFrame<'_>,
    ) -> Result<NoiseFrameOutput, NoiseCancellationError> {
        Ok(NoiseFrameOutput {
            samples: frame.samples.to_vec(),
            voice_activity_probability: None,
        })
    }
}
```

- [x] **Step 4: Implement RNNoise provider**

Add:

```rust
#[derive(Debug)]
pub struct RnnoiseNoiseCanceller {
    config: NoiseCancellationConfig,
    state: Box<DenoiseState<'static>>,
}

impl RnnoiseNoiseCanceller {
    pub fn new(config: NoiseCancellationConfig) -> Self {
        Self {
            config,
            state: DenoiseState::new(),
        }
    }

    pub fn config(&self) -> &NoiseCancellationConfig {
        &self.config
    }
}

impl NoiseCanceller for RnnoiseNoiseCanceller {
    fn process_frame(
        &mut self,
        frame: NoiseFrame<'_>,
    ) -> Result<NoiseFrameOutput, NoiseCancellationError> {
        validate_rnnoise_frame(frame)?;

        let mut output = vec![0.0; RNNOISE_FRAME_SIZE];
        let vad = self.state.process_frame(&mut output, frame.samples);
        Ok(NoiseFrameOutput {
            samples: output,
            voice_activity_probability: Some(vad),
        })
    }
}

fn validate_rnnoise_frame(frame: NoiseFrame<'_>) -> Result<(), NoiseCancellationError> {
    if frame.sample_rate_hz == RNNOISE_SAMPLE_RATE_HZ
        && frame.channels == RNNOISE_CHANNELS
        && frame.samples.len() == RNNOISE_FRAME_SIZE
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

- [x] **Step 5: Run direct provider tests**

Run:

```bash
cargo test -p lyre-noise-cancelling factory_ rnnoise_ -- --nocapture
```

Expected: direct factory and RNNoise tests pass.

## Task 4: Implement Core AudioFrameProcessor Adapter

**SDD subtask gate:** This task is a bounded `$sdd-workflow` implementation subtask. It may edit only `crates/lyre-noise-cancelling/src/lib.rs`, must satisfy the adapter tests, and must be covered by the final independent implementation review.

**Files:**
- Modify: `crates/lyre-noise-cancelling/src/lib.rs`

- [x] **Step 1: Add cache key**

Add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct NoiseConfigKey {
    provider: NoiseProviderKey,
    intensity_bits: u32,
    voice_activity_threshold_bits: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum NoiseProviderKey {
    Off,
    Rnnoise,
    Deepfilternet,
}

impl From<NoiseProvider> for NoiseProviderKey {
    fn from(provider: NoiseProvider) -> Self {
        match provider {
            NoiseProvider::Off => Self::Off,
            NoiseProvider::Rnnoise => Self::Rnnoise,
            NoiseProvider::Deepfilternet => Self::Deepfilternet,
        }
    }
}

impl From<&NoiseCancellationConfig> for NoiseConfigKey {
    fn from(config: &NoiseCancellationConfig) -> Self {
        Self {
            provider: NoiseProviderKey::from(config.provider),
            intensity_bits: config.intensity.to_bits(),
            voice_activity_threshold_bits: config.voice_activity_threshold.to_bits(),
        }
    }
}
```

- [x] **Step 2: Add adapter struct and default**

Add:

```rust
#[derive(Default)]
pub struct NoiseCancellingAudioFrameProcessor {
    cancellers: Mutex<HashMap<NoiseConfigKey, Box<dyn NoiseCanceller + Send>>>,
}
```

- [x] **Step 3: Implement `AudioFrameProcessor`**

Add:

```rust
impl AudioFrameProcessor for NoiseCancellingAudioFrameProcessor {
    fn process(&self, frame: &AudioFrame, noise: &NoiseCancellationConfig) -> Vec<f32> {
        let mut cancellers = self.cancellers.lock().expect("noise canceller mutex poisoned");
        let key = NoiseConfigKey::from(noise);
        let canceller = match cancellers.get_mut(&key) {
            Some(canceller) => canceller,
            None => match build_noise_canceller(noise.clone()) {
                Ok(canceller) => cancellers.entry(key).or_insert(canceller),
                Err(error) => {
                    tracing::warn!(
                        error = format_args!("{error:#}"),
                        room_id = %frame.room_id,
                        user_id = %frame.user_id,
                        track_id = %frame.track_id,
                        sample_rate_hz = frame.sample_rate_hz,
                        channels = frame.channels,
                        samples = frame.samples.len(),
                        "noise canceller unavailable; passing audio frame through"
                    );
                    return frame.samples.clone();
                }
            },
        };

        match canceller.process_frame(NoiseFrame {
            sample_rate_hz: frame.sample_rate_hz,
            channels: frame.channels,
            samples: &frame.samples,
        }) {
            Ok(output) => output.samples,
            Err(error) => {
                tracing::warn!(
                    error = format_args!("{error:#}"),
                    room_id = %frame.room_id,
                    user_id = %frame.user_id,
                    track_id = %frame.track_id,
                    sample_rate_hz = frame.sample_rate_hz,
                    channels = frame.channels,
                    samples = frame.samples.len(),
                    "noise cancellation failed; passing audio frame through"
                );
                frame.samples.clone()
            }
        }
    }
}
```

- [x] **Step 4: Run adapter tests**

Run:

```bash
cargo test -p lyre-noise-cancelling audio_frame_processor_adapter_ -- --nocapture
```

Expected: adapter tests pass.

## Task 5: Rust Verification Before Docs

**Files:**
- Read: changed Rust files and Cargo manifests

- [x] **Step 1: Run targeted crate tests**

Run:

```bash
cargo test -p lyre-noise-cancelling -- --nocapture
```

Expected: all `lyre-noise-cancelling` tests pass.

- [x] **Step 2: Run full Rust verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: format check, clippy, and all workspace tests pass.

- [x] **Step 3: Continue only if Rust verification passes**

Do not update docs if Rust verification fails. Fix Rust issues first and rerun this task.

## Task 6: Documentation Updates Before Implementation Review

**SDD subtask gate:** This task is a bounded `$sdd-workflow` implementation subtask. It may edit only `README.md`, `MEMORY.md`, `docs/roadmap.md`, and this plan file, and its result must be included in the final independent implementation review.

**Files:**
- Modify: `README.md`
- Modify: `MEMORY.md`
- Modify: `docs/roadmap.md`
- Modify: `docs/superpowers/plans/2026-06-15-noise-provider-runtime.md`

- [x] **Step 1: Update README**

In `README.md` Media Topology section, add a paragraph stating:

```md
`lyre-noise-cancelling` can now run RNNoise-compatible processing for decoded 48 kHz mono PCM frames of 480 samples using `nnnoiseless`. DeepFilterNet remains a planned runtime backend and direct factory creation reports it as unsupported until real model loading/inference is added. This still does not terminate browser WebRTC media, decode/encode Opus, or broadcast processed audio.
```

- [x] **Step 2: Update MEMORY**

Append:

```md
## 2026-06-15 Noise Provider Runtime

- Added a fallible provider runtime in `lyre-noise-cancelling`.
- Implemented RNNoise-compatible 48 kHz mono 480-sample processing with `nnnoiseless`.
- Kept DeepFilterNet explicit as unsupported until a real libDF/model integration is added.
- Added a `lyre-core::AudioFrameProcessor` adapter with structured warning logs at the current infallible trait boundary.
- Left client-side noise cancellation as future Rust WASM work.
```

- [x] **Step 3: Update roadmap**

Move RNNoise groundwork to Completed by adding:

```md
- RNNoise-compatible decoded PCM provider runtime in `lyre-noise-cancelling`.
```

Keep these in Next:

```md
- Wire the RNNoise provider runtime into real server media termination and broadcast.
- Add DeepFilterNet binding and processing implementation.
- Add optional client-side noise cancellation using Rust compiled to WebAssembly.
```

- [x] **Step 4: Mark completed implementation and docs boxes**

Only check boxes for tasks that have actually been performed.

## Task 7: Independent Full Implementation Review

**Files:**
- Read: full diff, spec, plan, verification output

- [x] **Step 1: Dispatch independent implementation reviewer**

Reviewer input must include:

- spec path `docs/superpowers/specs/2026-06-15-noise-provider-runtime-design.md`
- plan path `docs/superpowers/plans/2026-06-15-noise-provider-runtime.md`
- relevant diff
- targeted/full Rust verification output
- documentation updates in README, MEMORY, and roadmap

Expected verdict:

```text
VERDICT: APPROVE
SPEC_COVERAGE:
- ...
BLOCKERS:
- None
REQUIRED_CHANGES:
- None
```

Do not commit until the implementation reviewer approves the full implementation and documentation diff.

## Task 8: Final Verification, Commit, Push

**Files:**
- Modify: `docs/superpowers/plans/2026-06-15-noise-provider-runtime.md`

- [x] **Step 1: Run final verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
cd frontend
npm run generate:webrpc
npm test -- --run
npm run typecheck
npm run lint
npm run build
git diff --check
git diff --stat
git status --short
```

Expected: all commands pass, and status shows only intended files before commit.

- [x] **Step 2: Commit with Lore protocol**

Stage:

```bash
git add Cargo.toml crates/lyre-noise-cancelling/Cargo.toml crates/lyre-noise-cancelling/src/lib.rs README.md MEMORY.md docs/roadmap.md docs/superpowers/specs/2026-06-15-noise-provider-runtime-design.md docs/superpowers/plans/2026-06-15-noise-provider-runtime.md
```

Commit:

```text
Add RNNoise provider runtime boundary

Constraint: RNNoise processing requires decoded 48 kHz mono 480-sample PCM frames; real WebRTC media termination and broadcast remain future work.
Rejected: Treating DeepFilterNet as passthrough | Unsupported providers must be explicit until real inference exists.
Confidence: high
Scope-risk: moderate
Directive: Do not claim end-to-end server-side noise cancellation until media termination feeds this processor and processed frames are broadcast.
Tested: cargo fmt --all --check; cargo clippy --workspace --all-targets -- -D warnings; cargo nextest run --manifest-path Cargo.toml --workspace; npm run generate:webrpc; npm test -- --run; npm run typecheck; npm run lint; npm run build; git diff --check
Not-tested: Real WebRTC termination, RTP/RTCP, Opus decode/encode, DeepFilterNet inference, client WASM noise cancellation, and client broadcast remain future work.
```

- [x] **Step 3: Push**

Run:

```bash
git push
```

Expected: push succeeds or exact remote/credential error is reported with local commit SHA.
