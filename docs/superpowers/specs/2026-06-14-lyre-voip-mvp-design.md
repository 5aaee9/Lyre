# Lyre VOIP MVP Design

## Goal

Build the first runnable Lyre VOIP application skeleton in this repository. The result must provide a Rust workspace with core room state, a Clap CLI, an Axum web API, a noise-cancelling crate, a Next.js frontend, Docker packaging, GitHub Actions publishing to GHCR, and project memory/roadmap documentation.

This milestone is an MVP foundation, not a complete production media server. It must implement room/user state, REST endpoints, WebSocket signalling for WebRTC offers/answers/ICE candidates, frontend room entry and settings screens, and typed tests. Real server-side audio decoding, RNNoise inference, DeepFilterNet inference, TURN/STUN provisioning, authentication, persistence, and horizontal scaling are documented as follow-up work.

## Assumptions

- Room `DEFAULT` is built in and always available.
- A client that omits a nickname gets an automatically assigned nickname from the server.
- "Remember Room ID" stores the last room id in browser local storage.
- WebRTC media flows peer-to-peer in this MVP. The server provides signalling and room presence only.
- Noise cancellation settings are modelled and validated now. The actual audio processing function is a deterministic passthrough placeholder in the server-side crate so future RNNoise/DeepFilterNet bindings have a stable API.
- WebRPC is represented by a typed JSON contract module and REST endpoints in this milestone. Code-generated WebRPC IDL is a later task because there is no existing frontend/backend contract in the empty repository.

## Architecture

The repository becomes a Cargo workspace plus a `frontend/` Next.js application:

- `crates/lyre-core`: domain model and in-memory room registry.
- `crates/lyre-noise-cancelling`: noise cancellation configuration and processing trait.
- `crates/lyre-web`: Axum REST and WebSocket signalling server.
- `crates/lyre-app`: Clap CLI binary that starts the web server and exposes a config inspection command.
- `frontend/`: Next.js app router UI using React, Tailwind CSS, shadcn-style components, TypeScript, and Vitest.
- `.github/workflows/docker.yml`: Docker image build/publish workflow for GHCR.
- `Dockerfile`: multi-stage build for the Rust server binary and Next.js standalone frontend.

All Rust crates must keep files under 400 lines and use `anyhow` context at runtime/system boundaries. Logs and errors must preserve source context.

Frontend packaging uses Next.js standalone output so arbitrary shareable room URLs work. `frontend/next.config.ts` sets `output: "standalone"` and `npm run build` emits `.next/standalone`. Packaging produces two Docker images: `lyre-api` runs `lyre serve` for REST/WebSocket API on port `8080`, and `lyre-web` runs the Next.js standalone frontend on port `3000`. The frontend is configured with `APP_BASE_URL` for its own public URL and `APP_API_URL` for the Rust API URL; WebSocket URLs are derived from `APP_API_URL`.

## Backend Design

### Core domain

`lyre-core` owns:

- `RoomId`, `UserId`, and `Nickname`.
- `NoiseCancellationConfig` with provider enum `off | rnnoise | deepfilternet`.
- `UserProfile` containing id, nickname, joined timestamp, and noise config.
- `RoomSnapshot` containing room id and sorted users.
- `RoomRegistry`, an async-safe in-memory registry backed by `DashMap`.

Acceptance criteria:

- Default room id constant is `DEFAULT`.
- Room ids are case-sensitive, trimmed at the HTTP boundary, and must contain at least one non-whitespace character.
- Any valid non-empty room id is auto-created on first join or first read.
- Joining a room with blank/missing nickname assigns a deterministic `Guest N` nickname.
- Leaving a room removes only that user.
- Listing room users returns stable ordering for tests and UI.
- Tests cover default room creation, arbitrary room auto-creation, blank room id rejection, nickname assignment, joining, leaving, and noise config serialization.

### Noise cancellation

`lyre-noise-cancelling` owns:

- `NoiseCanceller` trait.
- `PassthroughNoiseCanceller` implementation.
- `NoiseProvider` and `NoiseCancellationConfig` re-export or shared-compatible types from `lyre-core`.

Acceptance criteria:

- `off`, `rnnoise`, and `deepfilternet` provider names serialize to frontend-friendly lowercase strings.
- Passthrough processing returns the input samples unchanged.
- Tests cover config defaults and passthrough output.

### Web API and signalling

`lyre-web` owns:

- `AppState` with shared `RoomRegistry`.
- REST routes:
  - `GET /health` returns `{ "status": "ok" }`.
  - `GET /api/rooms/:room_id` returns a room snapshot.
  - `POST /api/rooms/:room_id/join` accepts optional nickname/noise config and returns a user profile plus room snapshot.
  - `POST /api/rooms/:room_id/leave` accepts a user id and returns the updated room snapshot.
  - `GET /api/noise/providers` returns supported noise providers and default parameters.
- WebSocket route:
  - `GET /api/rooms/:room_id/ws?user_id=...` upgrades to signalling.
  - The MVP broadcasts WebRTC signalling messages to other connected peers in the same room.

Signalling messages use this JSON envelope:

```json
{
  "type": "offer",
  "room_id": "DEFAULT",
  "sender_id": "user_01H...",
  "recipient_id": "user_01J...",
  "payload": {}
}
```

Required message variants:

- `offer`: payload is `{ "sdp": "..." }`; sent by a user and forwarded to `recipient_id` when present, otherwise broadcast to all other room peers.
- `answer`: payload is `{ "sdp": "..." }`; sent by a user and forwarded to `recipient_id` when present, otherwise broadcast to all other room peers.
- `ice-candidate`: payload is `{ "candidate": "...", "sdp_mid": "0", "sdp_m_line_index": 0 }`; sent by a user and forwarded using the same recipient rule.
- `user-joined`: payload is `{ "user": UserProfile }`; emitted by the server to other connected peers after a successful join.
- `user-left`: payload is `{ "user_id": "..." }`; emitted by the server to other connected peers after leave/disconnect.
- `room-snapshot`: payload is `{ "room": RoomSnapshot }`; emitted by the server to the newly connected WebSocket.
- `error`: payload is `{ "message": "..." }`; emitted only to the offending socket when JSON is malformed, the message type is unknown, `room_id` does not match the path, or `sender_id` does not match the query user.

Invalid client messages do not panic or close the socket unless the WebSocket transport itself fails.

Acceptance criteria:

- Route tests cover health, join, list room, leave, provider list, blank room id rejection, and malformed leave body.
- WebSocket message types serialize predictably, recipient/broadcast decisions are unit tested, and invalid-message handling returns an `error` message.
- Runtime bind/listen errors preserve context with `anyhow`.

### CLI

`lyre-app` owns:

- `lyre serve --host 0.0.0.0 --port 8080` to start the Axum server.
- `lyre config print` to print default room and supported noise providers as JSON.

Acceptance criteria:

- Clap derives parse both commands.
- CLI tests validate default args and config print shape.

## Frontend Design

The first screen is the actual room entry workflow, not a landing page.

Routes:

- `/`: room join screen with Room ID input, nickname input, remember toggle, noise provider selector, join button, and default `DEFAULT` room.
- `/room/[roomId]`: room view with current user, user list, connection/signalling status, leave action, and basic local media/signalling controls. This dynamic route is preserved so room links are shareable.
- `/settings`: nickname and noise cancellation settings page.

Room page MVP behavior:

- The page creates a browser `RTCPeerConnection` only after the user clicks a "Connect audio" control.
- The page calls `navigator.mediaDevices.getUserMedia({ audio: true })` only from that user action.
- The page opens the room WebSocket after the room has a current user id. It handles `room-snapshot`, `user-joined`, `user-left`, and `error` messages to update presence/status.
- The page sends `offer`, `answer`, and `ice-candidate` messages only from the user-triggered audio connection flow.
- If microphone access is unavailable or denied, the UI reports the failure and keeps room presence/signalling usable.
- Tests mock browser media APIs; they must prove the initial render does not request a microphone.
- The MVP does not require successful real peer audio in automated tests.

UI constraints:

- Use Tailwind CSS and local shadcn-style primitives in `frontend/src/components/ui`.
- Keep the interface compact and operational, not marketing-style.
- Use local storage for remembered room id and settings.
- Use TypeScript types shared from a frontend API module matching the backend JSON contract.

Acceptance criteria:

- Vitest covers local storage helpers and API request serialization.
- TypeScript check passes.
- The app can run with `npm run dev`.
- The UI does not require a real microphone to render and test.

## Packaging and CI

Docker:

- Build the Rust API image as `lyre-api`, copying only the `lyre` binary into the runtime image and running `lyre serve` on port `8080`.
- Build the Next.js standalone image as `lyre-web`, copying `.next/standalone`, `.next/static`, and `public` into the runtime image and running Next.js on port `3000`.

GitHub Actions:

- On push to `main` and tags, build and push images to `ghcr.io/${{ github.repository }}/lyre-api` and `ghcr.io/${{ github.repository }}/lyre-web`.
- Use `docker/login-action`, `docker/setup-buildx-action`, `docker/metadata-action`, and `docker/build-push-action`.

Acceptance criteria:

- Workflow file exists and references GHCR.
- Docker packaging has separate API and frontend targets. API target sets `LYRE_API_BIND=0.0.0.0:8080`; frontend target sets `PORT=3000`, `APP_BASE_URL`, and `APP_API_URL`.

## Documentation

Required docs:

- `MEMORY.md` records major design decisions and implementation details.
- `docs/roadmap.md` records completed MVP items and next TODOs.
- `README.md` explains local backend/frontend commands, tests, and Docker image purpose.
- `AGENTS.md` is updated if new crates or important package conventions are added.

## Verification

Fresh verification before final completion:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`; if `cargo-nextest` is unavailable, install-free fallback is not allowed as the final claim, but `cargo test --workspace` may be reported as secondary evidence.
- `npm test -- --run`
- `npm run typecheck`
- `npm run lint`
- `docker build --target api -t lyre-api:local .`
- `docker build --target web -t lyre-web:local .`

If an environment lacks Node, npm, Docker, or cargo-nextest, report that exact blocker and any successful substitute checks.
