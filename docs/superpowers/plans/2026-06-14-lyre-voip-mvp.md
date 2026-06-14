# Lyre VOIP MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first runnable Lyre VOIP MVP foundation with Rust backend crates, Axum REST/WebSocket signalling, Clap CLI, Next.js frontend, Docker/GHCR packaging, tests, and docs.

**Architecture:** Create a Cargo workspace with four focused crates and a Next.js standalone app. The backend owns room/presence/signalling state; the frontend owns room entry, shareable dynamic room routes, settings, and user-triggered WebRTC media setup. Docker builds two images: `lyre-api` for Rust API/WebSocket and `lyre-web` for the Next.js frontend.

**Tech Stack:** Rust, Tokio, Axum, Tower, DashMap, Serde, Clap, Next.js App Router standalone output, React, Tailwind CSS, shadcn-style local components, TypeScript, Vitest, GitHub Actions, Docker Buildx.

---

## File Structure

- Create `Cargo.toml`: workspace members and shared dependency versions.
- Create `crates/lyre-core`: domain model, room registry, tests.
- Create `crates/lyre-noise-cancelling`: processing trait, passthrough implementation, tests.
- Create `crates/lyre-web`: Axum router, REST handlers, signalling types, WebSocket hub, tests.
- Create `crates/lyre-app`: Clap CLI binary and parser tests.
- Create `frontend/`: Next.js standalone app with local UI primitives, API client, shareable room/settings pages, and Vitest tests.
- Create `.github/workflows/docker.yml`: GHCR Docker publish workflow.
- Create `Dockerfile`: API image target and frontend image target.
- Modify `AGENTS.md`: list the frontend and the new crates now present in the repo.
- Create `README.md`, `MEMORY.md`, and `docs/roadmap.md`.

## Task 1: Rust Workspace and Core Domain

**Files:**
- Create: `Cargo.toml`
- Create: `crates/lyre-core/Cargo.toml`
- Create: `crates/lyre-core/src/lib.rs`
- Create: `crates/lyre-core/src/ids.rs`
- Create: `crates/lyre-core/src/noise.rs`
- Create: `crates/lyre-core/src/room.rs`

- [ ] **Step 1: Create workspace manifest**

Add a workspace with members for all four crates and shared dependencies:

```toml
[workspace]
members = [
    "crates/lyre-app",
    "crates/lyre-core",
    "crates/lyre-noise-cancelling",
    "crates/lyre-web",
]
resolver = "2"

[workspace.package]
edition = "2021"
license = "MIT"
version = "0.1.0"

[workspace.dependencies]
anyhow = "1"
axum = { version = "0.8", features = ["ws"] }
chrono = { version = "0.4", features = ["serde"] }
clap = { version = "4", features = ["derive"] }
dashmap = "6"
futures-util = "0.3"
http-body-util = "0.1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tokio = { version = "1", features = ["macros", "net", "rt-multi-thread", "signal", "sync"] }
tower = { version = "0.5", features = ["util"] }
tower-http = { version = "0.6", features = ["fs", "trace", "cors"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
ulid = "1"
```

- [ ] **Step 2: Implement core ids and noise config**

`crates/lyre-core/src/ids.rs` defines `DEFAULT_ROOM_ID`, `RoomId`, `UserId`, constructors, `as_str`, and serde-friendly derives. `RoomId::parse_boundary` trims external input and returns an error for blank ids. `UserId::new` creates `user_<ulid>`.

`crates/lyre-core/src/noise.rs` defines `NoiseProvider` with serde lowercase names, `NoiseCancellationConfig { provider, intensity, voice_activity_threshold }`, defaults, and `supported_noise_providers()`.

- [ ] **Step 3: Implement room registry**

`crates/lyre-core/src/room.rs` defines `UserProfile`, `RoomSnapshot`, `JoinRoomRequest`, `JoinRoomResponse`, `LeaveRoomRequest`, `RoomRegistry`, and methods:

```rust
impl RoomRegistry {
    pub fn new() -> Self;
    pub fn snapshot(&self, room_id: RoomId) -> RoomSnapshot;
    pub fn join(&self, room_id: RoomId, request: JoinRoomRequest) -> JoinRoomResponse;
    pub fn leave(&self, room_id: &RoomId, user_id: &UserId) -> RoomSnapshot;
}
```

Use DashMap for rooms and stable user sorting by nickname then id.

- [ ] **Step 4: Add core tests**

Add unit tests for:

- `DEFAULT` room id parsing.
- blank room id rejection.
- arbitrary room auto-creation via `snapshot`.
- blank nickname becomes `Guest 1`.
- leave removes only the matching user.
- noise provider serde uses `off`, `rnnoise`, `deepfilternet`.

- [ ] **Step 5: Verify core crate**

Run: `cargo test -p lyre-core`

Expected: all core tests pass.

## Task 2: Noise Cancelling Crate

**Files:**
- Create: `crates/lyre-noise-cancelling/Cargo.toml`
- Create: `crates/lyre-noise-cancelling/src/lib.rs`

- [ ] **Step 1: Create crate manifest**

Depend on `lyre-core` and workspace serde dependencies.

- [ ] **Step 2: Implement passthrough API**

Define:

```rust
pub trait NoiseCanceller {
    fn process_frame(&self, samples: &[f32]) -> Vec<f32>;
}

#[derive(Debug, Clone)]
pub struct PassthroughNoiseCanceller {
    config: NoiseCancellationConfig,
}
```

Expose `NoiseProvider`, `NoiseCancellationConfig`, `PassthroughNoiseCanceller::new`, and `config()`.

- [ ] **Step 3: Add tests**

Test default config and unchanged sample output for `[-0.2, 0.0, 0.3]`.

- [ ] **Step 4: Verify crate**

Run: `cargo test -p lyre-noise-cancelling`

Expected: all tests pass.

## Task 3: Axum Web API and Signalling

**Files:**
- Create: `crates/lyre-web/Cargo.toml`
- Create: `crates/lyre-web/src/lib.rs`
- Create: `crates/lyre-web/src/api.rs`
- Create: `crates/lyre-web/src/error.rs`
- Create: `crates/lyre-web/src/server.rs`
- Create: `crates/lyre-web/src/signalling.rs`

- [ ] **Step 1: Create web crate manifest**

Depend on `lyre-core`, `anyhow`, `axum`, `futures-util`, `serde`, `serde_json`, `tokio`, `tower`, `tower-http`, `tracing`, and `http-body-util` for tests.

- [ ] **Step 2: Define API errors**

`error.rs` defines an `ApiError` enum for bad room ids and invalid request bodies, implements `IntoResponse`, and returns JSON `{ "error": "..." }`.

- [ ] **Step 3: Define signalling types and routing decisions**

`signalling.rs` defines `SignalMessage`, `SignalPayload`, `SignalKind`, `SignalRoute`, `route_signal_message(path_room, query_user, message)`, and `invalid_signal(message)`.

Payload variants must be concrete:

```rust
pub enum SignalPayload {
    Offer { sdp: String },
    Answer { sdp: String },
    IceCandidate { candidate: String, sdp_mid: Option<String>, sdp_m_line_index: Option<u16> },
    UserJoined { user: UserProfile },
    UserLeft { user_id: UserId },
    RoomSnapshot { room: RoomSnapshot },
    Error { message: String },
}
```

Tests must cover:

- offer JSON serialization.
- answer JSON serialization.
- ice-candidate JSON serialization.
- user-joined JSON serialization with `{ "user": ... }`.
- user-left JSON serialization with `{ "user_id": ... }`.
- room-snapshot JSON serialization with `{ "room": ... }`.
- error JSON serialization with `{ "message": ... }`.
- targeted recipient route.
- broadcast route when `recipient_id` is absent.
- error when message room differs from WebSocket path.
- error when sender differs from query user.
- targeted delivery reaches only the intended peer.
- broadcast delivery excludes the sender.
- `user-joined` is emitted to existing peers after a successful join.
- `user-left` is emitted after explicit leave and WebSocket disconnect.

- [ ] **Step 4: Implement AppState and REST routes**

`api.rs` defines `AppState`, `router(state)`, handlers for health, room snapshot, join, leave, and noise providers. Trim and validate room ids at handler boundaries.

- [ ] **Step 5: Implement WebSocket hub**

Use per-room peer registries stored in `DashMap<RoomId, ...>` so server code can send directly to connected peers. On connect, send `room-snapshot` to the new socket. Forward valid client messages only to `recipient_id` when present, otherwise to all connected peers in the room except the sender. Emit `user-joined` to existing peers after REST join creates a user, emit `user-left` after REST leave, and emit `user-left` when a WebSocket disconnect removes a connected peer. Send `error` back to the current socket for malformed JSON, unknown type, mismatched room id, or mismatched sender id.

Add an internal `PeerHub` API so REST handlers and WebSocket tasks use the same emission path:

```rust
impl PeerHub {
    pub async fn connect(&self, room_id: RoomId, user_id: UserId, tx: PeerSender) -> RoomSnapshot;
    pub async fn disconnect(&self, room_id: &RoomId, user_id: &UserId);
    pub async fn user_joined(&self, room_id: &RoomId, user: UserProfile);
    pub async fn user_left(&self, room_id: &RoomId, user_id: &UserId);
    pub async fn forward(&self, message: SignalMessage) -> SignalDelivery;
}
```

- [ ] **Step 6: Add route tests**

Use `tower::ServiceExt::oneshot` and `http_body_util::BodyExt` to test:

- `GET /health`.
- `GET /api/rooms/DEFAULT`.
- `POST /api/rooms/DEFAULT/join`.
- `POST /api/rooms/DEFAULT/leave`.
- `GET /api/noise/providers`.
- blank encoded room id rejection through direct parsing unit test.
- route-level blank/whitespace room id rejection with `/api/rooms/%20%20`.
- malformed leave body returns a client error.
- `PeerHub::user_joined` sends `user-joined` to existing peers.
- `PeerHub::user_left` sends `user-left` to existing peers.
- WebSocket disconnect removes the peer and emits `user-left`.
- targeted `offer` delivery reaches only `recipient_id`.
- broadcast `ice-candidate` delivery reaches other peers and excludes the sender.
- invalid client JSON/semantics produces an `error` message to the current peer.

- [ ] **Step 7: Verify web crate**

Run: `cargo test -p lyre-web`

Expected: all web tests pass.

## Task 4: Clap CLI

**Files:**
- Create: `crates/lyre-app/Cargo.toml`
- Create: `crates/lyre-app/src/main.rs`
- Create: `crates/lyre-app/src/cli.rs`

- [ ] **Step 1: Create app crate manifest**

Define binary package `lyre`; depend on `lyre-core`, `lyre-web`, `anyhow`, `clap`, `serde_json`, `tokio`, `tracing`, and `tracing-subscriber`.

- [ ] **Step 2: Implement Clap parser**

`cli.rs` defines:

```rust
#[derive(Debug, Parser)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Serve(ServeArgs),
    Config(ConfigCommand),
}
```

`ServeArgs` defaults to host `0.0.0.0` and port `8080`. `ConfigCommand::Print` prints default room and supported providers as JSON.

- [ ] **Step 3: Implement main runtime**

Initialize tracing, match commands, call `lyre_web::serve`, and preserve errors with `anyhow::Context`.

- [ ] **Step 4: Add CLI tests**

Test `Cli::try_parse_from(["lyre", "serve"])`, custom host/port, and config print JSON builder.

- [ ] **Step 5: Verify app crate**

Run: `cargo test -p lyre-app`

Expected: all CLI tests pass.

## Task 5: Next.js Frontend

**Files:**
- Create: `frontend/package.json`
- Create: `frontend/next.config.ts`
- Create: `frontend/tsconfig.json`
- Create: `frontend/vitest.config.ts`
- Create: `frontend/eslint.config.mjs`
- Create: `frontend/postcss.config.mjs`
- Create: `frontend/public/.gitkeep`
- Create: `frontend/src/app/layout.tsx`
- Create: `frontend/src/app/page.tsx`
- Create: `frontend/src/app/room/[roomId]/page.tsx`
- Create: `frontend/src/app/settings/page.tsx`
- Create: `frontend/src/app/globals.css`
- Create: `frontend/src/components/ui/button.tsx`
- Create: `frontend/src/components/ui/input.tsx`
- Create: `frontend/src/components/ui/select.tsx`
- Create: `frontend/src/components/ui/switch.tsx`
- Create: `frontend/src/lib/api.ts`
- Create: `frontend/src/lib/storage.ts`
- Create: `frontend/src/lib/signalling.ts`
- Create: `frontend/src/lib/webrtc.ts`
- Create: `frontend/src/lib/storage.test.ts`
- Create: `frontend/src/lib/api.test.ts`
- Create: `frontend/src/lib/signalling.test.ts`
- Create: `frontend/src/app/room/[roomId]/room-client.test.tsx`

- [ ] **Step 1: Create package and config files**

Use Next.js standalone output:

```ts
const nextConfig = {
  output: "standalone",
};

export default nextConfig;
```

Scripts:

```json
{
  "dev": "next dev",
  "build": "next build",
  "lint": "eslint .",
  "typecheck": "tsc --noEmit",
  "test": "vitest"
}
```

- [ ] **Step 2: Implement frontend API and storage modules**

`api.ts` exports TypeScript types matching backend JSON and functions for room snapshot, join, leave, and provider list. It reads `APP_API_URL` as the API base URL. `signalling.ts` exports WebSocket message types, `createRoomSocket(roomId, userId)`, message encoders for offer/answer/ice-candidate, and handlers for room-snapshot/user-joined/user-left/error; it derives `ws://` or `wss://` from `APP_API_URL`. `storage.ts` wraps `localStorage` keys for room id, nickname, remember-room toggle, and noise settings. `APP_BASE_URL` is used when constructing shareable room links.

- [ ] **Step 3: Implement UI primitives**

Create small shadcn-style local components using Tailwind classes: button, input, select, switch. Keep APIs minimal and typed.

- [ ] **Step 4: Implement pages**

`/` renders the room entry workflow and navigates to `/room/<encodedRoomId>` after a successful join. `/room/[roomId]` reads `roomId` from the route params, joins if needed, opens the room WebSocket after it has a current user id, renders user list, connection status, leave button, and "Connect audio" button. `/settings` renders nickname and noise provider settings.

- [ ] **Step 5: Implement WebRTC helper**

`webrtc.ts` exports `createAudioPeerConnection()` that constructs `RTCPeerConnection` and requests microphone only when invoked by the room page click handler. The room page sends offer/answer/ice-candidate messages through `signalling.ts` only after the click-triggered audio connection flow starts.

- [ ] **Step 6: Add frontend tests**

Vitest tests cover:

- local storage helpers.
- API request URL/body serialization.
- WebSocket URL construction and offer/answer/ice-candidate message encoding.
- presence reducer behavior for room-snapshot, user-joined, user-left, and error messages.
- room page initial render does not call `getUserMedia`.
- room page initial render opens presence WebSocket after current user id is available.
- room page does not send offer/answer/ICE before clicking "Connect audio".

- [ ] **Step 7: Verify frontend**

Run from `frontend/`:

- `npm test -- --run`
- `npm run typecheck`
- `npm run lint`
- `npm run build`

Expected: all commands pass or report environment dependency blockers exactly.

## Task 6: Docker and GitHub Actions

**Files:**
- Create: `Dockerfile`
- Create: `.dockerignore`
- Create: `.github/workflows/docker.yml`

- [ ] **Step 1: Create Dockerfile**

Use targets:

- `rust-build`: `rust:1-bookworm`, copy workspace, `cargo build --release -p lyre-app`.
- `api`: slim Debian target, copy `/target/release/lyre`, set `LYRE_API_BIND=0.0.0.0:8080`, expose `8080`, run `["/usr/local/bin/lyre", "serve"]`.
- `frontend-build`: `node:22-bookworm-slim`, `npm ci`, `npm run build`.
- `web`: Node slim target, copy `.next/standalone`, `.next/static`, and `public`, set `PORT=3000`, `APP_BASE_URL=http://localhost:3000`, and `APP_API_URL=http://localhost:8080`, expose `3000`, run `["node", "server.js"]`.

- [ ] **Step 2: Create dockerignore**

Ignore `.git`, `.omx`, `target`, `frontend/node_modules`, and `frontend/.next`.

- [ ] **Step 3: Create GHCR workflow**

Workflow triggers on pushes to `main`, version tags, and manual dispatch. It logs into GHCR with `GITHUB_TOKEN`, generates Docker metadata for `ghcr.io/${{ github.repository }}/lyre-api` and `ghcr.io/${{ github.repository }}/lyre-web`, and uses Buildx to build and push both targets.

- [ ] **Step 4: Verify Docker config**

Run:

```bash
docker build --target api -t lyre-api:local .
docker build --target web -t lyre-web:local .
```

Expected: both images build when Docker is available.

## Task 7: Documentation and Repo Guidance Before Final Verification

**Files:**
- Create: `README.md`
- Create: `MEMORY.md`
- Create: `docs/roadmap.md`
- Modify: `AGENTS.md`

- [ ] **Step 1: Confirm implementation tasks are complete**

Run `git status --short` and verify Tasks 1-6 implementation files exist before writing final docs. Do not run final verification until `MEMORY.md` and `docs/roadmap.md` are updated.

- [ ] **Step 2: Write README**

Include overview, backend commands, frontend commands, test commands, Docker build command, API route summary, and MVP scope limits.

- [ ] **Step 3: Write MEMORY**

Record decisions: peer-to-peer WebRTC for MVP, shareable Next.js dynamic room routes via standalone frontend image, separate API/frontend Docker images, passthrough noise canceller placeholder, in-memory room registry, and WebRPC represented by typed JSON contract for now.

- [ ] **Step 4: Write roadmap**

Mark completed MVP foundation items and list next TODOs: real WebRTC mesh flow hardening, TURN/STUN, server-side audio pipeline, RNNoise binding, DeepFilterNet binding, auth, persistence, production observability, generated WebRPC IDL.

- [ ] **Step 5: Update AGENTS**

Add the now-present `frontend/` convention and any new workspace crate notes without removing existing guidance.

## Task 8: Final Verification and Review

**Files:**
- All changed files.

- [ ] **Step 1: Run Rust formatting and lint**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: both pass.

- [ ] **Step 2: Run Rust tests**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: pass. If `cargo-nextest` is unavailable, record the blocker and also run `cargo test --workspace`.

- [ ] **Step 3: Run frontend verification**

Run:

```bash
cd frontend
npm test -- --run
npm run typecheck
npm run lint
npm run build
```

Expected: all pass.

- [ ] **Step 4: Run Docker build**

Run:

```bash
docker build --target api -t lyre-api:local .
docker build --target web -t lyre-web:local .
```

Expected: both pass when Docker daemon is available.

- [ ] **Step 5: Review diff**

Run:

```bash
git status --short
git diff --stat
```

Expected: only MVP implementation, docs, and workflow files are changed.
