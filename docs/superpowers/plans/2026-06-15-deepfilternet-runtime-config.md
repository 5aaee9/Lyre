# DeepFilterNet Runtime Configuration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Lyre's existing server-side DeepFilterNet libDF runtime configurable from CLI/env through the web media runtime without claiming pretrained neural model inference.

**Architecture:** Add a `DeepFilterNetRuntimeConfig` owned by `lyre-noise-cancelling`, thread it through `lyre-app` and `lyre-web`, and keep current default behavior. Split oversized CLI/API files before adding config to satisfy the 400 LOC project rule.

**Tech Stack:** Rust 2021, clap env support, Axum app state, `deep_filter = 0.2.5`, existing Lyre media runtime.

---

## File Map

- Create `crates/lyre-app/src/cli/config.rs`: config-print DTO and default config printer.
- Create `crates/lyre-app/src/cli/deepfilternet.rs`: CLI args, validation wrapper, and tests for DeepFilterNet runtime config.
- Create `crates/lyre-app/src/cli/ice.rs`: ICE parsing errors/functions and ICE tests moved from `cli.rs`.
- Create `crates/lyre-app/src/cli/serve.rs`: `ServeArgs`, bind/state/TURN effective config methods, and serve-related tests moved from `cli.rs`.
- Modify `crates/lyre-app/Cargo.toml`: add path dependency on `lyre-noise-cancelling`.
- Replace `crates/lyre-app/src/cli.rs`: small module root containing `Cli`, `Commands`, `ConfigCommand`, and re-exports.
- Modify `crates/lyre-app/src/main.rs`: pass DeepFilterNet runtime config into `lyre_web::ServeConfig`.
- Create `crates/lyre-web/src/app_state.rs`: move `AppState` and runtime accessor methods out of `api.rs`, then add DeepFilterNet runtime config field construction.
- Modify `crates/lyre-web/src/api.rs`: import `AppState` from `app_state`, keep routes/handlers unchanged.
- Modify `crates/lyre-web/src/lib.rs`: export `app_state::AppState`.
- Modify `crates/lyre-web/src/server.rs`: add `deepfilternet_runtime` to `ServeConfig` and pass it to `AppState`.
- Modify `crates/lyre-web/src/media_runtime.rs`: add `WebMediaRuntime::with_deepfilternet_runtime`.
- Modify `crates/lyre-noise-cancelling/src/lib.rs`: add runtime config, validation, fallible constructors, and config-aware processor.
- Modify `crates/lyre-noise-cancelling/src/tests.rs`: cover default/custom/invalid DeepFilterNet runtime config.
- Modify `README.md`, `MEMORY.md`, and `docs/roadmap.md`: document the completed runtime configuration before implementation review.

## Task 1: Split CLI Modules Without Behavior Changes

**Files:**
- Create: `crates/lyre-app/src/cli/config.rs`
- Create: `crates/lyre-app/src/cli/ice.rs`
- Create: `crates/lyre-app/src/cli/serve.rs`
- Modify: `crates/lyre-app/src/cli.rs`
- Modify: `crates/lyre-app/src/main.rs`

- [ ] **Step 1: Move config print into `cli/config.rs`**

Create `crates/lyre-app/src/cli/config.rs` with:

```rust
use lyre_core::{default_ice_servers, supported_noise_providers, IceServerConfig, DEFAULT_ROOM_ID};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ConfigPrint {
    pub default_room_id: &'static str,
    pub noise_providers: Vec<lyre_core::NoiseCancellationConfig>,
    pub ice_servers: Vec<IceServerConfig>,
}

pub fn config_print() -> ConfigPrint {
    ConfigPrint {
        default_room_id: DEFAULT_ROOM_ID,
        noise_providers: supported_noise_providers(),
        ice_servers: default_ice_servers(),
    }
}
```

- [ ] **Step 2: Move ICE parsing into `cli/ice.rs`**

Move `IceServerConfigError`, `parse_ice_server_entries`, `parse_ice_server_entry`, and their tests from `cli.rs` into `crates/lyre-app/src/cli/ice.rs`. Make the parser public inside the crate:

```rust
pub(crate) fn parse_ice_server_entries(
    entries: &[String],
) -> Result<Vec<IceServerConfig>, IceServerConfigError>
```

Keep the exact existing error variants and assertions.

- [ ] **Step 3: Move serve args and serve tests into `cli/serve.rs`**

Move `ServeArgs`, `BindConfigError`, `TurnRestConfigError`, `StateFileConfigError`, `parse_api_bind`, and all serve-related tests into `crates/lyre-app/src/cli/serve.rs`.

In `serve.rs`, import:

```rust
use super::ice::{parse_ice_server_entries, IceServerConfigError};
use clap::Args;
use lyre_core::{default_ice_servers, IceServerConfig, TurnRestCredentialsConfig};
use std::{env, path::PathBuf};
use thiserror::Error;
```

- [ ] **Step 4: Replace `cli.rs` with a small module root**

`crates/lyre-app/src/cli.rs` should contain command definitions and re-exports:

```rust
mod config;
mod ice;
mod serve;

use clap::{Args, Parser, Subcommand};

pub use config::{config_print, ConfigPrint};
pub use serve::ServeArgs;

#[derive(Debug, Parser)]
#[command(name = "lyre")]
#[command(about = "Lyre VOIP server")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Serve(Box<ServeArgs>),
    Config(ConfigCommand),
}

#[derive(Debug, Args)]
pub struct ConfigCommand {
    #[command(subcommand)]
    pub command: ConfigSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum ConfigSubcommand {
    Print,
}
```

- [ ] **Step 5: Update `main.rs` config print call only if needed**

Keep existing `cli::config_print()` usage. No behavior change should be needed in `main.rs` for this task.

- [ ] **Step 6: Verify CLI split**

Run:

```bash
cargo fmt --all --check
cargo test -p lyre-app
wc -l crates/lyre-app/src/cli.rs crates/lyre-app/src/cli/*.rs
```

Expected:

- `cargo fmt --all --check` exits 0.
- `cargo test -p lyre-app` exits 0.
- No listed `lyre-app` Rust file exceeds 400 LOC.

## Task 2: Split Web App State Without Behavior Changes

**Files:**
- Create: `crates/lyre-web/src/app_state.rs`
- Modify: `crates/lyre-web/src/api.rs`
- Modify: `crates/lyre-web/src/lib.rs`

- [ ] **Step 1: Move `AppState` into `app_state.rs`**

Move these items unchanged from `api.rs` into `crates/lyre-web/src/app_state.rs`:

- `AppState` struct
- `impl Default for AppState`
- `impl AppState` block through `leave_room_persisted`

Use imports copied from `api.rs` that are actually needed by the moved code:

```rust
use crate::{
    error::ApiError,
    media_egress::{ProcessedAudioEgressFanout, ProcessedAudioEgressFrame},
    media_runtime::WebMediaRuntime,
    metrics::MetricsState,
    processed_audio_webrtc_egress_pump::ProcessedAudioWebRtcEgressPump,
    server_media_runtime_pump::ServerMediaRuntimePump,
    signalling::PeerHub,
    state_persistence::RoomStatePersistence,
};
use lyre_core::{
    default_ice_servers, AudioFrame, IceServerConfig, JoinRoomRequest, MediaRelayError,
    MediaRelayRegistry, ProcessedAudioFrame, RoomId, RoomRegistry,
};
use lyre_webrtc::{ServerMediaNegotiator, ServerMediaSessionRegistry, WebRtcStack};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
```

- [ ] **Step 2: Import `AppState` into `api.rs`**

Remove `AppState` definitions from `api.rs`. Add:

```rust
pub(crate) use crate::app_state::AppState;
```

Remove now-unused imports from `api.rs` such as `DashMap`, `WebMediaRuntime`, egress types, metrics state, pumps, persistence, WebRTC stack, `broadcast`, and `Mutex` if they are only used by the moved code.

- [ ] **Step 3: Export `app_state` in `lib.rs`**

Add:

```rust
pub mod app_state;
```

Change the re-export from:

```rust
pub use api::{router, AppState};
```

to:

```rust
pub use api::router;
pub use app_state::AppState;
```

- [ ] **Step 4: Verify web split**

Run:

```bash
cargo fmt --all --check
cargo test -p lyre-web api_tests::health_route_returns_ok
cargo check -p lyre-web --all-targets
wc -l crates/lyre-web/src/api.rs crates/lyre-web/src/app_state.rs
```

Expected:

- All commands exit 0.
- `api.rs` and `app_state.rs` are both below 400 LOC.

## Task 3: Add DeepFilterNet Runtime Config to Noise Cancelling

**Files:**
- Modify: `crates/lyre-noise-cancelling/src/lib.rs`
- Modify: `crates/lyre-noise-cancelling/src/tests.rs`

- [ ] **Step 1: Add config and error variants**

In `lib.rs`, add:

```rust
pub const DEEPFILTERNET_DEFAULT_FFT_SIZE: usize = 960;
pub const DEEPFILTERNET_DEFAULT_ERB_BANDS: usize = 32;
pub const DEEPFILTERNET_DEFAULT_MIN_ERB_FREQS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeepFilterNetRuntimeConfig {
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub fft_size: usize,
    pub hop_size: usize,
    pub erb_bands: usize,
    pub min_erb_freqs: usize,
}

impl Default for DeepFilterNetRuntimeConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
            channels: DEEPFILTERNET_CHANNELS,
            fft_size: DEEPFILTERNET_DEFAULT_FFT_SIZE,
            hop_size: DEEPFILTERNET_FRAME_SIZE,
            erb_bands: DEEPFILTERNET_DEFAULT_ERB_BANDS,
            min_erb_freqs: DEEPFILTERNET_DEFAULT_MIN_ERB_FREQS,
        }
    }
}
```

Extend `NoiseCancellationError` with:

```rust
#[error("invalid DeepFilterNet runtime config: {reason}")]
InvalidDeepFilterNetRuntimeConfig { reason: String },
```

- [ ] **Step 2: Add deterministic config validation**

Add:

```rust
impl DeepFilterNetRuntimeConfig {
    pub fn validate(self) -> Result<Self, NoiseCancellationError> {
        if self.sample_rate_hz != DEEPFILTERNET_SAMPLE_RATE_HZ {
            return Err(invalid_deepfilternet_runtime_config(format!(
                "sample_rate_hz must be {DEEPFILTERNET_SAMPLE_RATE_HZ}, got {}",
                self.sample_rate_hz
            )));
        }
        if self.channels != DEEPFILTERNET_CHANNELS {
            return Err(invalid_deepfilternet_runtime_config(format!(
                "channels must be {DEEPFILTERNET_CHANNELS}, got {}",
                self.channels
            )));
        }
        if self.fft_size == 0 {
            return Err(invalid_deepfilternet_runtime_config(
                "fft_size must be greater than zero",
            ));
        }
        if self.hop_size == 0 {
            return Err(invalid_deepfilternet_runtime_config(
                "hop_size must be greater than zero",
            ));
        }
        if self.hop_size.saturating_mul(2) > self.fft_size {
            return Err(invalid_deepfilternet_runtime_config(format!(
                "hop_size * 2 must be <= fft_size, got hop_size {} and fft_size {}",
                self.hop_size, self.fft_size
            )));
        }
        if self.erb_bands == 0 {
            return Err(invalid_deepfilternet_runtime_config(
                "erb_bands must be greater than zero",
            ));
        }
        if self.min_erb_freqs == 0 {
            return Err(invalid_deepfilternet_runtime_config(
                "min_erb_freqs must be greater than zero",
            ));
        }
        let freq_size = self.fft_size / 2 + 1;
        if self.erb_bands > freq_size {
            return Err(invalid_deepfilternet_runtime_config(format!(
                "erb_bands must be <= fft_size / 2 + 1 ({freq_size}), got {}",
                self.erb_bands
            )));
        }
        if self.min_erb_freqs > freq_size {
            return Err(invalid_deepfilternet_runtime_config(format!(
                "min_erb_freqs must be <= fft_size / 2 + 1 ({freq_size}), got {}",
                self.min_erb_freqs
            )));
        }
        if self
            .erb_bands
            .checked_mul(self.min_erb_freqs)
            .is_none_or(|minimum_bins| minimum_bins > freq_size)
        {
            return Err(invalid_deepfilternet_runtime_config(format!(
                "erb_bands * min_erb_freqs must fit fft_size / 2 + 1 ({freq_size}), got {} * {}",
                self.erb_bands, self.min_erb_freqs
            )));
        }
        let erb = df::erb_fb(
            self.sample_rate_hz as usize,
            self.fft_size,
            self.erb_bands,
            self.min_erb_freqs,
        );
        if erb.len() != self.erb_bands
            || erb.iter().any(|band_width| *band_width == 0)
            || erb.iter().sum::<usize>() != freq_size
        {
            return Err(invalid_deepfilternet_runtime_config(
                "ERB filter bank does not match configured FFT frequency bins",
            ));
        }
        Ok(self)
    }
}

fn invalid_deepfilternet_runtime_config(
    reason: impl Into<String>,
) -> NoiseCancellationError {
    NoiseCancellationError::InvalidDeepFilterNetRuntimeConfig {
        reason: reason.into(),
    }
}
```

- [ ] **Step 3: Make constructors config-aware**

Change:

```rust
pub fn build_noise_canceller(
    config: NoiseCancellationConfig,
) -> Result<Box<dyn NoiseCanceller + Send>, NoiseCancellationError>
```

to call a new function:

```rust
pub fn build_noise_canceller(
    config: NoiseCancellationConfig,
) -> Result<Box<dyn NoiseCanceller + Send>, NoiseCancellationError> {
    build_noise_canceller_with_runtime_config(config, DeepFilterNetRuntimeConfig::default())
}

pub fn build_noise_canceller_with_runtime_config(
    config: NoiseCancellationConfig,
    deepfilternet_runtime: DeepFilterNetRuntimeConfig,
) -> Result<Box<dyn NoiseCanceller + Send>, NoiseCancellationError> {
    match config.provider {
        NoiseProvider::Off => Ok(Box::new(PassthroughNoiseCanceller::new(config))),
        NoiseProvider::Rnnoise => Ok(Box::new(RnnoiseNoiseCanceller::new(config))),
        NoiseProvider::Deepfilternet => Ok(Box::new(DeepFilterNetNoiseCanceller::new(
            config,
            deepfilternet_runtime,
        )?)),
    }
}
```

Change `DeepFilterNetNoiseCanceller::new` to return `Result<Self, NoiseCancellationError>` and construct `DFState::new(...)` from the validated runtime config.

- [ ] **Step 4: Validate frames using runtime config**

Change `DeepFilterNetNoiseCanceller` to store `runtime: DeepFilterNetRuntimeConfig`, and change frame validation to:

```rust
fn validate_deepfilternet_frame(
    runtime: DeepFilterNetRuntimeConfig,
    frame: NoiseFrame<'_>,
) -> Result<(), NoiseCancellationError>
```

Expected fields in `InvalidFrameShape` must use `runtime.sample_rate_hz`, `runtime.channels`, and `runtime.hop_size`.

Process chunks with:

```rust
for chunk in frame.samples.chunks_exact(self.runtime.hop_size) {
    let mut output = vec![0.0; self.runtime.hop_size];
    self.state.process_frame(chunk, &mut output);
    samples.extend(output);
}
```

- [ ] **Step 5: Make processor cache config-aware**

Add a field to `NoiseCancellingAudioFrameProcessor`:

```rust
deepfilternet_runtime: DeepFilterNetRuntimeConfig,
```

Add:

```rust
impl NoiseCancellingAudioFrameProcessor {
    pub fn new(deepfilternet_runtime: DeepFilterNetRuntimeConfig) -> Self {
        Self {
            cancellers: Mutex::new(HashMap::new()),
            deepfilternet_runtime,
        }
    }
}

impl Default for NoiseCancellingAudioFrameProcessor {
    fn default() -> Self {
        Self::new(DeepFilterNetRuntimeConfig::default())
    }
}
```

Update `NoiseConfigKey` to include:

```rust
deepfilternet_runtime: Option<DeepFilterNetRuntimeConfig>,
```

The key should use `Some(self.deepfilternet_runtime)` only when `noise.provider == NoiseProvider::Deepfilternet`; use `None` for `Off` and `Rnnoise`.

Call `build_noise_canceller_with_runtime_config(noise.clone(), self.deepfilternet_runtime)`.

- [ ] **Step 6: Add targeted tests**

Add tests in `crates/lyre-noise-cancelling/src/tests.rs`:

```rust
#[test]
fn deepfilternet_custom_runtime_accepts_configured_hop_size() {
    let runtime = DeepFilterNetRuntimeConfig {
        fft_size: 1920,
        hop_size: 960,
        ..DeepFilterNetRuntimeConfig::default()
    };
    let mut canceller =
        build_noise_canceller_with_runtime_config(config(NoiseProvider::Deepfilternet), runtime)
            .unwrap();
    let input = decoded_opus_frame_samples();

    let output = canceller
        .process_frame(NoiseFrame {
            sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
            channels: DEEPFILTERNET_CHANNELS,
            samples: &input,
        })
        .unwrap();

    assert_eq!(output.samples.len(), 960);
    assert!(output.samples.iter().all(|sample| sample.is_finite()));
}

#[test]
fn deepfilternet_custom_runtime_rejects_default_hop_size() {
    let runtime = DeepFilterNetRuntimeConfig {
        fft_size: 1920,
        hop_size: 960,
        ..DeepFilterNetRuntimeConfig::default()
    };
    let mut canceller =
        build_noise_canceller_with_runtime_config(config(NoiseProvider::Deepfilternet), runtime)
            .unwrap();

    assert_eq!(
        canceller
            .process_frame(NoiseFrame {
                sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
                channels: DEEPFILTERNET_CHANNELS,
                samples: &[0.0; DEEPFILTERNET_FRAME_SIZE],
            })
            .unwrap_err(),
        NoiseCancellationError::InvalidFrameShape {
            provider: NoiseProvider::Deepfilternet,
            sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
            channels: DEEPFILTERNET_CHANNELS,
            samples: DEEPFILTERNET_FRAME_SIZE,
            expected_sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
            expected_channels: DEEPFILTERNET_CHANNELS,
            expected_samples: 960,
        }
    );
}

#[test]
fn deepfilternet_runtime_rejects_invalid_hop_and_erb_configs() {
    assert!(DeepFilterNetRuntimeConfig {
        fft_size: 480,
        hop_size: 480,
        ..DeepFilterNetRuntimeConfig::default()
    }
    .validate()
    .is_err());

    assert!(DeepFilterNetRuntimeConfig {
        fft_size: 960,
        erb_bands: 300,
        min_erb_freqs: 2,
        ..DeepFilterNetRuntimeConfig::default()
    }
    .validate()
    .is_err());
}
```

Adjust existing tests that instantiate `DeepFilterNetNoiseCanceller::new` directly if any.

- [ ] **Step 7: Verify noise crate**

Run:

```bash
cargo fmt --all --check
cargo test -p lyre-noise-cancelling deepfilternet
```

Expected: both commands exit 0.

## Task 4: Thread Runtime Config Through Web and CLI

**Files:**
- Modify: `crates/lyre-app/Cargo.toml`
- Create: `crates/lyre-app/src/cli/deepfilternet.rs`
- Modify: `crates/lyre-app/src/cli.rs`
- Modify: `crates/lyre-app/src/cli/config.rs`
- Modify: `crates/lyre-app/src/cli/serve.rs`
- Modify: `crates/lyre-app/src/main.rs`
- Modify: `crates/lyre-web/src/server.rs`
- Modify: `crates/lyre-web/src/app_state.rs`
- Modify: `crates/lyre-web/src/media_runtime.rs`
- Modify: `README.md`
- Modify: `MEMORY.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Add `lyre-app` dependency**

In `crates/lyre-app/Cargo.toml`, add:

```toml
lyre-noise-cancelling = { path = "../lyre-noise-cancelling" }
```

- [ ] **Step 2: Add `WebMediaRuntime` config constructor**

In `crates/lyre-web/src/media_runtime.rs`, add:

```rust
use lyre_noise_cancelling::{DeepFilterNetRuntimeConfig, NoiseCancellingAudioFrameProcessor};
```

Change `new` to delegate:

```rust
pub fn new(relays: Arc<MediaRelayRegistry>) -> Self {
    Self::with_deepfilternet_runtime(relays, DeepFilterNetRuntimeConfig::default())
}

pub fn with_deepfilternet_runtime(
    relays: Arc<MediaRelayRegistry>,
    deepfilternet_runtime: DeepFilterNetRuntimeConfig,
) -> Self {
    let sink = ProcessedAudioBroadcaster::default();
    let runtime = MediaRuntime::new(
        relays,
        NoiseCancellingAudioFrameProcessor::new(deepfilternet_runtime),
        sink.clone(),
    );
    Self { runtime, sink }
}
```

- [ ] **Step 3: Add config to `AppState` constructors**

In `app_state.rs`, import `DeepFilterNetRuntimeConfig`.

Keep `AppState::new` default behavior:

```rust
Self::with_room_state_persistence(
    ice_servers,
    turn_rest_credentials,
    None,
    DeepFilterNetRuntimeConfig::default(),
)
```

Change `with_room_state_persistence` signature to accept:

```rust
deepfilternet_runtime: DeepFilterNetRuntimeConfig,
```

Use:

```rust
let media_runtime = Arc::new(WebMediaRuntime::with_deepfilternet_runtime(
    Arc::clone(&media_relays),
    deepfilternet_runtime,
));
```

Update all call sites and tests to pass `DeepFilterNetRuntimeConfig::default()` where they call `with_room_state_persistence` directly. Existing internal imports that use `crate::api::AppState` remain valid through the single `pub(crate) use crate::app_state::AppState;` added to `api.rs` during Task 2. Do not add a second private `use crate::app_state::AppState;` import.

- [ ] **Step 4: Add config to server `ServeConfig`**

In `crates/lyre-web/src/server.rs`, import `DeepFilterNetRuntimeConfig`, add field:

```rust
pub deepfilternet_runtime: DeepFilterNetRuntimeConfig,
```

Pass it to `AppState::with_room_state_persistence(...)`.

- [ ] **Step 5: Add CLI DeepFilterNet module**

Create `crates/lyre-app/src/cli/deepfilternet.rs`:

```rust
use lyre_noise_cancelling::DeepFilterNetRuntimeConfig;
use thiserror::Error;

pub const DEFAULT_DEEPFILTERNET_FFT_SIZE: usize = 960;
pub const DEFAULT_DEEPFILTERNET_HOP_SIZE: usize = 480;
pub const DEFAULT_DEEPFILTERNET_ERB_BANDS: usize = 32;
pub const DEFAULT_DEEPFILTERNET_MIN_ERB_FREQS: usize = 2;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DeepFilterNetConfigError {
    #[error(transparent)]
    InvalidRuntimeConfig(#[from] lyre_noise_cancelling::NoiseCancellationError),
}

pub(crate) fn validate_deepfilternet_runtime(
    runtime: DeepFilterNetRuntimeConfig,
) -> Result<DeepFilterNetRuntimeConfig, DeepFilterNetConfigError> {
    Ok(runtime.validate()?)
}
```

If `NoiseCancellationError` cannot derive `Eq` because existing variants contain `NoiseProvider` only and strings, derive only `PartialEq` for `DeepFilterNetConfigError`.

- [ ] **Step 6: Add CLI fields and effective method**

In `serve.rs`, add fields to `ServeArgs`:

```rust
#[arg(
    long,
    default_value_t = super::deepfilternet::DEFAULT_DEEPFILTERNET_FFT_SIZE,
    env = "LYRE_DEEPFILTERNET_FFT_SIZE"
)]
pub deepfilternet_fft_size: usize,
#[arg(
    long,
    default_value_t = super::deepfilternet::DEFAULT_DEEPFILTERNET_HOP_SIZE,
    env = "LYRE_DEEPFILTERNET_HOP_SIZE"
)]
pub deepfilternet_hop_size: usize,
#[arg(
    long,
    default_value_t = super::deepfilternet::DEFAULT_DEEPFILTERNET_ERB_BANDS,
    env = "LYRE_DEEPFILTERNET_ERB_BANDS"
)]
pub deepfilternet_erb_bands: usize,
#[arg(
    long,
    default_value_t = super::deepfilternet::DEFAULT_DEEPFILTERNET_MIN_ERB_FREQS,
    env = "LYRE_DEEPFILTERNET_MIN_ERB_FREQS"
)]
pub deepfilternet_min_erb_freqs: usize,
```

Add:

```rust
pub fn effective_deepfilternet_runtime(
    &self,
) -> Result<DeepFilterNetRuntimeConfig, super::deepfilternet::DeepFilterNetConfigError> {
    super::deepfilternet::validate_deepfilternet_runtime(DeepFilterNetRuntimeConfig {
        fft_size: self.deepfilternet_fft_size,
        hop_size: self.deepfilternet_hop_size,
        erb_bands: self.deepfilternet_erb_bands,
        min_erb_freqs: self.deepfilternet_min_erb_freqs,
        ..DeepFilterNetRuntimeConfig::default()
    })
}
```

- [ ] **Step 7: Pass CLI config to server**

In `main.rs`, after `state_file`:

```rust
let deepfilternet_runtime = args.effective_deepfilternet_runtime()?;
```

Add it to `ServeConfig`.

- [ ] **Step 8: Include defaults in `config print`**

Avoid adding `serde` to `lyre-noise-cancelling` for this display-only CLI output. In `cli/config.rs`, add a local serializable DTO:

```rust
#[derive(Debug, Serialize)]
pub struct DeepFilterNetRuntimeConfigPrint {
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub fft_size: usize,
    pub hop_size: usize,
    pub erb_bands: usize,
    pub min_erb_freqs: usize,
}

impl From<lyre_noise_cancelling::DeepFilterNetRuntimeConfig>
    for DeepFilterNetRuntimeConfigPrint
{
    fn from(config: lyre_noise_cancelling::DeepFilterNetRuntimeConfig) -> Self {
        Self {
            sample_rate_hz: config.sample_rate_hz,
            channels: config.channels,
            fft_size: config.fft_size,
            hop_size: config.hop_size,
            erb_bands: config.erb_bands,
            min_erb_freqs: config.min_erb_freqs,
        }
    }
}
```

Then add the field to `ConfigPrint`:

```rust
pub deepfilternet_runtime: DeepFilterNetRuntimeConfigPrint,
```

Set it to:

```rust
lyre_noise_cancelling::DeepFilterNetRuntimeConfig::default().into()
```

- [ ] **Step 9: Add CLI tests**

In `serve.rs` tests, update `default_serve_args()` with new fields.

Add:

```rust
#[test]
fn parses_deepfilternet_runtime_cli_args() {
    let cli = Cli::try_parse_from([
        "lyre",
        "serve",
        "--deepfilternet-fft-size",
        "1920",
        "--deepfilternet-hop-size",
        "960",
        "--deepfilternet-erb-bands",
        "32",
        "--deepfilternet-min-erb-freqs",
        "2",
    ])
    .unwrap();
    match cli.command {
        Commands::Serve(args) => {
            let runtime = args.effective_deepfilternet_runtime().unwrap();
            assert_eq!(runtime.fft_size, 1920);
            assert_eq!(runtime.hop_size, 960);
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn deepfilternet_runtime_env_enables_config() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var("LYRE_DEEPFILTERNET_FFT_SIZE", "1920");
    std::env::set_var("LYRE_DEEPFILTERNET_HOP_SIZE", "960");
    let cli = Cli::try_parse_from(["lyre", "serve"]).unwrap();
    match cli.command {
        Commands::Serve(args) => {
            let runtime = args.effective_deepfilternet_runtime().unwrap();
            assert_eq!(runtime.fft_size, 1920);
            assert_eq!(runtime.hop_size, 960);
        }
        Commands::Config(_) => panic!("expected serve"),
    }
    std::env::remove_var("LYRE_DEEPFILTERNET_FFT_SIZE");
    std::env::remove_var("LYRE_DEEPFILTERNET_HOP_SIZE");
}

#[test]
fn rejects_invalid_deepfilternet_runtime_config() {
    let cli = Cli::try_parse_from([
        "lyre",
        "serve",
        "--deepfilternet-fft-size",
        "480",
        "--deepfilternet-hop-size",
        "480",
    ])
    .unwrap();
    match cli.command {
        Commands::Serve(args) => {
            assert!(args.effective_deepfilternet_runtime().is_err());
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}
```

Update `config_print_has_defaults` to assert:

```rust
assert_eq!(value["deepfilternet_runtime"]["hop_size"], 480);
```

- [ ] **Step 10: Update documentation**

Update `README.md`:

- Add the four DeepFilterNet runtime flags/env vars near the backend serve examples.
- State that these tune libDF DSP/STFT runtime parameters only.
- State that pretrained DeepFilterNet neural model inference is still future work because `deep_filter = 0.2.5` does not expose a Rust checkpoint loader.

Update `MEMORY.md` with a `2026-06-15 DeepFilterNet Runtime Configuration` section:

- Record that config is threaded from CLI/env into `WebMediaRuntime`.
- Record that Lyre deliberately does not claim neural model support from the current `deep_filter` crate.

Update `docs/roadmap.md`:

- Add DeepFilterNet runtime configuration to Completed.
- Keep full pretrained neural inference/configuration in Next.

- [ ] **Step 11: Verify threading**

Run:

```bash
cargo fmt --all --check
cargo test -p lyre-app deepfilternet
cargo check -p lyre-web --all-targets
wc -l crates/lyre-app/src/cli.rs crates/lyre-app/src/cli/*.rs crates/lyre-web/src/api.rs crates/lyre-web/src/app_state.rs
```

Expected:

- All commands exit 0.
- No listed touched Rust source file exceeds 400 LOC.

## Task 5: Targeted Integration Verification

**Files:**
- No required code edits unless tests fail.

- [ ] **Step 1: Run targeted SDD verification**

Run:

```bash
cargo fmt --all --check
cargo test -p lyre-noise-cancelling deepfilternet
cargo test -p lyre-app deepfilternet
cargo check -p lyre-web --all-targets
git diff --check
```

Expected: all commands exit 0.

- [ ] **Step 2: Inspect touched file sizes**

Run:

```bash
wc -l crates/lyre-app/src/cli.rs crates/lyre-app/src/cli/*.rs crates/lyre-web/src/api.rs crates/lyre-web/src/app_state.rs crates/lyre-noise-cancelling/src/lib.rs crates/lyre-noise-cancelling/src/tests.rs
```

Expected: every listed file is under 400 LOC. If `lyre-noise-cancelling/src/lib.rs` or `tests.rs` exceeds 400 LOC, split tests or implementation into focused modules before review.

## Final Verification and Commit

After independent implementation review returns `VERDICT: APPROVE`, run full final verification:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
cd frontend && npm test -- --run
cd frontend && npm run typecheck
cd frontend && npm run lint
cd frontend && npm run build
git diff --check
```

Commit with Lore protocol and push the current branch.
