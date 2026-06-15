# Per-User Server Media Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a single server-media browser client release server-side media resources without stopping the whole room relay.

**Architecture:** Add participant removal to `lyre-core::MediaRelayRegistry`, expose an AppState close method that stops only the matching server-media runtime pump and peer/session, document it in RIDL, and call it from frontend server relay Leave/startup-failure paths. Keep unmount local-only.

**Tech Stack:** Rust, Axum, WebRTC boundary crate, WebRPC RIDL/generated TypeScript, Next.js, Vitest.

**Reviewed Spec:** `docs/superpowers/specs/2026-06-15-per-user-server-media-cleanup-design.md`

---

## File Structure

- Modify `crates/lyre-core/src/media.rs`: add per-user participant removal.
- Modify `crates/lyre-core/src/media_tests.rs`: cover active removal and inactive/missing-room errors.
- Modify `crates/lyre-web/src/api_server_media_state.rs`: add `close_server_media_session_for_user`.
- Modify `crates/lyre-web/src/api_server_media.rs`: add `POST /api/rooms/{room_id}/server-media/close`.
- Modify `crates/lyre-web/src/api_server_media_tests.rs`: route behavior and idempotence.
- Modify `crates/lyre-web/src/api_webrtc_session_tests.rs` and/or `server_media_runtime_pump_tests.rs`: AppState closes only one user's session/pump.
- Modify `proto/lyre.ridl` and regenerate `frontend/src/lib/lyre.gen.ts`.
- Modify `frontend/src/lib/api.ts` and `frontend/src/lib/api.test.ts`: wrapper/types.
- Modify `frontend/src/app/room/[roomId]/room-client.tsx` and test: call cleanup on Leave and relevant startup failures only.
- Modify `MEMORY.md` and `docs/roadmap.md` only after implementation review approval.

Keep changed Rust source files under 400 LOC. If an existing test file is already over 400 LOC, avoid growing production files and keep test additions focused.

## Task 0: SDD Pre-Implementation Gate

**Files:**
- Read: `docs/superpowers/specs/2026-06-15-per-user-server-media-cleanup-design.md`
- Read: `docs/superpowers/plans/2026-06-15-per-user-server-media-cleanup.md`

- [x] **Step 1: Confirm approved spec review exists**

Required evidence before implementation:

```text
VERDICT: APPROVE
ISSUES:
- None
REQUIRED_CHANGES:
- None
```

- [x] **Step 2: Dispatch independent plan reviewer**

Dispatch an independent reviewer with the approved spec and this plan. Required verdict:

```text
VERDICT: APPROVE
```

Do not edit implementation files until the plan review approves.

## Task 1: Core Media Relay Participant Removal

**Files:**
- Modify: `crates/lyre-core/src/media.rs`
- Modify: `crates/lyre-core/src/media_tests.rs`

- [x] **Step 1: Add failing tests**

Add tests proving:

- Removing `user_a` from an active relay removes all of `user_a` tracks.
- The room remains active and keeps the previous `NoiseCancellationConfig`.
- Other participants and tracks remain.
- Removing from an inactive existing room returns `MediaRelayError::Inactive`.
- Removing from a missing room returns `MediaRelayError::Inactive` and does not create a room.

- [x] **Step 2: Implement `remove_participant`**

Add to `MediaRelayRegistry`:

```rust
pub fn remove_participant(
    &self,
    room_id: RoomId,
    user_id: &UserId,
) -> Result<MediaRelayRoomStatus, MediaRelayError>
```

Implementation should read the existing room without creating a missing room, require active relay, remove `user_id` from `participants`, and return `snapshot(room_id)`.

- [x] **Step 3: Verify core tests**

Run:

```bash
cargo test -p lyre-core media_tests::remove_participant
wc -l crates/lyre-core/src/media.rs
```

Expected: tests pass and `media.rs` remains under 400 LOC.

## Task 2: Backend Server-Media Close Operation and Route

**Files:**
- Modify: `crates/lyre-web/src/api_server_media_state.rs`
- Modify: `crates/lyre-web/src/api_server_media.rs`
- Modify: `crates/lyre-web/src/api_server_media_tests.rs`
- Modify: `crates/lyre-web/src/api_webrtc_session_tests.rs`
- Modify as needed: `crates/lyre-web/src/server_media_runtime_pump_tests.rs`

- [x] **Step 1: Add backend tests**

Add tests for:

- `AppState::close_server_media_session_for_user(room_id, user_id)` closes only matching server-media peer/session, stops only the matching runtime pump, removes only that relay participant, keeps relay status active, keeps noise unchanged, and leaves room egress pump active.
- Other users' server-media sessions and peer handles remain active.
- `POST /api/rooms/DEFAULT/server-media/close` returns `{ media_relay, session }` for an existing session.
- The route returns `{ media_relay, session: null }` when the relay participant exists but server-media session is already missing.
- The route returns a relay inactive error for missing/inactive relay without creating the room.

- [x] **Step 2: Implement AppState close**

Add a response DTO in `api_server_media_state.rs` or `api_server_media.rs`:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct CloseServerMediaSessionResponse {
    pub media_relay: lyre_core::MediaRelayRoomStatus,
    pub session: Option<lyre_webrtc::ServerMediaSessionStatus>,
}
```

Add AppState method:

```rust
pub fn close_server_media_session_for_user(
    &self,
    room_id: RoomId,
    user_id: UserId,
) -> Result<CloseServerMediaSessionResponse, MediaRelayError>
```

Required order:

1. Build `ServerMediaSessionKey`.
2. Stop `server_media_runtime_pump` for the key.
3. Close the negotiator for the key, returning the closed session status if available.
4. Remove the media relay participant via core registry.
5. Return response.

Do not stop `processed_audio_webrtc_egress_pump` for the room.

- [x] **Step 3: Implement REST route**

In `api_server_media.rs` add:

```text
POST /api/rooms/{room_id}/server-media/close
```

Request body:

```rust
struct CloseServerMediaSessionRequest {
    user_id: UserId,
}
```

Return JSON `CloseServerMediaSessionResponse`. Reuse `ApiError` conversion for `MediaRelayError`.

- [x] **Step 4: Verify backend targeted tests**

Run:

```bash
cargo test -p lyre-core media_tests::remove_participant
cargo test -p lyre-web api_server_media_tests::server_media_close
cargo test -p lyre-web api_webrtc_session_tests::close_server_media_session_for_user
cargo test -p lyre-web server_media_runtime_pump_tests::close_server_media_session_for_user
```

Expected: targeted tests pass.

## Task 3: WebRPC Contract and Frontend API Wrapper

**Files:**
- Modify: `proto/lyre.ridl`
- Regenerate: `frontend/src/lib/lyre.gen.ts`
- Modify: `frontend/src/lib/api.ts`
- Modify: `frontend/src/lib/api.test.ts`

- [x] **Step 1: Update RIDL**

Add:

```ridl
struct ServerMediaSessionStatus
  - roomID: string
  - userID: string
  - audioTrackID: string
  - state: ServerMediaSessionState

struct CloseServerMediaSessionResponse
  - mediaRelay: MediaRelayRoomStatus
  - session?: ServerMediaSessionStatus
```

Add service method:

```ridl
# Documents POST /api/rooms/{room_id}/server-media/close; REST fetch remains the runtime transport in this increment.
- CloseServerMediaSession(roomID: string, userID: string) => (closed: CloseServerMediaSessionResponse)
```

- [x] **Step 2: Regenerate TypeScript**

Run:

```bash
cd frontend
npm run generate:webrpc
```

Expected: `frontend/src/lib/lyre.gen.ts` contains generated close request/response types and query key.

- [x] **Step 3: Add frontend wrapper/tests**

In `frontend/src/lib/api.ts`, add local REST-shaped types for `ServerMediaSessionStatus` and `CloseServerMediaSessionResponse`, URL helper `serverMediaCloseUrl(roomId)`, and:

```ts
export async function closeServerMediaSession(
  roomId: string,
  userId: string
): Promise<CloseServerMediaSessionResponse>
```

Use `jsonOrThrow(response, "failed to close server media session")`.

Tests should verify URL construction, body serialization, generated type compatibility, and non-2xx error.

- [x] **Step 4: Verify frontend API tests**

Run:

```bash
cd frontend
npm test -- --run src/lib/api.test.ts
npm run typecheck
```

Expected: pass.

## Task 4: Room Client Cleanup Integration

**Files:**
- Modify: `frontend/src/app/room/[roomId]/room-client.tsx`
- Modify: `frontend/src/app/room/[roomId]/room-client.test.tsx`

- [x] **Step 1: Add room tests**

Add/adjust tests for:

- Server relay explicit Leave closes local media, calls `closeServerMediaSession(roomId, userId)`, then calls `leaveRoom`, and never calls `stopMediaRelay`.
- Server relay startup failure after `startMediaRelay` calls `closeServerMediaSession` and preserves the original startup error if cleanup succeeds.
- Server relay startup failure after `registerMediaTrack` calls `closeServerMediaSession` and preserves the original startup error if cleanup itself fails.
- Failure before `startMediaRelay` does not call `closeServerMediaSession`.
- Component unmount does not call `closeServerMediaSession`.
- Peer mesh mode does not call `closeServerMediaSession`.

- [x] **Step 2: Implement room cleanup state**

Update `RoomClient` to track whether server-side cleanup is needed for the current server relay attempt/session. Keep this state in a ref so cleanup works in async failure paths without forcing UI re-renders.

Rules:

- Set cleanup-needed after `startMediaRelay` succeeds.
- Leave cleanup-needed true after `registerMediaTrack` succeeds and after negotiation succeeds.
- On startup failure, if cleanup-needed is true, call `closeServerMediaSession`; ignore cleanup errors while preserving original status.
- On explicit Leave, if server relay was active/cleanup-needed, call `closeServerMediaSession` after local close and before `leaveRoom`.
- Reset cleanup-needed after successful close or when starting a fresh attempt.
- Do not call cleanup from unmount.
- Do not call cleanup in peer mesh mode.

- [x] **Step 3: Verify room tests**

Run:

```bash
cd frontend
npm test -- --run 'src/app/room/[roomId]/room-client.test.tsx'
```

Expected: pass.

## Task 5: Implementation Review, Docs, Final Verification, Commit

**Files:**
- Modify after implementation review approval: `MEMORY.md`
- Modify after implementation review approval: `docs/roadmap.md`

- [x] **Step 1: Run pre-review verification**

Run targeted backend and frontend commands from Tasks 2-4 plus:

```bash
cargo fmt --all --check
cd frontend && npm run typecheck
```

- [x] **Step 2: Independent implementation review**

Dispatch a fresh reviewer with the approved spec, plan, diff, and verification output. Required verdict:

```text
VERDICT: APPROVE
```

Fix and re-review until approved.

- [x] **Step 3: Update docs**

Update `MEMORY.md` with:

- Per-user server-media cleanup closes only one session/pump/participant.
- Room relay remains active and room-level stop remains separate.
- Frontend unmount remains local-only; explicit Leave and server relay startup failures perform cleanup.

Update `docs/roadmap.md`:

- Move per-user server-media cleanup to Completed.
- Keep DeepFilterNet, jitter/PLC, Rust WASM client-side noise cancellation, auth, persistence, observability, and generated WebRPC Rust server path in Next.

- [ ] **Step 4: Final verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path Cargo.toml --workspace
cd frontend
npm run generate:webrpc
npm test -- --run
npm run typecheck
npm run lint
npm run build
cd ..
git diff --check
```

Expected: all pass.

- [ ] **Step 5: Commit and push**

Stage only this increment's files. Leave unrelated untracked SDD artifacts untouched. Create a Lore-format commit and push current branch/upstream.
