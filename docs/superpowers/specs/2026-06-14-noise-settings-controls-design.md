# Noise Settings Controls Design

## Goal

Expose noise-cancellation parameters in the frontend settings and join flow so users can choose a provider and adjust its parameters instead of always using hard-coded defaults.

## Scope

In scope:

- Settings page controls for:
  - provider: `off`, `rnnoise`, `deepfilternet`
  - intensity: numeric value from `0` to `1`
  - voice activity threshold: numeric value from `0` to `1`
- Persist the full `NoiseCancellationConfig` in browser local storage.
- Room join uses the stored provider and parameters.
- Frontend tests cover persistence and UI behavior.
- README, MEMORY, and roadmap updates.

Out of scope:

- Real RNNoise or DeepFilterNet inference.
- Backend-side parameter validation beyond existing JSON typing.
- Per-provider advanced parameters beyond current `intensity` and `voice_activity_threshold`.

## UI Contract

Settings page:

- Shows provider select.
- Shows numeric controls for `intensity` and `voice_activity_threshold`.
- Controls use `step=0.05`, `min=0`, `max=1`.
- Saving writes the exact numeric values to local storage.

Join page:

- Keeps provider selector for quick choice.
- Uses stored `intensity` and `voice_activity_threshold` when joining.
- Changing provider on the join page updates provider only; it does not reset numeric parameters.

Storage:

- `readNoiseConfig()` returns defaults when storage is absent.
- `writeNoiseConfig()` stores all three fields.

## Testing

- Storage test proves all fields round-trip.
- Settings page test edits provider, intensity, threshold, saves, and verifies stored values.
- Join page/API test proves join request includes stored numeric parameter values.

## Documentation

- README mentions settings page supports provider plus intensity/voice activity threshold.
- MEMORY records the current parameter model.
- Roadmap keeps real RNNoise/DeepFilterNet implementations as future work.

## Verification

Run:

- `npm test -- --run`
- `npm run typecheck`
- `npm run lint`
- `npm run build`
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`
