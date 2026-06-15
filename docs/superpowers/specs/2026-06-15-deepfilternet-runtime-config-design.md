# DeepFilterNet Runtime Configuration Design

## Scope

This increment makes the existing server-side DeepFilterNet DSP runtime configurable at the Rust API process boundary. It does not add pretrained DeepFilterNet neural model inference. The locked `deep_filter = 0.2.5` crate exposes libDF DSP/STFT primitives such as `DFState::new` and `DFState::process_frame`; it does not expose a Rust API for loading the Python package's pretrained model checkpoints. This increment must therefore avoid claiming full neural inference and instead create a clear configuration seam for the current libDF runtime.

## Current State

- `lyre-noise-cancelling` builds `DeepFilterNetNoiseCanceller` with `DFState::default()`.
- `DFState::default()` is equivalent to sample rate `48000`, FFT size `960`, hop size `480`, ERB bands `32`, and minimum ERB frequencies `2`.
- Lyre currently validates DeepFilterNet frames as 48 kHz mono with a non-empty multiple of 480 samples.
- `lyre-web::WebMediaRuntime::new` always creates a default `NoiseCancellingAudioFrameProcessor`.
- `lyre-app` has no CLI or environment configuration for DeepFilterNet runtime parameters.
- `crates/lyre-app/src/cli.rs` is above the 400 LOC project guideline, so adding more CLI code there must include a focused split.
- `crates/lyre-web/src/api.rs` is above the 400 LOC project guideline, and this increment must touch `AppState`, so it also requires a focused split.

## Goals

- Add a Lyre-owned `DeepFilterNetRuntimeConfig` in `lyre-noise-cancelling`.
- Use that config when constructing `DeepFilterNetNoiseCanceller`.
- Keep the default runtime behavior unchanged.
- Expose config through `lyre-app serve` CLI flags and environment variables:
  - `--deepfilternet-fft-size` / `LYRE_DEEPFILTERNET_FFT_SIZE`
  - `--deepfilternet-hop-size` / `LYRE_DEEPFILTERNET_HOP_SIZE`
  - `--deepfilternet-erb-bands` / `LYRE_DEEPFILTERNET_ERB_BANDS`
  - `--deepfilternet-min-erb-freqs` / `LYRE_DEEPFILTERNET_MIN_ERB_FREQS`
- Validate these process-boundary values before starting the server.
- Pass the resulting runtime config from `lyre-app` to `lyre-web::ServeConfig`, then to `AppState`, `WebMediaRuntime`, and `NoiseCancellingAudioFrameProcessor`.
- Include the resolved default DeepFilterNet runtime config in `lyre config print`.
- Split the CLI implementation so no touched Rust source file remains above 400 LOC.

## Non-Goals

- Do not add neural network model loading, ONNX, Torch, tract, Python process execution, checkpoint download, or model path configuration.
- Do not change the frontend settings model.
- Do not change REST, WebRPC, or persistence DTO shapes.
- Do not alter RNNoise behavior.
- Do not add a runtime fallback that silently ignores invalid DeepFilterNet process configuration.

## Runtime Config Semantics

`DeepFilterNetRuntimeConfig` must contain:

- `sample_rate_hz: u32`, fixed at `48_000` for this increment.
- `channels: u16`, fixed at `1` for this increment.
- `fft_size: usize`
- `hop_size: usize`
- `erb_bands: usize`
- `min_erb_freqs: usize`

Defaults:

- `sample_rate_hz = 48_000`
- `channels = 1`
- `fft_size = 960`
- `hop_size = 480`
- `erb_bands = 32`
- `min_erb_freqs = 2`

Validation:

- `sample_rate_hz` must be `48_000`.
- `channels` must be `1`.
- `fft_size` must be greater than zero.
- `hop_size` must be greater than zero.
- `hop_size * 2 <= fft_size`, matching the `DFState::new` assertion.
- `erb_bands` must be greater than zero.
- `min_erb_freqs` must be greater than zero.
- The generated ERB filter bank must fit the configured FFT frequency bins using deterministic Lyre-owned validation before constructing `DFState`:
  - Compute `freq_size = fft_size / 2 + 1`.
  - Reject configs where `erb_bands > freq_size`.
  - Reject configs where `min_erb_freqs > freq_size`.
  - Reject configs where `erb_bands * min_erb_freqs > freq_size`.
  - After calling `deep_filter::erb_fb(sample_rate_hz, fft_size, erb_bands, min_erb_freqs)`, reject configs where the returned vector length is not `erb_bands`, any band width is zero, or the band-width sum is not `freq_size`.
  - No validation path may use `catch_unwind` as normal control flow; invalid user input must be rejected before reaching panic-prone `DFState::new` invariants.

Invalid process-boundary configuration must return an error from CLI/server startup with context. It must not fall back to defaults.

## Processing Behavior

- `DeepFilterNetNoiseCanceller` must construct `DFState::new(sample_rate_hz, fft_size, hop_size, erb_bands, min_erb_freqs)`.
- Frame validation must use the configured `sample_rate_hz`, `channels`, and `hop_size`.
- Valid DeepFilterNet input is a non-empty multiple of configured `hop_size`.
- Output sample count must match input sample count.
- The `NoiseCancellingAudioFrameProcessor` cache key must include the DeepFilterNet runtime config so a server config change cannot reuse an incompatible canceller state inside one process.
- Existing per-user noise settings (`provider`, `intensity`, `voice_activity_threshold`) remain part of the canceller cache key.

## CLI Split

The plan should split `crates/lyre-app/src/cli.rs` into focused modules while preserving public behavior:

- Keep command type definitions in `cli.rs` or a small module.
- Move ICE/TURN/state parsing and tests into smaller files as needed.
- Add DeepFilterNet config parsing in its own focused module.
- Do not rewrite unrelated CLI behavior.

## Web API Split

The plan should split `crates/lyre-web/src/api.rs` before modifying `AppState`:

- Move `AppState` construction and process-local runtime accessors into a focused `crates/lyre-web/src/app_state.rs` module.
- Keep route handlers and router assembly in `api.rs`.
- Preserve the existing public `lyre_web::{router, AppState}` re-export.
- Do not move unrelated route behavior unless required by the `AppState` split.
- No touched `lyre-web` Rust source file may remain above 400 LOC after the split.

## Acceptance Criteria

- `build_noise_canceller(Deepfilternet)` with default runtime config produces the same valid 48 kHz mono 480/960-sample behavior as before.
- A custom runtime config with `fft_size = 1920` and `hop_size = 960` accepts 960-sample chunks and rejects 480-sample chunks.
- Invalid runtime config, such as `fft_size = 480` with `hop_size = 480`, is rejected before constructing `DFState`.
- Invalid ERB config, such as `fft_size = 960`, `erb_bands = 300`, and `min_erb_freqs = 2`, is rejected deterministically before constructing `DFState`.
- `lyre serve --deepfilternet-hop-size 960 --deepfilternet-fft-size 1920` passes the config into `ServeConfig`.
- `LYRE_DEEPFILTERNET_HOP_SIZE` and related env vars work through clap env support.
- `lyre config print` includes default DeepFilterNet runtime config values.
- Existing tests for RNNoise, REST, WebRPC, server media, frontend typecheck, and packaging remain unaffected.
- No touched Rust source file exceeds 400 LOC after the CLI split.

## Documentation

- `README.md` must document the new DeepFilterNet runtime flags and clarify that this is libDF DSP/STFT configuration, not pretrained neural model inference.
- `MEMORY.md` must record the decision not to claim neural model support from `deep_filter = 0.2.5`.
- `docs/roadmap.md` must move DeepFilterNet runtime configuration to Completed and keep full pretrained neural inference as a future item.

## Verification

Required targeted verification:

- `cargo fmt --all --check`
- `cargo test -p lyre-noise-cancelling deepfilternet`
- `cargo test -p lyre-app deepfilternet`
- `cargo check -p lyre-web --all-targets`

Required final verification:

- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`
- `cd frontend && npm test -- --run`
- `cd frontend && npm run typecheck`
- `cd frontend && npm run lint`
- `cd frontend && npm run build`
- `git diff --check`
