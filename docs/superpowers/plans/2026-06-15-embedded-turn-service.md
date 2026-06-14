# Embedded TURN Service Implementation Plan

> **For agentic workers:** REQUIRED WORKFLOW: Execute this plan as a `$sdd-workflow` subtask. Use the SDD reviewer gates before implementation and after implementation. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an opt-in embedded UDP TURN relay service using the MIT `turn-server` crate from the `turn-rs` project.

**Architecture:** Add a `lyre-turn` adapter crate that owns all `turn-server` types. Parse embedded TURN options at the CLI boundary, inject a default embedded TURN ICE server only when no explicit ICE config exists, and run the TURN runtime alongside Axum with shared cancellation/error propagation.

**Tech Stack:** Rust, Tokio, Clap, Axum, `turn-server` 4.1.2 with UDP-only features.

---

## File Structure

- Modify `Cargo.toml`: add `crates/lyre-turn` workspace member and `turn-server` workspace dependency.
- Create `crates/lyre-turn/Cargo.toml`: adapter crate dependencies.
- Create `crates/lyre-turn/src/lib.rs`: Lyre-facing TURN config, validation helpers, `turn-server` config conversion, and runtime entrypoint.
- Modify `crates/lyre-web/Cargo.toml`: depend on `lyre-turn`.
- Modify `crates/lyre-web/src/server.rs`: carry optional embedded TURN config and orchestrate API/TURN tasks.
- Modify `crates/lyre-app/Cargo.toml`: depend on `lyre-turn`.
- Modify `crates/lyre-app/src/cli.rs`: add CLI/env options, parse embedded TURN config, auto-inject ICE server, tests.
- Modify `crates/lyre-app/src/main.rs`: pass embedded TURN config to `ServeConfig`.
- Modify `README.md`, `MEMORY.md`, `docs/roadmap.md`, `AGENTS.md`.

## Task 1: Add `lyre-turn` Adapter Crate

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/lyre-turn/Cargo.toml`
- Create: `crates/lyre-turn/src/lib.rs`

**Workflow:** Execute this task under the active `$sdd-workflow`; its changes are reviewed by the final independent implementation reviewer.

- [x] **Step 1: Add workspace member and dependency**

In root `Cargo.toml`, add `"crates/lyre-turn"` to `workspace.members`.

In `[workspace.dependencies]`, add:

```toml
turn-server = { version = "4.1.2", default-features = false, features = ["udp"] }
```

- [x] **Step 2: Create crate manifest**

Create `crates/lyre-turn/Cargo.toml`:

```toml
[package]
name = "lyre-turn"
edition.workspace = true
license.workspace = true
version.workspace = true

[dependencies]
anyhow.workspace = true
thiserror.workspace = true
tokio.workspace = true
turn-server.workspace = true
```

- [x] **Step 3: Implement adapter types and validation**

Create `crates/lyre-turn/src/lib.rs` with these public types and helpers:

```rust
use anyhow::{Context, Result};
use std::{net::SocketAddr, str::FromStr};
use thiserror::Error;
use turn_server::{
    config::{Auth, Config, Interface, Server},
    service::session::ports::PortRange,
};

const MIN_RELAY_PORT: u16 = 49152;
const MAX_RELAY_PORT: u16 = 65535;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedTurnConfig {
    pub listen: SocketAddr,
    pub external: SocketAddr,
    pub realm: String,
    pub port_range: EmbeddedTurnPortRange,
    pub static_auth_secret: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddedTurnPortRange {
    pub start: u16,
    pub end: u16,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EmbeddedTurnConfigError {
    #[error("embedded TURN requires a TURN REST shared secret")]
    MissingTurnRestSecret,
    #[error("embedded TURN realm must not be blank")]
    BlankRealm,
    #[error("embedded TURN port range must use <start>..<end>, got `{value}`")]
    InvalidPortRangeFormat { value: String },
    #[error("embedded TURN relay ports must be within 49152..65535, got `{value}`")]
    PortRangeOutsideRelayRange { value: String },
    #[error("embedded TURN relay port range start must be <= end, got `{value}`")]
    PortRangeStartAfterEnd { value: String },
}

impl Default for EmbeddedTurnPortRange {
    fn default() -> Self {
        Self {
            start: MIN_RELAY_PORT,
            end: MAX_RELAY_PORT,
        }
    }
}

impl FromStr for EmbeddedTurnPortRange {
    type Err = EmbeddedTurnConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some((start, end)) = value.split_once("..") else {
            return Err(EmbeddedTurnConfigError::InvalidPortRangeFormat {
                value: value.to_owned(),
            });
        };
        if start.is_empty() || end.is_empty() {
            return Err(EmbeddedTurnConfigError::InvalidPortRangeFormat {
                value: value.to_owned(),
            });
        }
        let start = start.parse::<u16>().map_err(|_| {
            EmbeddedTurnConfigError::InvalidPortRangeFormat {
                value: value.to_owned(),
            }
        })?;
        let end =
            end.parse::<u16>()
                .map_err(|_| EmbeddedTurnConfigError::InvalidPortRangeFormat {
                    value: value.to_owned(),
                })?;
        if start < MIN_RELAY_PORT || end > MAX_RELAY_PORT {
            return Err(EmbeddedTurnConfigError::PortRangeOutsideRelayRange {
                value: value.to_owned(),
            });
        }
        if start > end {
            return Err(EmbeddedTurnConfigError::PortRangeStartAfterEnd {
                value: value.to_owned(),
            });
        }
        Ok(Self { start, end })
    }
}

impl std::fmt::Display for EmbeddedTurnPortRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

impl EmbeddedTurnConfig {
    pub fn ice_server_url(&self) -> String {
        format!("turn:{}", self.external)
    }

    pub fn to_turn_server_config(&self) -> Config {
        Config {
            server: Server {
                realm: self.realm.clone(),
                interfaces: vec![Interface::Udp {
                    listen: self.listen,
                    external: self.external,
                    idle_timeout: 20,
                    mtu: 1500,
                }],
                port_range: PortRange::from_str(&self.port_range.to_string())
                    .expect("validated embedded TURN port range must parse"),
                max_threads: 1,
            },
            auth: Auth {
                static_auth_secret: Some(self.static_auth_secret.clone()),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

pub async fn run_embedded_turn(config: EmbeddedTurnConfig) -> Result<()> {
    let addr = config.listen;
    turn_server::start_server(config.to_turn_server_config())
        .await
        .with_context(|| format!("embedded TURN server failed at {addr}"))
}
```

- [x] **Step 4: Add adapter tests**

Add tests in `crates/lyre-turn/src/lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> EmbeddedTurnConfig {
        EmbeddedTurnConfig {
            listen: "0.0.0.0:3478".parse().unwrap(),
            external: "127.0.0.1:3478".parse().unwrap(),
            realm: "lyre.local".to_owned(),
            port_range: EmbeddedTurnPortRange::default(),
            static_auth_secret: "secret".to_owned(),
        }
    }

    #[test]
    fn embedded_turn_defaults_generate_local_ice_url() {
        let config = config();
        assert_eq!(config.ice_server_url(), "turn:127.0.0.1:3478");
    }

    #[test]
    fn parses_valid_port_range() {
        assert_eq!(
            "50000..50100".parse::<EmbeddedTurnPortRange>().unwrap(),
            EmbeddedTurnPortRange {
                start: 50000,
                end: 50100
            }
        );
    }

    #[test]
    fn rejects_invalid_port_ranges() {
        assert_eq!(
            "49152-65535".parse::<EmbeddedTurnPortRange>(),
            Err(EmbeddedTurnConfigError::InvalidPortRangeFormat {
                value: "49152-65535".to_owned()
            })
        );
        assert_eq!(
            "49152..".parse::<EmbeddedTurnPortRange>(),
            Err(EmbeddedTurnConfigError::InvalidPortRangeFormat {
                value: "49152..".to_owned()
            })
        );
        assert_eq!(
            "49151..65535".parse::<EmbeddedTurnPortRange>(),
            Err(EmbeddedTurnConfigError::PortRangeOutsideRelayRange {
                value: "49151..65535".to_owned()
            })
        );
        assert_eq!(
            "60000..59999".parse::<EmbeddedTurnPortRange>(),
            Err(EmbeddedTurnConfigError::PortRangeStartAfterEnd {
                value: "60000..59999".to_owned()
            })
        );
        assert_eq!(
            "49152..70000".parse::<EmbeddedTurnPortRange>(),
            Err(EmbeddedTurnConfigError::InvalidPortRangeFormat {
                value: "49152..70000".to_owned()
            })
        );
    }

    #[test]
    fn converts_to_turn_server_config() {
        let turn_config = config().to_turn_server_config();
        assert_eq!(turn_config.server.realm, "lyre.local");
        assert_eq!(turn_config.server.port_range.start(), 49152);
        assert_eq!(turn_config.server.port_range.end(), 65535);
        assert_eq!(turn_config.server.interfaces.len(), 1);
        assert_eq!(
            turn_config.auth.static_auth_secret.as_deref(),
            Some("secret")
        );
    }
}
```

## Task 2: CLI Config and ICE Auto-Injection

**Files:**
- Modify: `crates/lyre-app/Cargo.toml`
- Modify: `crates/lyre-app/src/cli.rs`

**Workflow:** Execute this task under the active `$sdd-workflow`; its changes are reviewed by the final independent implementation reviewer.

- [x] **Step 1: Add dependency**

In `crates/lyre-app/Cargo.toml`, add:

```toml
lyre-turn = { path = "../lyre-turn" }
```

- [x] **Step 2: Add CLI fields**

In `ServeArgs`, add:

```rust
#[arg(long, default_value_t = false, env = "LYRE_EMBEDDED_TURN")]
pub embedded_turn: bool,
#[arg(long, default_value = "0.0.0.0:3478", env = "LYRE_EMBEDDED_TURN_LISTEN")]
pub embedded_turn_listen: String,
#[arg(long, default_value = "127.0.0.1:3478", env = "LYRE_EMBEDDED_TURN_EXTERNAL")]
pub embedded_turn_external: String,
#[arg(long, default_value = "lyre.local", env = "LYRE_EMBEDDED_TURN_REALM")]
pub embedded_turn_realm: String,
#[arg(long, default_value = "49152..65535", env = "LYRE_EMBEDDED_TURN_PORT_RANGE")]
pub embedded_turn_port_range: String,
```

Update `default_serve_args()` test helper with these defaults.

- [x] **Step 3: Add CLI error variants**

Add these variants to the existing `TurnRestConfigError` enum:

```rust
#[error(transparent)]
EmbeddedTurn(#[from] lyre_turn::EmbeddedTurnConfigError),
#[error("embedded TURN listen address must be a valid socket address, got `{value}`")]
InvalidEmbeddedTurnListen { value: String },
#[error("embedded TURN external address must be an IP socket address, got `{value}`")]
InvalidEmbeddedTurnExternal { value: String },
```

- [x] **Step 4: Implement embedded TURN config method**

Add:

```rust
pub fn effective_embedded_turn_config(
    &self,
) -> Result<Option<lyre_turn::EmbeddedTurnConfig>, TurnRestConfigError> {
    if !self.embedded_turn {
        return Ok(None);
    }
    let turn_rest = self.effective_turn_rest_credentials()?;
    let Some(turn_rest) = turn_rest else {
        return Err(TurnRestConfigError::EmbeddedTurn(
            lyre_turn::EmbeddedTurnConfigError::MissingTurnRestSecret,
        ));
    };
    let listen = self
        .embedded_turn_listen
        .parse()
        .map_err(|_| TurnRestConfigError::InvalidEmbeddedTurnListen {
            value: self.embedded_turn_listen.clone(),
        })?;
    let external = self
        .embedded_turn_external
        .parse()
        .map_err(|_| TurnRestConfigError::InvalidEmbeddedTurnExternal {
            value: self.embedded_turn_external.clone(),
        })?;
    if self.embedded_turn_realm.trim().is_empty() {
        return Err(TurnRestConfigError::EmbeddedTurn(
            lyre_turn::EmbeddedTurnConfigError::BlankRealm,
        ));
    }
    let port_range = self.embedded_turn_port_range.parse()?;
    Ok(Some(lyre_turn::EmbeddedTurnConfig {
        listen,
        external,
        realm: self.embedded_turn_realm.trim().to_owned(),
        port_range,
        static_auth_secret: turn_rest.secret,
    }))
}
```

- [x] **Step 5: Auto-inject ICE when embedded TURN is enabled**

Change `effective_ice_servers` to delegate to a helper that can see embedded TURN:

```rust
pub fn effective_ice_servers(&self) -> Result<Vec<IceServerConfig>, IceServerConfigError> {
    if !self.ice_servers.is_empty() {
        return parse_ice_server_entries(&self.ice_servers);
    }
    if let Ok(raw) = env::var("LYRE_ICE_SERVERS") {
        let entries = raw.split(';').map(str::to_owned).collect::<Vec<_>>();
        return parse_ice_server_entries(&entries);
    }
    if self.embedded_turn {
        let external = self
            .embedded_turn_external
            .parse::<std::net::SocketAddr>()
            .map_err(|_| IceServerConfigError::InvalidEmbeddedTurnExternal {
                value: self.embedded_turn_external.clone(),
            })?;
        return Ok(vec![IceServerConfig {
            urls: vec![format!("turn:{external}")],
            username: None,
            credential: None,
        }]);
    }
    Ok(default_ice_servers())
}
```

Add `InvalidEmbeddedTurnExternal` to `IceServerConfigError` if this method performs parsing. Hostname external values must fail.

- [x] **Step 6: Add CLI tests**

Add tests for:

- default `lyre serve` has `embedded_turn == false`.
- `--embedded-turn --turn-rest-secret secret` yields default config with `listen = 0.0.0.0:3478`, `external = 127.0.0.1:3478`, realm `lyre.local`, and port range `49152..65535`.
- missing secret with `--embedded-turn` returns `MissingTurnRestSecret`.
- custom listen/external/realm/port-range parse correctly.
- `LYRE_EMBEDDED_TURN=true` and `LYRE_TURN_REST_SECRET=secret` enable config. Use `ENV_LOCK`.
- hostname external such as `turn.example.com:3478` is rejected.
- invalid port ranges from the spec are rejected.
- embedded TURN with no explicit ICE returns one `turn:127.0.0.1:3478`.
- explicit CLI ICE and `LYRE_ICE_SERVERS` still take precedence over embedded TURN auto-injection.

## Task 3: Web Server Runtime Orchestration

**Files:**
- Modify: `crates/lyre-web/Cargo.toml`
- Modify: `crates/lyre-web/src/server.rs`
- Modify: `crates/lyre-app/src/main.rs`

**Workflow:** Execute this task under the active `$sdd-workflow`; its changes are reviewed by the final independent implementation reviewer.

- [x] **Step 1: Add dependency**

In `crates/lyre-web/Cargo.toml`, add:

```toml
lyre-turn = { path = "../lyre-turn" }
```

- [x] **Step 2: Extend `ServeConfig`**

Add:

```rust
pub embedded_turn: Option<lyre_turn::EmbeddedTurnConfig>,
```

Update call sites and tests.

- [x] **Step 3: Add orchestration helper**

In `crates/lyre-web/src/server.rs`, add a testable helper:

```rust
async fn run_api_and_optional_turn<A, T>(
    api: A,
    embedded_turn: Option<T>,
) -> Result<()>
where
    A: std::future::Future<Output = Result<()>> + Send + 'static,
    T: std::future::Future<Output = Result<()>> + Send + 'static,
{
    match embedded_turn {
        None => api.await,
        Some(turn) => {
            let mut api_task = tokio::spawn(api);
            let mut turn_task = tokio::spawn(turn);
            tokio::select! {
                api_result = &mut api_task => {
                    turn_task.abort();
                    api_result
                        .context("Lyre API task join failed while embedded TURN was enabled")?
                        .context("Lyre API task exited while embedded TURN was enabled")
                }
                turn_result = &mut turn_task => {
                    api_task.abort();
                    turn_result
                        .context("embedded TURN task join failed")?
                        .context("embedded TURN task exited")
                }
            }
        }
    }
}
```

Use this helper in `serve()`:

```rust
let api = async move {
    axum::serve(listener, router(AppState::new(config.ice_servers, config.turn_rest_credentials)))
        .await
        .context("Lyre API server failed")
};
let turn = config
    .embedded_turn
    .map(|turn_config| lyre_turn::run_embedded_turn(turn_config));
run_api_and_optional_turn(api, turn).await
```

This helper owns both runtime branches as `JoinHandle`s and explicitly aborts the sibling task when the first branch exits.

- [x] **Step 4: Add orchestration tests**

Add `#[cfg(test)]` tests in `server.rs` using futures that return immediately:

```rust
#[tokio::test]
async fn api_error_is_returned_when_turn_is_enabled() {
    let err = run_api_and_optional_turn(
        async { anyhow::bail!("api boom") },
        Some(async {
            std::future::pending::<Result<()>>().await
        }),
    )
    .await
    .unwrap_err();

    assert!(format!("{err:#}").contains("api boom"));
    assert!(format!("{err:#}").contains("Lyre API task exited"));
}

#[tokio::test]
async fn turn_error_is_returned_when_api_is_running() {
    let err = run_api_and_optional_turn(
        async {
            std::future::pending::<Result<()>>().await
        },
        Some(async { anyhow::bail!("turn boom") }),
    )
    .await
    .unwrap_err();

    assert!(format!("{err:#}").contains("turn boom"));
    assert!(format!("{err:#}").contains("embedded TURN task exited"));
}
```

- [x] **Step 5: Pass config from `main.rs`**

Update `crates/lyre-app/src/main.rs`:

```rust
let embedded_turn = args.effective_embedded_turn_config()?;
lyre_web::serve(ServeConfig {
    host,
    port,
    ice_servers,
    turn_rest_credentials: args.effective_turn_rest_credentials()?,
    embedded_turn,
})
```

Avoid silently recomputing inconsistent secrets if code can reuse a previously computed value cleanly.

## Task 4: Documentation and Roadmap

**Files:**
- Modify: `README.md`
- Modify: `MEMORY.md`
- Modify: `docs/roadmap.md`
- Modify: `AGENTS.md`

**Workflow:** Execute this task only after the independent implementation reviewer in Task 5 returns `VERDICT: APPROVE`. This is the required post-review documentation phase before final fresh verification, commit, and push.

- [x] **Step 1: Update README**

Document:

- `--embedded-turn`
- `--embedded-turn-listen`
- `--embedded-turn-external`
- `--embedded-turn-realm`
- `--embedded-turn-port-range`
- env equivalents
- embedded TURN requires `LYRE_TURN_REST_SECRET`
- default advertised ICE URL is `turn:127.0.0.1:3478`
- external must be an IP socket address, not a hostname
- explicit `LYRE_ICE_SERVERS` / `--ice-server` disables auto-injection
- embedded TURN does not do server-side noise cancellation
- `turn-server` does not enforce username timestamp expiry itself

- [x] **Step 2: Update MEMORY**

Append:

```markdown
## 2026-06-15 Embedded TURN Service

- Added an opt-in embedded UDP TURN relay using the MIT `turn-server` crate from the `turn-rs` project.
- Kept the GPL `turn-rs` crate out of the dependency graph; `lyre-turn` isolates the `turn-server` API.
- Embedded TURN advertises a local-only `turn:127.0.0.1:3478` URL by default and requires explicit IP socket configuration for public deployments.
- Confirmed TURN relay remains separate from server-side noise cancellation; media processing still needs a future WebRTC media relay.
```

- [x] **Step 3: Update roadmap**

Move embedded `turn-rs`/TURN service evaluation to Completed. Keep media relay/SFU-like server-side audio pipeline and RNNoise/DeepFilterNet bindings in Next.

- [x] **Step 4: Update AGENTS.md dependency guidance**

Add `crates/lyre-turn` to the workspace crate list and add `turn-server` under Key Dependencies as the MIT service crate from the `turn-rs` project used for embedded UDP TURN relay. Mention that the GPL `turn-rs` crate is intentionally not used.

## Task 5: Verification, SDD Review, Docs, and Commit

**Files:**
- All changed files.

**Workflow:** Execute this task under the active `$sdd-workflow`; do not update final docs, commit, or push until independent implementation review returns `VERDICT: APPROVE`.

- [x] **Step 1: Run targeted Rust checks**

Run:

```bash
cargo test -p lyre-turn
cargo test -p lyre-app cli::tests -- --nocapture
cargo test -p lyre-web server::tests -- --nocapture
```

- [x] **Step 2: Run full Rust verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

- [x] **Step 3: Confirm WebRPC/frontend unchanged**

If no frontend/proto files changed, run:

```bash
git diff --name-only | rg '^(frontend/|proto/)'
```

Expected: no output.

If frontend/proto files changed unexpectedly, run the standard frontend verification and WebRPC generation comparison.

- [x] **Step 4: Request SDD implementation review**

Dispatch a fresh independent reviewer with the approved spec, reviewed plan, diff, and verification output. Require:

```text
VERDICT: APPROVE | REVISE
SPEC_COVERAGE:
- [implemented requirement or missing requirement]
BLOCKERS:
- [blocking gap or "None"]
REQUIRED_CHANGES:
- [change or "None"]
```

- [x] **Step 5: Update documentation after reviewer approval**

After `VERDICT: APPROVE`, execute Task 4 documentation updates.

- [x] **Step 6: Final verification and diff review after docs update**

After docs are updated, rerun:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
git diff --check
git diff --stat
git diff
git status --short
```

- [ ] **Step 7: Commit and push**

Stage intended files, commit with the Lore commit protocol, then run `git push`. If push fails due to missing remote/upstream/credentials, report the exact error with the successful local commit SHA.
