# DeepFilterNet DSP Runtime Design

## Scope

Add the first Rust DeepFilterNet libDF-backed runtime path for Lyre server-side noise-provider wiring.

This increment covers:

- Adding the MIT/Apache-2.0 `deep_filter` crate as a workspace dependency.
- Replacing the current `NoiseProvider::Deepfilternet` unsupported-provider branch with a Rust `NoiseCanceller` implementation built on `df::DFState` from the `deep_filter` package.
- Validating DeepFilterNet frame shape at the noise-cancelling boundary.
- Wiring the provider through the existing `NoiseCancellingAudioFrameProcessor` so decoded WebRTC PCM frames can take the DeepFilterNet provider path.
- Updating tests, memory, and roadmap to describe exactly what this runtime does.

This increment does not add DeepFilterNet neural-network model inference, pretrained model downloads, ONNX/PyTorch bindings, post-filtering, runtime model configuration, model files in the repository, CLI model paths, browser-side WASM noise cancellation, frontend UI changes, or any claim that the output is noise-reduced.

## Dependency Finding

The available Rust package is `deep_filter = "0.2.5"`, licensed MIT/Apache-2.0. Its library crate is imported as `df`; its default feature set exposes `df::DFState`, STFT/ISTFT, ERB filter-bank helpers, and frame processing primitives. It does not expose a complete pretrained DeepFilterNet/DeepFilterNet2 neural-network inference pipeline or model loader.

Because of that, this increment must not claim complete DeepFilterNet model-based speech enhancement or observable noise reduction. It makes the DeepFilterNet provider use the Rust libDF DSP frame path instead of being unsupported, and it keeps full model inference as future work.

## Backend Behavior

`build_noise_canceller(config)` should return a `DeepFilterNetNoiseCanceller` when `config.provider == NoiseProvider::Deepfilternet`.

The DeepFilterNet runtime should:

- Accept 48 kHz mono PCM only.
- Accept a non-empty sample length that is a multiple of 480 samples.
- Construct its state with `df::DFState::default()`.
- Process each 480-sample chunk by allocating a 480-sample output buffer and calling `DFState::process_frame(input_chunk, output_chunk)`.
- Return the same number of samples as the input.
- Return `voice_activity_probability: None` because the libDF DSP path does not expose VAD.
- Preserve one persistent DF state per `NoiseCancellationConfig` through the existing processor cache.

The expected observable DSP behavior is STFT/ISTFT reconstruction through libDF's stateful frame processor. Tests should verify shape, finite samples, and that the provider no longer falls back as unsupported. Tests must not assert noise attenuation, speech enhancement quality, or a specific sample-by-sample waveform because this increment does not run neural DeepFilterNet gains.

Invalid sample rate, channel count, empty input, or non-multiple frame length should return `NoiseCancellationError::InvalidFrameShape` with `provider: NoiseProvider::Deepfilternet` and expected values matching the DeepFilterNet runtime.

## Server-Media Integration

No new media pipeline code is needed. Existing server media flow already maps room relay noise config into `WebMediaRuntime`, which calls `NoiseCancellingAudioFrameProcessor`. Once the DeepFilterNet provider builds successfully, decoded server-media PCM frames configured with `deepfilternet` should use this new runtime path.

The existing warning/fallback behavior remains unchanged at the infallible `AudioFrameProcessor` boundary: if DeepFilterNet processing returns an error for an invalid frame shape, the processor logs the full error context and passes through the original frame samples.

## Testing

Add Rust tests in `crates/lyre-noise-cancelling/src/tests.rs` for:

- `build_noise_canceller` builds `NoiseProvider::Deepfilternet` instead of returning `UnsupportedProvider`.
- DeepFilterNet rejects wrong sample rate, wrong channel count, empty input, and non-multiple-of-480 input with `InvalidFrameShape` for `NoiseProvider::Deepfilternet`.
- DeepFilterNet processes a valid 480-sample 48 kHz mono frame through `DFState::process_frame` and returns exactly 480 finite samples.
- DeepFilterNet processes a decoded Opus-sized 960-sample 48 kHz mono frame by chunking into two `DFState::process_frame` calls and returns exactly 960 finite samples.
- `NoiseCancellingAudioFrameProcessor` uses DeepFilterNet for valid audio and still passes through invalid DeepFilterNet frames.

Add or adjust web runtime tests if needed so an active relay configured with `NoiseProvider::Deepfilternet` can process a valid 48 kHz mono server-media frame without falling back because the provider is unsupported.

## Documentation

After implementation review approval:

- Update `MEMORY.md` to record that DeepFilterNet is now wired through the Rust libDF DSP frame path, not full pretrained model inference or proven noise attenuation.
- Update `docs/roadmap.md`:
  - Move the first DeepFilterNet runtime wiring from Next to Completed.
  - Keep full DeepFilterNet model inference/configuration in Next.

## Acceptance Criteria

- `NoiseProvider::Deepfilternet` is no longer rejected as unsupported by `build_noise_canceller`.
- Valid 48 kHz mono DeepFilterNet frames are processed through `df::DFState::process_frame` using persistent per-config state.
- Invalid DeepFilterNet frame shapes return provider-specific `InvalidFrameShape` errors.
- Existing server media runtime can use a DeepFilterNet-configured room relay without an unsupported-provider fallback for valid decoded PCM.
- The implementation explicitly documents that it is libDF DSP-only provider wiring and does not claim full neural DeepFilterNet speech enhancement or noise attenuation.
- Rust formatting, clippy, and workspace tests pass.
