# ICE Server Configuration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add configured STUN/TURN ICE servers from CLI/env through the Rust API into browser `RTCPeerConnection`.

**Architecture:** Store ICE server config as a core JSON-compatible type, parse configuration at CLI boundaries, carry it through `ServeConfig` into `AppState`, expose it via REST, and fetch it from the room page immediately before creating the peer connection.

**Tech Stack:** Rust, Clap, Axum, Serde, Next.js, TypeScript, Vitest.

---

## Task 1: Core ICE Model

**Files:**
- Modify: `crates/lyre-core/src/lib.rs`
- Add: `crates/lyre-core/src/webrtc.rs`

- [ ] **Step 1: Add model**

Create `IceServerConfig { urls: Vec<String>, username: Option<String>, credential: Option<String> }`.

- [ ] **Step 2: Add default**

Add `default_ice_servers() -> Vec<IceServerConfig>` returning `stun:stun.l.google.com:19302`.

- [ ] **Step 3: Add tests**

Test serde field names and default value.

- [ ] **Step 4: Verify**

Run: `cargo test -p lyre-core`

## Task 2: CLI and Env Parsing

**Files:**
- Modify: `crates/lyre-app/src/cli.rs`
- Modify: `crates/lyre-app/src/main.rs`

- [ ] **Step 1: Add CLI args**

Add repeated `--ice-server <value>` to `ServeArgs`.

- [ ] **Step 2: Parse effective ICE servers**

Implement `effective_ice_servers() -> Result<Vec<IceServerConfig>, IceServerConfigError>`:

- CLI values, if present, win.
- Else `LYRE_ICE_SERVERS`, if present, split by `;`.
- Else `default_ice_servers()`.
- Entry format: `url[,url...][|username|credential]`.
- Preserve order and duplicates.
- Error on blank entry, blank URL, extra `|`, or empty result.
- Error variants preserve invalid value.

- [ ] **Step 3: Include in config print**

`lyre config print` includes `ice_servers`.

- [ ] **Step 4: Pass into web config**

`main.rs` passes parsed ICE servers into `lyre_web::ServeConfig`.

- [ ] **Step 5: Add tests**

Cover default, repeated CLI values, env semicolon list, CLI precedence, valid TURN credentials, blank entry, blank URL, extra separator, and config print.

- [ ] **Step 6: Verify**

Run: `cargo test -p lyre-app`

## Task 3: Web API Route

**Files:**
- Modify: `crates/lyre-web/src/api.rs`
- Modify: `crates/lyre-web/src/server.rs`

- [ ] **Step 1: Store ICE servers**

Add `ice_servers: Arc<Vec<IceServerConfig>>` to `AppState`.

- [ ] **Step 2: Update server config**

Add `ice_servers: Vec<IceServerConfig>` to `ServeConfig`; `serve` creates `AppState::new(ice_servers)`.

- [ ] **Step 3: Add route**

Add `GET /api/webrtc/ice-servers` returning exactly the configured vector.

- [ ] **Step 4: Add tests**

Test default route output and custom route output preserving order and duplicate URLs.

- [ ] **Step 5: Verify**

Run: `cargo test -p lyre-web`

## Task 4: Frontend Fetch and Peer Connection

**Files:**
- Modify: `frontend/src/lib/api.ts`
- Modify: `frontend/src/lib/api.test.ts`
- Modify: `frontend/src/lib/webrtc.ts`
- Add: `frontend/src/lib/webrtc.test.ts`
- Modify: `frontend/src/app/room/[roomId]/room-client.tsx`
- Modify: `frontend/src/app/room/[roomId]/room-client.test.tsx`

- [ ] **Step 1: Add frontend API**

Add `IceServerConfig` type and `getIceServers()`.

- [ ] **Step 2: Update WebRTC helper**

Change `createAudioPeerConnection(iceServers)` to call `new RTCPeerConnection({ iceServers })`.

- [ ] **Step 3: Fetch before peer construction**

Room page `connectAudio` fetches ICE servers, then constructs peer connection with them. If fetch fails, set status to the error and do not call `getUserMedia` or construct `RTCPeerConnection`.

- [ ] **Step 4: Add tests**

Cover API URL, helper config, room click fetch-before-peer behavior, and fetch failure prevents media start.

- [ ] **Step 5: Verify**

Run from `frontend/`: `npm test -- --run && npm run typecheck && npm run lint && npm run build`

## Task 5: Docs and Final Verification

**Files:**
- Modify: `README.md`
- Modify: `MEMORY.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update docs**

Document `--ice-server`, `LYRE_ICE_SERVERS`, `/api/webrtc/ice-servers`, and the warning that configured TURN credentials are browser-visible.

- [ ] **Step 2: Update roadmap**

Move static TURN/STUN config to completed; keep dynamic short-lived TURN credentials as future work.

- [ ] **Step 3: Full verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
cd frontend
npm test -- --run
npm run typecheck
npm run lint
npm run build
```

- [ ] **Step 4: Review diff**

Run `git status --short` and `git diff --stat`.
