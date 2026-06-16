# WebSocket Disconnect Room Leave Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove users from rooms when their authenticated room WebSocket disconnects, while preserving explicit Leave behavior and avoiding stale frontend room sessions.

**Architecture:** `AppState` owns the authoritative leave flow and will expose a WebSocket teardown helper that removes room membership, handles persistence/metrics, and gates `user-left` broadcast on actual removal. `PeerHub` gets a socket-only removal method so failed persistence and duplicate cleanup can remove dead senders without broadcasting false presence changes. `RoomClient` clears stale `sessionStorage` room sessions on socket close/unmount and calls REST Leave before closing the WebSocket on explicit Leave.

**Tech Stack:** Rust 2021, Axum WebSocket, DashMap, Tokio mpsc, Vitest, React Testing Library.

---

## File Structure

- Modify `crates/lyre-web/src/signalling.rs`: add `PeerHub::remove_peer`; keep `disconnect` as remove-plus-broadcast.
- Modify `crates/lyre-web/src/app_state.rs`: change `leave_room_persisted` to return `LeaveRoomResponse`; add `disconnect_room_socket`.
- Modify `crates/lyre-web/src/api.rs`: convert REST leave response body from `LeaveRoomResponse.room`; call `disconnect_room_socket` from `handle_socket`.
- Modify `crates/lyre-web/src/webrpc/handlers.rs`: convert WebRPC leave response body from `LeaveRoomResponse.room`; broadcast only when removed.
- Modify `crates/lyre-web/src/signalling_tests.rs`: test socket-only removal does not broadcast.
- Modify `crates/lyre-web/src/state_persistence_tests.rs`: add direct `AppState` disconnect cleanup tests for success, duplicate cleanup, persistence success, and persistence failure rollback.
- Modify `frontend/src/app/room/[roomId]/room-client.tsx`: clear stored room session on WebSocket close/unmount; reorder explicit Leave to call REST leave before closing WebSocket.
- Modify `frontend/src/app/room/[roomId]/room-client.test.tsx`: assert unmount/socket close clear session and explicit Leave calls REST leave before socket close.
- Modify `docs/roadmap.md`: add completed roadmap entry.

## Task 1: Server Disconnect Cleanup

**Files:**
- Modify: `crates/lyre-web/src/signalling.rs`
- Modify: `crates/lyre-web/src/app_state.rs`
- Modify: `crates/lyre-web/src/api.rs`
- Modify: `crates/lyre-web/src/webrpc/handlers.rs`
- Test: `crates/lyre-web/src/signalling_tests.rs`
- Test: `crates/lyre-web/src/state_persistence_tests.rs`

- [ ] **Step 1: Add the failing PeerHub socket-only removal test**

Add this test to `crates/lyre-web/src/signalling_tests.rs`:

```rust
#[test]
fn remove_peer_drops_socket_without_presence_broadcast() {
    let hub = PeerHub::new();
    let registry = RoomRegistry::new();
    let room_id = RoomId::default_room();
    let leaving_id = UserId::from_external("leaving");
    let peer_id = UserId::from_external("peer");
    let (leaving_tx, mut leaving_rx) = mpsc::unbounded_channel();
    let (peer_tx, mut peer_rx) = mpsc::unbounded_channel();
    hub.connect(&registry, room_id.clone(), leaving_id.clone(), leaving_tx);
    hub.connect(&registry, room_id.clone(), peer_id.clone(), peer_tx);

    hub.remove_peer(&room_id, &leaving_id);
    let delivered = hub.forward(SignalMessage::new(
        room_id,
        peer_id,
        Some(leaving_id),
        SignalPayload::Offer { sdp: "sdp".into() },
    ));

    assert_eq!(delivered.delivered, 0);
    assert!(leaving_rx.try_recv().is_err());
    assert!(peer_rx.try_recv().is_err());
}
```

- [ ] **Step 2: Run the focused failing PeerHub test**

Run:

```bash
cargo test -p lyre-web remove_peer_drops_socket_without_presence_broadcast
```

Expected: FAIL because `PeerHub::remove_peer` does not exist.

- [ ] **Step 3: Add `PeerHub::remove_peer`**

In `crates/lyre-web/src/signalling.rs`, add:

```rust
pub fn remove_peer(&self, room_id: &RoomId, user_id: &UserId) {
    if let Some(room) = self.peers.get(room_id) {
        room.remove(user_id);
    }
}
```

Change `disconnect` to:

```rust
pub fn disconnect(&self, room_id: &RoomId, user_id: &UserId) -> SignalDelivery {
    self.remove_peer(room_id, user_id);
    self.user_left(room_id, user_id)
}
```

- [ ] **Step 4: Run the PeerHub tests**

Run:

```bash
cargo test -p lyre-web signalling_tests
```

Expected: PASS.

- [ ] **Step 5: Add AppState disconnect cleanup tests**

Add tests to `crates/lyre-web/src/state_persistence_tests.rs`. Use existing `unique_state_path`, `RoomStatePersistence`, and `persisted_user` helpers. Cover:

```rust
#[tokio::test]
async fn websocket_disconnect_removes_room_user_and_broadcasts_user_left() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let leaving = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap()
        .user;
    let staying = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap()
        .user;
    let (leaving_tx, _leaving_rx) = tokio::sync::mpsc::unbounded_channel();
    let (staying_tx, mut staying_rx) = tokio::sync::mpsc::unbounded_channel();
    state
        .peers
        .connect(&state.registry, room_id.clone(), leaving.id.clone(), leaving_tx);
    state
        .peers
        .connect(&state.registry, room_id.clone(), staying.id.clone(), staying_tx);

    state.disconnect_room_socket(&room_id, &leaving.id).await;

    let snapshot = state.registry.snapshot(room_id.clone());
    assert_eq!(snapshot.users.len(), 1);
    assert_eq!(snapshot.users[0].id, staying.id);
    let signal = staying_rx.try_recv().unwrap();
    assert_eq!(signal.payload, crate::signalling::SignalPayload::UserLeft { user_id: leaving.id });
    let metrics = crate::metrics::snapshot(&state);
    assert_eq!(metrics.leaves, 1);
}
```

Also add:

```rust
#[tokio::test]
async fn websocket_disconnect_after_rest_leave_only_removes_socket() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let leaving = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap()
        .user;
    let staying = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap()
        .user;
    let (leaving_tx, _leaving_rx) = tokio::sync::mpsc::unbounded_channel();
    let (staying_tx, mut staying_rx) = tokio::sync::mpsc::unbounded_channel();
    state
        .peers
        .connect(&state.registry, room_id.clone(), leaving.id.clone(), leaving_tx);
    state
        .peers
        .connect(&state.registry, room_id.clone(), staying.id.clone(), staying_tx);

    let response = state.leave_room_persisted(&room_id, &leaving.id).await.unwrap();
    assert!(response.removed);
    state.disconnect_room_socket(&room_id, &leaving.id).await;

    assert!(staying_rx.try_recv().is_err());
    let metrics = crate::metrics::snapshot(&state);
    assert_eq!(metrics.leaves, 1);
}
```

Add persistence success:

```rust
#[tokio::test]
async fn websocket_disconnect_updates_persisted_room_state() {
    let path = unique_state_path("ws-disconnect");
    std::fs::write(
        &path,
        serde_json::to_vec(&PersistedRoomRegistry {
            rooms: vec![PersistedRoom {
                room_id: RoomId::default_room(),
                users: vec![persisted_user("user_a", "token_a")],
            }],
        })
        .unwrap(),
    )
    .unwrap();
    let state = AppState::with_room_state_persistence(
        Default::default(),
        None,
        Some(RoomStatePersistence::new(path.clone())),
        DeepFilterNetRuntimeConfig::default(),
    )
    .unwrap();

    state
        .disconnect_room_socket(&RoomId::default_room(), &UserId::from_external("user_a"))
        .await;

    let file = std::fs::read_to_string(&path).unwrap();
    assert!(!file.contains("user_a"));
    assert!(!file.contains("token_a"));
    let _ = std::fs::remove_file(path);
}
```

Add persistence failure:

```rust
#[tokio::test]
async fn websocket_disconnect_persistence_failure_rolls_back_without_broadcast() {
    let path = unique_state_path("ws-disconnect-rollback");
    std::fs::write(
        &path,
        serde_json::to_vec(&PersistedRoomRegistry {
            rooms: vec![PersistedRoom {
                room_id: RoomId::default_room(),
                users: vec![persisted_user("user_a", "token_a")],
            }],
        })
        .unwrap(),
    )
    .unwrap();
    let bad_path = unique_state_path("bad-ws-disconnect-rollback");
    let state = AppState::with_room_state_persistence(
        Default::default(),
        None,
        Some(RoomStatePersistence::new(path.clone())),
        DeepFilterNetRuntimeConfig::default(),
    )
    .unwrap();
    state
        .set_room_state_persistence_for_tests(Some(RoomStatePersistence::always_fail_for_tests(
            bad_path.clone(),
        )))
        .await;
    let room_id = RoomId::default_room();
    let (leaving_tx, _leaving_rx) = tokio::sync::mpsc::unbounded_channel();
    let (peer_tx, mut peer_rx) = tokio::sync::mpsc::unbounded_channel();
    state.peers.connect(
        &state.registry,
        room_id.clone(),
        UserId::from_external("user_a"),
        leaving_tx,
    );
    state.peers.connect(
        &state.registry,
        room_id.clone(),
        UserId::from_external("peer"),
        peer_tx,
    );

    state
        .disconnect_room_socket(&room_id, &UserId::from_external("user_a"))
        .await;

    assert!(state
        .registry
        .validate_access_token(
            &room_id,
            &UserId::from_external("user_a"),
            &RoomAccessToken::from_external("token_a"),
        )
        .is_ok());
    assert!(peer_rx.try_recv().is_err());
    let delivered = state.peers.forward(crate::signalling::SignalMessage::new(
        room_id,
        UserId::from_external("peer"),
        Some(UserId::from_external("user_a")),
        crate::signalling::SignalPayload::Offer { sdp: "sdp".into() },
    ));
    assert_eq!(delivered.delivered, 0);
    let metrics = crate::metrics::snapshot(&state);
    assert_eq!(metrics.leaves, 0);
    assert_eq!(metrics.persistence_failures, 1);
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(bad_path);
}
```

- [ ] **Step 6: Run focused failing AppState tests**

Run:

```bash
cargo test -p lyre-web websocket_disconnect
```

Expected: FAIL because `disconnect_room_socket` does not exist and `leave_room_persisted` still returns `RoomSnapshot`.

- [ ] **Step 7: Change AppState leave response and add disconnect cleanup**

In `crates/lyre-web/src/app_state.rs`, update `leave_room_persisted` to return `Result<lyre_core::LeaveRoomResponse, ApiError>` and return `response` instead of `response.room`.

Add:

```rust
pub async fn disconnect_room_socket(
    &self,
    room_id: &RoomId,
    user_id: &lyre_core::UserId,
) {
    self.peers.remove_peer(room_id, user_id);
    let leave = self.leave_room_persisted(room_id, user_id).await;
    match leave {
        Ok(response) if response.removed => {
            self.peers.user_left(room_id, user_id);
        }
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(
                error = format_args!("{error:#}"),
                "failed to leave room after websocket disconnect"
            );
        }
    }
}
```

- [ ] **Step 8: Update REST and WebRPC leave handlers**

In `crates/lyre-web/src/api.rs`, change REST leave to:

```rust
let response = state
    .leave_room_persisted(&room_id, &request.user_id)
    .await?;
if response.removed {
    state.peers.user_left(&room_id, &request.user_id);
}
Ok(Json(response.room))
```

In `handle_socket`, replace `state.peers.disconnect(&room_id, &user_id);` with:

```rust
state.disconnect_room_socket(&room_id, &user_id).await;
```

In `crates/lyre-web/src/webrpc/handlers.rs`, change WebRPC leave to:

```rust
let response = state.leave_room_persisted(&room_id, &user_id).await?;
if response.removed {
    state.peers.user_left(&room_id, &user_id);
}
Ok(Json(dto::LeaveRoomResponse {
    room: response.room.into(),
}))
```

- [ ] **Step 9: Run focused server tests**

Run:

```bash
cargo test -p lyre-web signalling_tests
cargo test -p lyre-web websocket_disconnect
cargo test -p lyre-web room_routes_join_snapshot_and_leave webrpc_join_get_and_leave_use_generated_client_shape
```

Expected: PASS.

## Task 2: Frontend Stale Session Cleanup

**Files:**
- Modify: `frontend/src/app/room/[roomId]/room-client.tsx`
- Test: `frontend/src/app/room/[roomId]/room-client.test.tsx`

- [ ] **Step 1: Add failing frontend tests**

Update the existing `keeps server relay unmount cleanup local without room mutations` test to seed `sessionStorage` and assert the stored session is removed:

```ts
sessionStorage.setItem(
  "lyre.roomSession",
  JSON.stringify({ roomId: "DEFAULT", accessToken: "token_a", user: makeUser("user_a") })
);
```

After `rendered.unmount();`, add:

```ts
expect(sessionStorage.getItem("lyre.roomSession")).toBeNull();
```

Add a new test:

```ts
it("clears stored room session when websocket closes", async () => {
  render(<RoomClient roomId="DEFAULT" />);
  await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());
  expect(sessionStorage.getItem("lyre.roomSession")).toContain("token_a");

  act(() => {
    sockets[0].onclose?.();
  });

  expect(sessionStorage.getItem("lyre.roomSession")).toBeNull();
  expect(screen.getByText("Disconnected")).toBeInTheDocument();
});
```

Update the existing leave cleanup test after clicking Leave:

```ts
await waitFor(() => expect(apiMocks.leaveRoom).toHaveBeenCalledWith("DEFAULT", "user_a", "token_a"));
expect(apiMocks.leaveRoom.mock.invocationCallOrder[0]).toBeLessThan(
  sockets[0].close.mock.invocationCallOrder[0]
);
expect(sessionStorage.getItem("lyre.roomSession")).toBeNull();
```

- [ ] **Step 2: Run focused failing frontend tests**

Run:

```bash
npm --prefix frontend test -- --run src/app/room/[roomId]/room-client.test.tsx
```

Expected: FAIL because session storage is not cleared on unmount/socket close and Leave currently closes the socket before REST leave.

- [ ] **Step 3: Implement frontend cleanup and Leave ordering**

In `frontend/src/app/room/[roomId]/room-client.tsx`, add:

```ts
function clearRoomSession() {
  sessionStorage.removeItem("lyre.roomSession");
}
```

Use it in `socket.onclose`:

```ts
socket.onclose = () => {
  clearRoomSession();
  setStatus("Disconnected");
};
```

Use it in the effect cleanup before closing the socket:

```ts
clearRoomSession();
socketRef.current?.close();
```

In `leave`, move WebSocket close after authenticated cleanup:

```ts
if (currentUser && accessToken) {
  if (shouldCloseServerMedia) {
    await closeServerMediaSession(roomId, currentUser.id, accessToken);
    serverMediaCleanupNeededRef.current = false;
  }
  await leaveRoom(roomId, currentUser.id, accessToken);
}
clearRoomSession();
socketRef.current?.close();
socketRef.current = null;
window.location.href = "/";
```

- [ ] **Step 4: Run focused frontend tests**

Run:

```bash
npm --prefix frontend test -- --run src/app/room/[roomId]/room-client.test.tsx
```

Expected: PASS.

## Task 3: Docs and Verification

**Files:**
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update roadmap**

Add this Completed bullet to `docs/roadmap.md`:

```markdown
- Authenticated room WebSocket disconnects now remove users from room membership, persist the leave, notify remaining peers, and clear stale frontend room sessions.
```

- [ ] **Step 2: Run formatting**

Run:

```bash
cargo fmt
```

Expected: exit 0.

- [ ] **Step 3: Run lint and focused tests**

Run:

```bash
cargo clippy --workspace --all-targets -- -D warnings
npm --prefix frontend test -- --run src/app/room/[roomId]/room-client.test.tsx
npm --prefix frontend typecheck
```

Expected: all exit 0.

- [ ] **Step 4: Run workspace tests**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: all tests pass. If `cargo nextest` is unavailable in the environment, run `cargo test --workspace` and report that substitution.

- [ ] **Step 5: Review diff**

Run:

```bash
git status --short
git diff -- docs/superpowers/specs/2026-06-16-websocket-disconnect-room-leave-design.md docs/superpowers/plans/2026-06-16-websocket-disconnect-room-leave.md crates/lyre-web/src/signalling.rs crates/lyre-web/src/app_state.rs crates/lyre-web/src/api.rs crates/lyre-web/src/webrpc/handlers.rs crates/lyre-web/src/signalling_tests.rs crates/lyre-web/src/state_persistence_tests.rs frontend/src/app/room/[roomId]/room-client.tsx frontend/src/app/room/[roomId]/room-client.test.tsx docs/roadmap.md
```

Expected: only intended SDD artifacts, server disconnect cleanup, frontend stale-session cleanup, tests, and roadmap changes are present.
