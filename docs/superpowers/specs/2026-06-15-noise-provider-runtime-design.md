# Noise Provider Runtime Design

## Goal

Add a concrete, tested runtime layer in `lyre-noise-cancelling` that can instantiate provider-specific audio processors for the existing `NoiseCancellationConfig`, starting with real RNNoise-compatible processing through the Rust `nnnoiseless` crate.

## Context

Lyre currently has:

- `lyre-core::NoiseCancellationConfig` with providers `off`, `rnnoise`, and `deepfilternet`;
- `lyre-noise-cancelling::NoiseCanceller`, which always behaves as passthrough;
- `lyre-core::AudioFrameProcessor`, which can call a future noise processor for decoded PCM frames;
- documentation that server-side noise cancellation must run after WebRTC media is terminated and decoded.

The next useful step is to make `lyre-noise-cancelling` honest about provider capability. `off` should remain passthrough. `rnnoise` should perform real frame processing when the input matches RNNoise's contract. `deepfilternet` should be represented as unsupported until a real libDF integration is added, not silently mapped to passthrough.

## External Dependency Evidence

Use `nnnoiseless` for RNNoise-compatible processing. Current documentation for `nnnoiseless::DenoiseState::process_frame` states:

- each frame is exactly `DenoiseState::FRAME_SIZE`, currently 480 samples;
- samples are `f32` values in the i16 PCM range, not normalized `[-1.0, 1.0]` floats;
- processing returns a voice activity detection probability.

DeepFilterNet/libDF is not part of this increment. The crate ecosystem exposes lower-level DeepFilterNet modules, but this spec does not add model loading, STFT state, tensor runtime wiring, or realtime VOIP integration for DeepFilterNet.

## Scope

### In Scope

- Add a provider factory in `lyre-noise-cancelling` that builds a processor from `NoiseCancellationConfig`.
- Keep a common trait for processing one decoded PCM frame.
- Implement `off` as passthrough.
- Implement `rnnoise` with `nnnoiseless::DenoiseState`.
- Require RNNoise input frames to be 48 kHz, mono, and exactly 480 samples.
- Preserve sample order and return the same number of samples as the input frame.
- Expose the last RNNoise VAD probability for tests and future telemetry.
- Return structured errors for unsupported providers or invalid frame shape.
- Add an adapter that lets a `lyre-noise-cancelling` processor satisfy `lyre-core::AudioFrameProcessor`.
- Make adapter failures observable through structured tracing while preserving the current infallible `AudioFrameProcessor` trait.
- Add tests for factory selection, passthrough behavior, RNNoise shape validation, RNNoise VAD metadata, unsupported DeepFilterNet, and the core adapter.
- Update `README.md`, `MEMORY.md`, and `docs/roadmap.md`.
- Add `nnnoiseless` to the workspace dependencies.

### Out of Scope

- DeepFilterNet/libDF model loading or inference.
- Client-side WASM noise cancellation.
- WebRTC termination, RTP/RTCP, Opus decode/encode, or real broadcast.
- Automatic conversion between normalized `[-1.0, 1.0]` floats and i16-range PCM.
- Resampling, channel remixing, buffering partial frames, or splitting large frames.
- Runtime configuration reload.
- Applying `intensity` or `voice_activity_threshold` to RNNoise output.

## API Design

`lyre-noise-cancelling` owns runtime processing types:

```rust
pub struct NoiseFrame<'a> {
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub samples: &'a [f32],
}

pub struct NoiseFrameOutput {
    pub samples: Vec<f32>,
    pub voice_activity_probability: Option<f32>,
}

pub trait NoiseCanceller {
    fn process_frame(&mut self, frame: NoiseFrame<'_>) -> Result<NoiseFrameOutput, NoiseCancellationError>;
}
```

The processor needs mutable access because RNNoise keeps recurrent state between frames.

Factory:

```rust
pub fn build_noise_canceller(
    config: NoiseCancellationConfig,
) -> Result<Box<dyn NoiseCanceller + Send>, NoiseCancellationError>;
```

Provider behavior:

- `NoiseProvider::Off`: returns input samples unchanged and `voice_activity_probability: None`.
- `NoiseProvider::Rnnoise`: validates 48 kHz mono 480-sample frames, runs `DenoiseState::process_frame`, and returns processed samples plus `Some(vad)`.
- `NoiseProvider::Deepfilternet`: returns `NoiseCancellationError::UnsupportedProvider { provider }`.

For this increment, `intensity` and `voice_activity_threshold` are carried in `NoiseCancellationConfig` and included in adapter cache identity, but the RNNoise direct processor does not use them to alter samples or suppress frames. The VAD probability is returned as metadata only.

Errors:

```rust
pub enum NoiseCancellationError {
    UnsupportedProvider { provider: NoiseProvider },
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

## Core Adapter

Add an adapter in `lyre-noise-cancelling` that implements `lyre-core::AudioFrameProcessor`:

```rust
pub struct NoiseCancellingAudioFrameProcessor;
```

The adapter should store a `Mutex<HashMap<NoiseConfigKey, Box<dyn NoiseCanceller + Send>>>`. `AudioFrameProcessor::process` takes `&self`, so the mutex is the concrete synchronization boundary that preserves mutable RNNoise state while satisfying `Send + Sync`.

`NoiseConfigKey` is an internal key with:

- `provider: NoiseProvider`
- `intensity_bits: u32`
- `voice_activity_threshold_bits: u32`

The key uses `f32::to_bits()` for the numeric fields instead of requiring `NoiseCancellationConfig` to implement `Eq` or `Hash`.

Because `lyre-core::AudioFrameProcessor::process` currently returns `Vec<f32>`, this adapter cannot propagate `NoiseCancellationError` directly. For this increment:

- `off` returns passthrough.
- `rnnoise` returns processed samples when frame shape is valid.
- unsupported or invalid frames return the original samples unchanged and emit a structured `tracing::warn!` event containing the full error chain with `{error:#}` plus room id, user id, track id, sample rate, channels, and sample count.

This infallible adapter behavior is allowed only because the core trait cannot return an error yet. The direct `NoiseCanceller` API must not hide errors.

## Testing

Add unit tests in `crates/lyre-noise-cancelling/src/lib.rs` or focused submodules:

- `factory_builds_off_passthrough`
- `factory_rejects_deepfilternet_until_real_backend_exists`
- `rnnoise_rejects_wrong_sample_rate_channels_and_frame_size`
- `rnnoise_processes_480_sample_mono_frame_and_reports_vad`
- `audio_frame_processor_adapter_uses_rnnoise_for_valid_audio`
- `audio_frame_processor_adapter_passthroughs_invalid_or_unsupported_frames`
- `audio_frame_processor_adapter_preserves_state_per_config_key`

Use deterministic low-amplitude i16-range sample data. Tests should assert shape and metadata, not exact RNNoise model output values except where unavoidable.

## Documentation

Update docs to say:

- RNNoise-compatible server-side frame processing is available in `lyre-noise-cancelling` for decoded 48 kHz mono 480-sample PCM frames.
- RNNoise currently returns VAD metadata but does not apply Lyre's `intensity` or `voice_activity_threshold` parameters to alter output.
- DeepFilterNet remains planned and unsupported at runtime.
- The server still does not terminate WebRTC media or broadcast processed audio.
- Client-side noise cancellation, if added before server-side media relay processing, must use Rust compiled to WebAssembly.

## Acceptance Criteria

- `lyre-noise-cancelling` no longer maps every provider to passthrough in its direct API.
- RNNoise-compatible processing uses `nnnoiseless`.
- DeepFilterNet direct factory creation fails with a structured unsupported-provider error.
- Invalid RNNoise frame shapes fail with a structured error containing actual and expected shape.
- The adapter can be plugged into `lyre-core::MediaRuntime` through `AudioFrameProcessor`.
- Adapter fallback for invalid or unsupported frames logs structured warnings and is limited to the current infallible core trait boundary.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo nextest run --manifest-path "Cargo.toml" --workspace` pass.
- `README.md`, `MEMORY.md`, and `docs/roadmap.md` are updated.
