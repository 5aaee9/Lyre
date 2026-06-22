# API Restart Room Rejoin Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Recover room clients from stale browser-stored room sessions after the API restarts and rejects the old access token.

**Architecture:** Keep recovery in `RoomClient` because the stale session lives in browser `sessionStorage` and the backend intentionally does not accept old in-memory tokens. Add a single recovery path that clears stale session state, rejoins, stores the fresh session, and reconnects the signalling socket/audio through the existing automatic audio startup flow.

**Tech Stack:** Next.js React client component, Vitest, Testing Library, existing mocked WebSocket/API test utilities.

---

## File Structure

- Modify `frontend/src/app/room/[roomId]/room-client.tsx`: add stale-session recovery helpers and route websocket/authenticated API 401 failures through them.
- Modify `frontend/src/app/room/[roomId]/room-client-test-utils.ts`: expose the mocked `joinRoom` and make mock WebSocket open timing controllable enough for close-before-open tests.
- Modify `frontend/src/app/room/[roomId]/room-client.test.tsx`: add regression tests for stale stored-session websocket auth failure and stale token media startup failure.
- Modify `docs/roadmap.md`: add the completed recovery behavior after final implementation approval.

## Task 1: Test Utilities For Session Recovery

**Files:**
- Modify: `frontend/src/app/room/[roomId]/room-client-test-utils.ts`

- [ ] **Step 1: Expose `joinRoom` in `apiMocks`**

Change the `apiMocks` hoisted object to include:

```ts
joinRoom: vi.fn(),
```

Change the `vi.mock("@/lib/api", ...)` export for `joinRoom` from an inline `vi.fn(async () => ...)` to:

```ts
joinRoom: apiMocks.joinRoom,
```

In `beforeEach`, reset it with the existing default response:

```ts
apiMocks.joinRoom.mockReset();
apiMocks.joinRoom.mockResolvedValue({
  access_token: "token_a",
  user: users[0],
  room: { room_id: "DEFAULT", users }
});
```

- [ ] **Step 2: Allow tests to close a socket before automatic open**

Add an exported boolean setter around the mock socket auto-open behavior:

```ts
let autoOpenSockets = true;

function setAutoOpenSockets(enabled: boolean): void {
  autoOpenSockets = enabled;
}
```

Update `MockWebSocket` constructor:

```ts
constructor() {
  sockets.push(this);
  setTimeout(() => {
    if (autoOpenSockets) {
      this.onopen?.();
    }
  }, 0);
}
```

Export `setAutoOpenSockets`.

Reset it in `beforeEach`:

```ts
autoOpenSockets = true;
```

- [ ] **Step 3: Run targeted tests to verify no behavior changed yet**

Run:

```bash
cd frontend && pnpm vitest run 'src/app/room/[roomId]/room-client.test.tsx'
```

Expected: the current RoomClient suite still passes.

## Task 2: Regression Tests For API Restart Recovery

**Files:**
- Modify: `frontend/src/app/room/[roomId]/room-client.test.tsx`

- [ ] **Step 1: Import new test utilities**

Add imports from `room-client-test-utils`:

```ts
setAutoOpenSockets,
```

The existing import already includes `apiMocks`, `makeUser`, `sockets`, and other utilities.

- [ ] **Step 2: Add websocket stale-session regression test**

Add this test near the existing websocket reconnect tests:

```ts
it("rejoins when a stored room session websocket closes before opening", async () => {
  const staleUser = makeUser("stale_user", "Stale");
  sessionStorage.setItem(
    "lyre.roomSession",
    JSON.stringify({ roomId: "DEFAULT", accessToken: "stale_token", user: staleUser })
  );
  setAutoOpenSockets(false);

  render(<RoomClient roomId="DEFAULT" />);
  await waitFor(() => expect(sockets).toHaveLength(1));

  act(() => {
    sockets[0].onclose?.();
  });
  setAutoOpenSockets(true);

  await waitFor(() => expect(apiMocks.joinRoom).toHaveBeenCalledOnce());
  await waitFor(() => expect(sockets).toHaveLength(2), { timeout: 2_000 });
  await waitFor(() => expect(screen.getByText("Server relay audio connected")).toBeInTheDocument());
  expect(sessionStorage.getItem("lyre.roomSession")).toContain("token_a");
  expect(apiMocks.startMediaRelay).toHaveBeenCalledWith("DEFAULT", defaultNoiseConfig, "token_a");
});
```

- [ ] **Step 3: Strengthen normal close-after-open reconnect coverage**

In the existing test named `reconnects the room websocket with the stored session when it closes`, add:

```ts
apiMocks.joinRoom.mockClear();
...
expect(apiMocks.joinRoom).not.toHaveBeenCalled();
```

Place `apiMocks.joinRoom.mockClear()` after the initial connected state and stored-token assertion, before triggering `sockets[0].onclose?.()`. Place `expect(apiMocks.joinRoom).not.toHaveBeenCalled()` after the second socket has been created and before the final stored-token assertion. This proves a normal socket close after a successful open still reuses the stored session and does not rejoin.

- [ ] **Step 4: Add authenticated media stale-token regression test**

Add this test near startup failure tests:

```ts
it("rejoins and retries audio startup when a stored access token is unauthorized", async () => {
  const staleUser = makeUser("stale_user", "Stale");
  sessionStorage.setItem(
    "lyre.roomSession",
    JSON.stringify({ roomId: "DEFAULT", accessToken: "stale_token", user: staleUser })
  );
  apiMocks.startMediaRelay.mockRejectedValueOnce(new Error("failed to start media relay: 401"));

  render(<RoomClient roomId="DEFAULT" />);

  await waitFor(() => expect(apiMocks.joinRoom).toHaveBeenCalledOnce());
  await waitFor(() => expect(screen.getByText("Server relay audio connected")).toBeInTheDocument());
  expect(sessionStorage.getItem("lyre.roomSession")).toContain("token_a");
  expect(apiMocks.startMediaRelay).toHaveBeenNthCalledWith(1, "DEFAULT", defaultNoiseConfig, "stale_token");
  expect(apiMocks.startMediaRelay).toHaveBeenNthCalledWith(2, "DEFAULT", defaultNoiseConfig, "token_a");
  expect(apiMocks.registerMediaTrack).toHaveBeenCalledWith("DEFAULT", "user_a", "audio-main", "audio", "token_a");
});
```

- [ ] **Step 5: Run tests to confirm red state**

Run:

```bash
cd frontend && pnpm vitest run 'src/app/room/[roomId]/room-client.test.tsx' -t 'rejoins'
```

Expected before implementation: at least the new stale-session tests fail.

## Task 3: Implement Stale Session Recovery

**Files:**
- Modify: `frontend/src/app/room/[roomId]/room-client.tsx`

- [ ] **Step 1: Add helpers for unauthorized detection and fresh join**

Add near constants:

```ts
function isUnauthorizedError(error: unknown): boolean {
  return error instanceof Error && error.message.endsWith(": 401");
}
```

Inside `RoomClient`, add refs:

```ts
const mountedRef = useRef(false);
const roomSessionRef = useRef<RoomSession | null>(null);
const sessionRecoveryRef = useRef<Promise<void> | null>(null);
const recoverExpiredSessionRef = useRef<() => Promise<void>>(() => Promise.resolve());
```

Add a mount guard effect:

```ts
useEffect(() => {
  mountedRef.current = true;
  return () => {
    mountedRef.current = false;
  };
}, []);
```

Add a helper inside the room-entry effect:

```ts
async function joinFreshRoom(): Promise<{ session: RoomSession; room: RoomSnapshot }> {
  const response = await joinRoom(roomId, { nickname: readNickname(), noise: readNoiseConfig() });
  const session = { roomId, user: response.user, accessToken: response.access_token };
  return { session, room: response.room };
}
```

Add a separate apply helper so state and storage writes can be guarded after awaited joins:

```ts
function applyRoomSession(session: RoomSession, roomSnapshot?: RoomSnapshot) {
  roomSessionRef.current = session;
  if (roomSnapshot) {
    setRoom(roomSnapshot);
  }
  sessionStorage.setItem("lyre.roomSession", JSON.stringify(session));
}
```

- [ ] **Step 2: Make socket reconnect use the latest session**

Refactor `connectSocket(session)` to accept a stale-session candidate flag:

```ts
function connectSocket(session: RoomSession, recoverOnPreOpenClose: boolean) {
```

The flag should be `true` for sessions read from `sessionStorage` and for reconnect attempts after a previously opened socket closes. It should be `false` only for the first socket created immediately after a fresh `joinRoom` response. This avoids rejoining on every transient close-before-open while still recovering from API restarts that invalidate a stored or already-established room session.

Inside `connectSocket`, write `roomSessionRef.current = session` before creating the socket.

Set `reconnectRoomSocketRef.current` to read `roomSessionRef.current` at timeout execution:

```ts
const nextSession = roomSessionRef.current;
if (!cancelled && nextSession) {
  connectSocket(nextSession, true);
}
```

Initial `enterRoom()` should:

```ts
const storedSession = readRoomSession(roomId);
if (storedSession) {
  applyRoomSession(storedSession);
  ...
  connectSocket(storedSession, true);
  return;
}
const { session, room } = await joinFreshRoom();
if (cancelled) {
  return;
}
applyRoomSession(session, room);
...
connectSocket(session, false);
```

Use the existing `setCurrentUser` and `setAccessToken` calls for both branches after the cancellation check.

- [ ] **Step 3: Add recovery path for stale sessions**

Inside the room-entry effect, add:

```ts
function recoverExpiredSession(): Promise<void> {
  if (sessionRecoveryRef.current) {
    return sessionRecoveryRef.current;
  }
  setStatus("Reconnecting");
  sessionRecoveryRef.current = (async () => {
    clearSocketReconnectRetry();
    closeAudioSessions();
    socketRef.current?.close();
    socketRef.current = null;
    setSocketOpen(false);
    audioStartedRef.current = false;
    listenOnlyRef.current = false;
    relayStartedRef.current = false;
    serverMediaCleanupNeededRef.current = false;
    lastSubscribedSourceIdsRef.current = [];
    setAudioStarted(false);
    setListenOnly(false);
    clearRoomSession();
    const { session, room } = await joinFreshRoom();
    if (!cancelled) {
      applyRoomSession(session, room);
      setCurrentUser(session.user);
      setAccessToken(session.accessToken);
      connectSocket(session, false);
    }
  })().finally(() => {
    sessionRecoveryRef.current = null;
  });
  return sessionRecoveryRef.current;
}
```

Assign it to the ref after declaration:

```ts
recoverExpiredSessionRef.current = recoverExpiredSession;
```

Use this function when a stale-session candidate socket closes before opening. Track a local `opened` boolean in `connectSocket`:

```ts
let opened = false;
socket.onopen = () => {
  ...
  opened = true;
  ...
};
socket.onclose = () => {
  ...
  if (!opened && recoverOnPreOpenClose) {
    recoverExpiredSession();
    return;
  }
  ...
};
```

- [ ] **Step 4: Route authenticated 401 startup failures through recovery**

In `connectServerRelayAudio` catch block, before showing the error as terminal, add:

```ts
if (isUnauthorizedError(error)) {
  if (!mountedRef.current) {
    return;
  }
  closeAudioSessions();
  audioStartedRef.current = false;
  listenOnlyRef.current = false;
  relayStartedRef.current = false;
  serverMediaCleanupNeededRef.current = false;
  lastSubscribedSourceIdsRef.current = [];
  setAudioStarted(false);
  setListenOnly(false);
  void recoverExpiredSessionRef.current();
  return;
}
```

This calls the same stale-session recovery used by close-before-open websocket failures, so authenticated `: 401` errors clear `lyre.roomSession`, join again, store the fresh token, and reconnect directly instead of retrying the stale socket.

- [ ] **Step 5: Cleanup on unmount**

In effect cleanup, clear:

```ts
roomSessionRef.current = null;
sessionRecoveryRef.current = null;
recoverExpiredSessionRef.current = () => Promise.resolve();
```

Keep the existing `clearRoomSession()` behavior on unmount and explicit leave unchanged.

## Task 4: Verify Frontend Implementation

**Files:**
- Modify as needed only in files from Tasks 1-3.

- [ ] **Step 1: Run targeted stale-session tests**

Run:

```bash
cd frontend && pnpm vitest run 'src/app/room/[roomId]/room-client.test.tsx' -t 'rejoins'
```

Expected: all matching tests pass, including the new stale-session tests and existing rejoin tests.

- [ ] **Step 2: Run full RoomClient tests**

Run:

```bash
cd frontend && pnpm vitest run 'src/app/room/[roomId]/room-client.test.tsx'
```

Expected: all RoomClient tests pass.

- [ ] **Step 3: Run frontend typecheck and lint**

Run:

```bash
cd frontend && pnpm typecheck
cd frontend && pnpm lint
```

Expected: both commands exit 0.

- [ ] **Step 4: Run Rust formatting, lint, and tests because repo instructions require them after edits**

Run:

```bash
cargo fmt --check
cargo clippy --workspace
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: all commands exit 0.

## Task 5: Update Roadmap

**Files:**
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Add completed roadmap entry**

Append this bullet to the `## Completed` section:

```md
- Frontend room clients now recover from API restarts that invalidate browser-stored room sessions by clearing stale credentials, rejoining, and reconnecting signalling plus server-relay audio.
```

- [ ] **Step 2: Review docs diff**

Run:

```bash
git diff -- docs/roadmap.md
```

Expected: only the new completed roadmap bullet is present.

## Task 6: Final Repository Verification

**Files:**
- Verify all changed files.

- [ ] **Step 1: Run full frontend checks**

Run:

```bash
cd frontend && pnpm vitest run 'src/app/room/[roomId]/room-client.test.tsx'
cd frontend && pnpm typecheck
cd frontend && pnpm lint
```

Expected: all commands exit 0.

- [ ] **Step 2: Run Rust checks required by repo instructions**

Run:

```bash
cargo fmt --check
cargo clippy --workspace
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: all commands exit 0.

- [ ] **Step 3: Review final diff**

Run:

```bash
git status --short
git diff --check
git diff -- frontend/src/app/room/[roomId]/room-client.tsx frontend/src/app/room/[roomId]/room-client-test-utils.ts frontend/src/app/room/[roomId]/room-client.test.tsx docs/roadmap.md docs/superpowers/specs/2026-06-23-api-restart-room-rejoin-design.md docs/superpowers/plans/2026-06-23-api-restart-room-rejoin.md
```

Expected: no whitespace errors; diff is limited to the approved spec, plan, RoomClient recovery, tests/utilities, and roadmap entry.

## Plan Self-Review

- Spec coverage: Task 2 covers the two required stale-session recovery tests and existing normal reconnect/cleanup tests remain in the suite; Task 3 implements fresh join, latest-session reconnect, direct 401 recovery, and in-flight recovery promise reuse; Tasks 4 and 6 verify frontend and repo-required checks; Task 5 covers the required roadmap update.
- Placeholder scan: no TODO/TBD placeholders remain.
- Type consistency: `RoomSession`, `UserProfile`, `apiMocks`, and existing API helper names match current files.
