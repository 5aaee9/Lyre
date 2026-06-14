# TURN REST Credentials Implementation Plan

> **For agentic workers:** REQUIRED WORKFLOW: Execute this plan as a `$sdd-workflow` subtask. Use the SDD reviewer gates before implementation and after implementation. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Generate short-lived TURN REST credentials for configured TURN/TURNS ICE servers while preserving static ICE behavior by default.

**Architecture:** Add credential generation to `lyre-core::webrtc`, carry optional TURN REST config through CLI/server state, and apply credentials when `/api/webrtc/ice-servers` is requested. The response schema stays unchanged, so WebRPC generated frontend files do not change.

**Tech Stack:** Rust, HMAC-SHA1 (`hmac`, `sha1`), standard base64 (`base64`), Axum, Clap, Serde.

---

## File Structure

Every implementation task below is part of this `$sdd-workflow` increment. Do not treat any task as complete until the final independent SDD implementation reviewer returns `VERDICT: APPROVE`.

- Modify `Cargo.toml`: add workspace dependencies `base64`, `hmac`, `sha1`.
- Modify `crates/lyre-core/Cargo.toml`: depend on those workspace crates.
- Modify `crates/lyre-core/src/webrtc.rs`: add TURN REST config/generator/apply logic and tests.
- Modify `crates/lyre-core/src/lib.rs`: re-export TURN REST types.
- Modify `crates/lyre-app/src/cli.rs`: add flags/env parsing and tests.
- Modify `crates/lyre-app/src/main.rs`: pass effective TURN REST config to `ServeConfig`.
- Modify `crates/lyre-web/src/server.rs`: carry config into `AppState`.
- Modify `crates/lyre-web/src/api.rs`: apply generated credentials in `ice_servers()` and add route test.
- Modify `README.md`, `MEMORY.md`, `docs/roadmap.md`, `AGENTS.md`.
- `AGENTS.md` change is limited to documenting third-party `hmac`, `sha1`, and `base64` under Key Dependencies/project conventions; do not add a new workspace crate entry.

## Task 1: Core TURN REST Credential Generation

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/lyre-core/Cargo.toml`
- Modify: `crates/lyre-core/src/webrtc.rs`
- Modify: `crates/lyre-core/src/lib.rs`

**Workflow:** Execute this task under the active `$sdd-workflow`; its changes are reviewed by the final independent implementation reviewer.

- [x] **Step 1: Add dependencies**

In root `Cargo.toml` workspace dependencies add:

```toml
base64 = "0.22"
hmac = "0.12"
sha1 = "0.10"
```

In `crates/lyre-core/Cargo.toml`, add:

```toml
base64.workspace = true
hmac.workspace = true
sha1.workspace = true
```

- [x] **Step 2: Add TURN REST types and error**

In `crates/lyre-core/src/webrtc.rs`, add imports:

```rust
use base64::{engine::general_purpose::STANDARD, Engine as _};
use hmac::{Hmac, Mac};
use sha1::Sha1;
use thiserror::Error;
```

Add:

```rust
type HmacSha1 = Hmac<Sha1>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnRestCredentialsConfig {
    pub secret: String,
    pub ttl_seconds: u64,
    pub identity: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnRestCredentials {
    pub username: String,
    pub credential: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TurnRestCredentialsError {
    #[error("TURN REST shared secret must not be blank")]
    BlankSecret,
    #[error("TURN REST identity must not be blank")]
    BlankIdentity,
}
```

- [x] **Step 3: Add generator and application functions**

Add:

```rust
pub fn generate_turn_rest_credentials(
    config: &TurnRestCredentialsConfig,
    now_unix_seconds: u64,
) -> Result<TurnRestCredentials, TurnRestCredentialsError> {
    if config.secret.trim().is_empty() {
        return Err(TurnRestCredentialsError::BlankSecret);
    }
    if config.identity.trim().is_empty() {
        return Err(TurnRestCredentialsError::BlankIdentity);
    }
    let username = format!(
        "{}:{}",
        now_unix_seconds.saturating_add(config.ttl_seconds),
        config.identity.trim()
    );
    let mut mac = HmacSha1::new_from_slice(config.secret.as_bytes())
        .expect("HMAC-SHA1 accepts keys of any length");
    mac.update(username.as_bytes());
    let credential = STANDARD.encode(mac.finalize().into_bytes());
    Ok(TurnRestCredentials { username, credential })
}

pub fn ice_servers_with_turn_rest_credentials(
    servers: &[IceServerConfig],
    config: Option<&TurnRestCredentialsConfig>,
    now_unix_seconds: u64,
) -> Result<Vec<IceServerConfig>, TurnRestCredentialsError> {
    let Some(config) = config else {
        return Ok(servers.to_vec());
    };
    let credentials = generate_turn_rest_credentials(config, now_unix_seconds)?;
    Ok(servers
        .iter()
        .map(|server| {
            if is_turn_server(server) {
                IceServerConfig {
                    urls: server.urls.clone(),
                    username: Some(credentials.username.clone()),
                    credential: Some(credentials.credential.clone()),
                }
            } else {
                server.clone()
            }
        })
        .collect())
}

fn is_turn_server(server: &IceServerConfig) -> bool {
    server.urls.iter().any(|url| {
        let lower = url.to_ascii_lowercase();
        lower.starts_with("turn:") || lower.starts_with("turns:")
    })
}
```

- [x] **Step 4: Add core tests**

Add tests:

```rust
#[test]
fn turn_rest_credentials_match_standard_test_vector() {
    let config = TurnRestCredentialsConfig {
        secret: "turn-secret".to_owned(),
        ttl_seconds: 3600,
        identity: "lyre".to_owned(),
    };

    let credentials = generate_turn_rest_credentials(&config, 1_700_000_000).unwrap();

    assert_eq!(credentials.username, "1700003600:lyre");
    assert_eq!(credentials.credential, "kPvQ2eDShdPecE5A3hgn5A03mIc=");
}

#[test]
fn turn_rest_credentials_reject_blank_secret_or_identity() {
    let mut config = TurnRestCredentialsConfig {
        secret: " ".to_owned(),
        ttl_seconds: 3600,
        identity: "lyre".to_owned(),
    };
    assert_eq!(
        generate_turn_rest_credentials(&config, 1),
        Err(TurnRestCredentialsError::BlankSecret)
    );
    config.secret = "secret".to_owned();
    config.identity = " ".to_owned();
    assert_eq!(
        generate_turn_rest_credentials(&config, 1),
        Err(TurnRestCredentialsError::BlankIdentity)
    );
}

#[test]
fn turn_rest_credentials_apply_only_to_turn_servers() {
    let servers = vec![
        IceServerConfig {
            urls: vec!["stun:stun.example:3478".to_owned()],
            username: None,
            credential: None,
        },
        IceServerConfig {
            urls: vec!["turn:turn.example:3478".to_owned()],
            username: Some("static-user".to_owned()),
            credential: Some("static-pass".to_owned()),
        },
        IceServerConfig {
            urls: vec!["turns:turn.example:5349".to_owned()],
            username: None,
            credential: None,
        },
    ];
    let config = TurnRestCredentialsConfig {
        secret: "turn-secret".to_owned(),
        ttl_seconds: 3600,
        identity: "lyre".to_owned(),
    };

    let rewritten = ice_servers_with_turn_rest_credentials(&servers, Some(&config), 1_700_000_000).unwrap();

    assert_eq!(rewritten[0], servers[0]);
    assert_eq!(rewritten[1].username.as_deref(), Some("1700003600:lyre"));
    assert_eq!(rewritten[1].credential.as_deref(), Some("kPvQ2eDShdPecE5A3hgn5A03mIc="));
    assert_eq!(rewritten[2].username.as_deref(), Some("1700003600:lyre"));
}
```

- [x] **Step 5: Re-export core types**

In `crates/lyre-core/src/lib.rs`, re-export:

```rust
generate_turn_rest_credentials,
ice_servers_with_turn_rest_credentials,
TurnRestCredentials,
TurnRestCredentialsConfig,
TurnRestCredentialsError,
```

## Task 2: CLI and Server Config

**Files:**
- Modify: `crates/lyre-app/src/cli.rs`
- Modify: `crates/lyre-app/src/main.rs`
- Modify: `crates/lyre-web/src/server.rs`

**Workflow:** Execute this task under the active `$sdd-workflow`; its changes are reviewed by the final independent implementation reviewer.

- [x] **Step 1: Add CLI args**

In `ServeArgs`, add:

```rust
#[arg(long, env = "LYRE_TURN_REST_SECRET")]
pub turn_rest_secret: Option<String>,
#[arg(long, default_value_t = 3600, env = "LYRE_TURN_REST_TTL_SECONDS")]
pub turn_rest_ttl_seconds: u64,
#[arg(long, default_value = "lyre", env = "LYRE_TURN_REST_IDENTITY")]
pub turn_rest_identity: String,
```

- [x] **Step 2: Add effective config method and error**

Add an error variant:

```rust
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TurnRestConfigError {
    #[error("TURN REST shared secret must not be blank")]
    BlankSecret,
    #[error("TURN REST identity must not be blank")]
    BlankIdentity,
}
```

Add:

```rust
pub fn effective_turn_rest_credentials(
    &self,
) -> Result<Option<lyre_core::TurnRestCredentialsConfig>, TurnRestConfigError> {
    let Some(secret) = &self.turn_rest_secret else {
        return Ok(None);
    };
    if secret.trim().is_empty() {
        return Err(TurnRestConfigError::BlankSecret);
    }
    if self.turn_rest_identity.trim().is_empty() {
        return Err(TurnRestConfigError::BlankIdentity);
    }
    Ok(Some(lyre_core::TurnRestCredentialsConfig {
        secret: secret.clone(),
        ttl_seconds: self.turn_rest_ttl_seconds,
        identity: self.turn_rest_identity.trim().to_owned(),
    }))
}
```

- [x] **Step 3: Thread config into server**

In `crates/lyre-web/src/server.rs`, add `pub turn_rest_credentials: Option<TurnRestCredentialsConfig>` to `ServeConfig`.

In `serve()`, call:

```rust
router(AppState::new(config.ice_servers, config.turn_rest_credentials))
```

Update `AppState::new` in Task 3 accordingly.

In `crates/lyre-app/src/main.rs`, set:

```rust
turn_rest_credentials: args.effective_turn_rest_credentials()?,
```

- [x] **Step 4: Add CLI tests**

Update existing `ServeArgs` test literals to include:

```rust
turn_rest_secret: None,
turn_rest_ttl_seconds: 3600,
turn_rest_identity: "lyre".to_owned(),
```

Add tests for:

```rust
#[test]
fn parses_turn_rest_cli_args() {
    let cli = Cli::try_parse_from([
        "lyre",
        "serve",
        "--turn-rest-secret",
        "secret",
        "--turn-rest-ttl-seconds",
        "600",
        "--turn-rest-identity",
        "room-a",
    ])
    .unwrap();
    match cli.command {
        Commands::Serve(args) => {
            let config = args.effective_turn_rest_credentials().unwrap().unwrap();
            assert_eq!(config.secret, "secret");
            assert_eq!(config.ttl_seconds, 600);
            assert_eq!(config.identity, "room-a");
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn turn_rest_secret_env_enables_default_ttl_and_identity() {
    std::env::set_var("LYRE_TURN_REST_SECRET", "secret");
    let cli = Cli::try_parse_from(["lyre", "serve"]).unwrap();
    match cli.command {
        Commands::Serve(args) => {
            let config = args.effective_turn_rest_credentials().unwrap().unwrap();
            assert_eq!(config.ttl_seconds, 3600);
            assert_eq!(config.identity, "lyre");
        }
        Commands::Config(_) => panic!("expected serve"),
    }
    std::env::remove_var("LYRE_TURN_REST_SECRET");
}

#[test]
fn rejects_blank_turn_rest_cli_secret() {
    let cli = Cli::try_parse_from(["lyre", "serve", "--turn-rest-secret", " "]).unwrap();
    match cli.command {
        Commands::Serve(args) => {
            assert_eq!(
                args.effective_turn_rest_credentials(),
                Err(TurnRestConfigError::BlankSecret)
            );
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn rejects_blank_turn_rest_env_secret() {
    std::env::set_var("LYRE_TURN_REST_SECRET", " ");
    let cli = Cli::try_parse_from(["lyre", "serve"]).unwrap();
    match cli.command {
        Commands::Serve(args) => {
            assert_eq!(
                args.effective_turn_rest_credentials(),
                Err(TurnRestConfigError::BlankSecret)
            );
        }
        Commands::Config(_) => panic!("expected serve"),
    }
    std::env::remove_var("LYRE_TURN_REST_SECRET");
}
```

## Task 3: Apply Credentials in Web API

**Files:**
- Modify: `crates/lyre-web/src/api.rs`

**Workflow:** Execute this task under the active `$sdd-workflow`; its changes are reviewed by the final independent implementation reviewer.

- [x] **Step 1: Extend AppState**

Add:

```rust
pub turn_rest_credentials: Option<lyre_core::TurnRestCredentialsConfig>,
```

Change `AppState::new` to accept:

```rust
pub fn new(
    ice_servers: Vec<IceServerConfig>,
    turn_rest_credentials: Option<lyre_core::TurnRestCredentialsConfig>,
) -> Self
```

Update `Default` to call `Self::new(default_ice_servers(), None)`.

- [x] **Step 2: Apply credentials at request time**

In `ice_servers`, compute current unix time and apply:

```rust
let now = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .expect("system clock must be after unix epoch")
    .as_secs();
let servers = lyre_core::ice_servers_with_turn_rest_credentials(
    &state.ice_servers,
    state.turn_rest_credentials.as_ref(),
    now,
)?;
Ok(Json(servers))
```

Change the handler return type to `Result<Json<Vec<IceServerConfig>>, ApiError>` and add `ApiError` conversion if needed.

- [x] **Step 3: Preserve error context**

If adding an `ApiError` variant is needed, include the underlying error string in the JSON response and do not discard it.

- [x] **Step 4: Add route test**

Add a route test that configures one TURN server with static credentials plus a TURN REST secret and asserts:

- status is OK,
- `username` ends with `:lyre`,
- `credential` is present,
- credential is not `static-pass`,
- response does not contain the secret.

## Task 4: Documentation and Project Guidance

**Files:**
- Modify: `README.md`
- Modify: `MEMORY.md`
- Modify: `docs/roadmap.md`
- Modify: `AGENTS.md`

**Workflow:** Execute this task under the active `$sdd-workflow`; its changes are reviewed by the final independent implementation reviewer.

- [x] **Step 1: Update README**

Document:

- `--turn-rest-secret`
- `--turn-rest-ttl-seconds`
- `--turn-rest-identity`
- `LYRE_TURN_REST_SECRET`
- `LYRE_TURN_REST_TTL_SECONDS`
- `LYRE_TURN_REST_IDENTITY`
- `LYRE_TURN_REST_SECRET` is never returned to browsers.
- `proto/lyre.ridl` is unchanged because the ICE server response shape is unchanged.

- [x] **Step 2: Update MEMORY**

Append:

```markdown
## 2026-06-14 TURN REST Credentials

- Added short-lived TURN REST credential generation for configured TURN/TURNS ICE servers.
- Kept STUN-only servers and default static ICE behavior unchanged when no shared secret is configured.
- Did not add `turn-rs` runtime yet; this prepares credentials for any TURN server that supports the shared-secret REST credential pattern.
```

- [x] **Step 3: Update roadmap**

Move short-lived TURN credential generation to Completed. Keep embedded `turn-rs` TURN service evaluation as Next.

- [x] **Step 4: Update AGENTS.md**

Update the Key Dependencies/project conventions area to mention `hmac`, `sha1`, and `base64` are third-party dependencies used for TURN REST credential generation. Do not add a workspace crate entry; this increment adds no new local crate.

## Task 5: Verification and Review

**Files:**
- All changed files.

**Workflow:** Execute this task under the active `$sdd-workflow`; do not commit or push until independent implementation review returns `VERDICT: APPROVE`.

- [x] **Step 1: Run Rust verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: all commands exit 0.

- [x] **Step 2: Run frontend verification**

Run:

```bash
cd frontend
npm test -- --run
npm run typecheck
npm run lint
npm run build
cp src/lib/lyre.gen.ts /tmp/lyre.gen.before.ts
npm run generate:webrpc
cmp /tmp/lyre.gen.before.ts src/lib/lyre.gen.ts
```

Expected: all commands exit 0 and WebRPC generation is unchanged.

- [x] **Step 3: Request SDD implementation review**

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

- [x] **Step 4: Final verification and diff review after reviewer approval**

After `VERDICT: APPROVE`, rerun Rust and frontend verification, then run:

```bash
git diff --check
git diff --stat
git diff
git status --short
```

Expected: only intended files are changed and no whitespace errors are reported.

- [ ] **Step 5: Commit and push**

Stage intended files, commit with the Lore commit protocol, then run `git push`. If push fails due to missing remote/upstream/credentials, report the exact error with the successful local commit SHA.
