# Auto Audio Reconnect Mute Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make room audio start automatically after join, reconnect local server-media audio on ICE interruption, and replace the manual `Connect audio` control with local `Mute` / `Unmute`.

**Architecture:** Keep server relay lifecycle ownership in `RoomClient` and local WebRTC session ownership in `ServerMediaAudioSession`. Add minimal session callbacks/mute methods, then drive automatic startup and serialized reconnect from the room component without backend changes.

**Tech Stack:** Next.js, React, TypeScript, Vitest, Testing Library, browser WebRTC APIs.

---

## File Structure

- Modify `frontend/src/lib/server-media-audio.ts`: add ICE interruption callback and local track mute control.
- Modify `frontend/src/lib/server-media-audio.test.ts`: cover ICE callback and track enabled toggling.
- Modify `frontend/src/app/room/[roomId]/room-client.tsx`: auto-start audio after join/socket open, replace connect button with mute toggle, serialize reconnect.
- Modify `frontend/src/app/room/[roomId]/room-client-test-utils.ts`: expose mock local audio track and ICE state callback support.
- Modify `frontend/src/app/room/[roomId]/room-client.test.tsx`: update old manual-connect assumptions and add auto-start, mute, reconnect coverage.
- Modify `README.md`, `docs/getting-started.md`, and `docs/roadmap.md`: remove stale connect instruction and record the new behavior.

## Tasks

### Task 1: Server Media Audio Session Controls

**Files:**
- Modify: `frontend/src/lib/server-media-audio.ts`
- Modify: `frontend/src/lib/server-media-audio.test.ts`

- [ ] **Step 1: Add failing tests for local mute and ICE interruption**

In `frontend/src/lib/server-media-audio.test.ts`, update the mock peer connection to include:

```ts
iceConnectionState: RTCIceConnectionState = "new";
oniceconnectionstatechange: (() => void) | null = null;
```

Update the test `stream` so it returns a stable track with an `enabled` property:

```ts
const localAudioTrack = { id: "local-audio", enabled: true, stop: stopTrack };

const stream = {
  getAudioTracks: () => [localAudioTrack]
} as unknown as MediaStream;
```

Reset `localAudioTrack.enabled = true` in `beforeEach`.

Add these tests:

```ts
it("toggles local microphone tracks without closing the session", async () => {
  const session = makeSession();
  await session.start();

  session.setMuted(true);

  expect(localAudioTrack.enabled).toBe(false);
  expect(peerConnections[0].close).not.toHaveBeenCalled();
  expect(stopTrack).not.toHaveBeenCalled();

  session.setMuted(false);

  expect(localAudioTrack.enabled).toBe(true);
});

it("reports ICE disconnection and failure through the interruption callback", async () => {
  const onConnectionInterrupted = vi.fn();
  const session = new ServerMediaAudioSession({
    roomId: "DEFAULT",
    userId: "user_a",
    accessToken: "token_a",
    socket: { readyState: WebSocket.OPEN, send: socketSend } as unknown as WebSocket,
    iceServers: [{ urls: ["stun:stun.example:3478"], username: null, credential: null }],
    stream,
    pollIntervalMs: 10,
    onConnectionInterrupted
  });
  await session.start();

  peerConnections[0].iceConnectionState = "connected";
  peerConnections[0].oniceconnectionstatechange?.();
  peerConnections[0].iceConnectionState = "disconnected";
  peerConnections[0].oniceconnectionstatechange?.();
  peerConnections[0].iceConnectionState = "failed";
  peerConnections[0].oniceconnectionstatechange?.();

  expect(onConnectionInterrupted).toHaveBeenCalledTimes(2);
});
```

- [ ] **Step 2: Run the targeted session test and confirm failure**

Run:

```bash
cd frontend
npm test -- --run src/lib/server-media-audio.test.ts
```

Expected: fail because `setMuted` and `onConnectionInterrupted` do not exist yet.

- [ ] **Step 3: Implement session mute and ICE interruption**

In `frontend/src/lib/server-media-audio.ts`, extend `ServerMediaAudioSessionInput`:

```ts
  onConnectionInterrupted?: () => void;
```

In the constructor, after `this.peer.ontrack = ...`, add:

```ts
    this.peer.oniceconnectionstatechange = () => {
      if (this.peer.iceConnectionState === "disconnected" || this.peer.iceConnectionState === "failed") {
        this.input.onConnectionInterrupted?.();
      }
    };
```

Add a public method before `close()`:

```ts
  setMuted(muted: boolean): void {
    for (const track of this.input.stream.getAudioTracks()) {
      track.enabled = !muted;
    }
  }
```

- [ ] **Step 4: Run the targeted session test and confirm pass**

Run:

```bash
cd frontend
npm test -- --run src/lib/server-media-audio.test.ts
```

Expected: pass.

### Task 2: Room Auto Start, Mute Toggle, And ICE Reconnect

**Files:**
- Modify: `frontend/src/app/room/[roomId]/room-client.tsx`
- Modify: `frontend/src/app/room/[roomId]/room-client-test-utils.ts`
- Modify: `frontend/src/app/room/[roomId]/room-client.test.tsx`

- [ ] **Step 1: Add failing room client tests**

In `frontend/src/app/room/[roomId]/room-client-test-utils.ts`, add a stable exported local track:

```ts
const localAudioTrack = { id: "track", enabled: true, stop: stopTrack };
```

Update `getUserMedia.mockResolvedValue` in `beforeEach`:

```ts
  localAudioTrack.enabled = true;
  getUserMedia.mockResolvedValue({
    getAudioTracks: () => [localAudioTrack]
  });
```

Export `localAudioTrack`.

Update `MockPeerConnection` with:

```ts
  iceConnectionState: RTCIceConnectionState = "new";
  oniceconnectionstatechange: (() => void) | null = null;
```

In `frontend/src/app/room/[roomId]/room-client.test.tsx`:

- import `localAudioTrack`;
- replace click-based setup in tests that need connected audio with waiting for `apiMocks.answerServerMediaOffer`;
- replace `screen.getByText("Connect audio")` assertions with `screen.queryByText("Connect audio")` absence and `Mute` / `Unmute` assertions.

Add these tests:

```tsx
it("waits for the room websocket to open before starting automatic audio", async () => {
  render(<RoomClient roomId="DEFAULT" />);

  expect(apiMocks.startMediaRelay).not.toHaveBeenCalled();

  await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());
});

it("starts server relay audio automatically after joining", async () => {
  render(<RoomClient roomId="DEFAULT" />);

  await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

  expect(screen.queryByText("Connect audio")).not.toBeInTheDocument();
  expect(screen.getByText("Mute")).toBeInTheDocument();
  expect(navigator.mediaDevices.getUserMedia).toHaveBeenCalledOnce();
  expect(apiMocks.startMediaRelay).toHaveBeenCalledOnce();
  expect(apiMocks.registerMediaTrack).toHaveBeenCalledOnce();
});

it("toggles local microphone mute without recreating audio", async () => {
  render(<RoomClient roomId="DEFAULT" />);
  await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

  fireEvent.click(screen.getByText("Mute"));

  expect(localAudioTrack.enabled).toBe(false);
  expect(screen.getByText("Unmute")).toBeInTheDocument();
  expect(peerConnections).toHaveLength(1);

  fireEvent.click(screen.getByText("Unmute"));

  expect(localAudioTrack.enabled).toBe(true);
  expect(screen.getByText("Mute")).toBeInTheDocument();
  expect(apiMocks.startMediaRelay).toHaveBeenCalledOnce();
});

it("reconnects local audio when ICE is interrupted without restarting relay registration", async () => {
  render(<RoomClient roomId="DEFAULT" />);
  await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

  act(() => {
    peerConnections[0].iceConnectionState = "disconnected";
    peerConnections[0].oniceconnectionstatechange?.();
  });

  await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledTimes(2));

  expect(peerConnections).toHaveLength(2);
  expect(peerConnections[0].close).toHaveBeenCalledOnce();
  expect(apiMocks.startMediaRelay).toHaveBeenCalledOnce();
  expect(apiMocks.registerMediaTrack).toHaveBeenCalledOnce();
  expect(screen.getByText("Server relay audio connected")).toBeInTheDocument();
});

it("unblocks later reconnect attempts when one reconnect fails", async () => {
  render(<RoomClient roomId="DEFAULT" />);
  await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());
  apiMocks.getIceServers.mockRejectedValueOnce(new Error("temporary ice failure"));

  act(() => {
    peerConnections[0].iceConnectionState = "failed";
    peerConnections[0].oniceconnectionstatechange?.();
  });
  await waitFor(() => expect(screen.getByText("temporary ice failure")).toBeInTheDocument());

  act(() => {
    peerConnections[0].iceConnectionState = "disconnected";
    peerConnections[0].oniceconnectionstatechange?.();
  });

  await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledTimes(2));
  expect(apiMocks.startMediaRelay).toHaveBeenCalledOnce();
  expect(apiMocks.registerMediaTrack).toHaveBeenCalledOnce();
});
```

Keep and update the existing startup error tests so they run without clicking `Connect audio`:

```tsx
it("cleans server relay startup failures after relay start without stopping the room relay", async () => {
  apiMocks.registerMediaTrack.mockRejectedValueOnce(new Error("track registration failed"));
  render(<RoomClient roomId="DEFAULT" />);

  await waitFor(() => expect(screen.getByText("track registration failed")).toBeInTheDocument());

  expect(apiMocks.closeServerMediaSession).toHaveBeenCalledWith("DEFAULT", "user_a", "token_a");
  expect(apiMocks.stopMediaRelay).not.toHaveBeenCalled();
  expect(stopTrack).toHaveBeenCalledOnce();
});

it("keeps missing signalling websocket startup errors visible", async () => {
  render(<RoomClient roomId="DEFAULT" />);
  await waitFor(() => expect(screen.getByText("Connected")).toBeInTheDocument());
  sockets[0].readyState = WebSocket.CLOSED;

  await waitFor(() => expect(screen.getByText("Audio signalling websocket is not connected")).toBeInTheDocument());

  expect(apiMocks.stopMediaRelay).not.toHaveBeenCalled();
  expect(apiMocks.closeServerMediaSession).toHaveBeenCalledWith("DEFAULT", "user_a", "token_a");
  expect(stopTrack).toHaveBeenCalledOnce();
  expect(peerConnections).toHaveLength(0);
});

it("does not start media when ice server fetch fails", async () => {
  apiMocks.getIceServers.mockRejectedValueOnce(new Error("ice unavailable"));
  render(<RoomClient roomId="DEFAULT" />);

  await waitFor(() => expect(screen.getByText("ice unavailable")).toBeInTheDocument());

  expect(navigator.mediaDevices.getUserMedia).not.toHaveBeenCalled();
  expect(peerConnections).toHaveLength(0);
  expect(apiMocks.startMediaRelay).not.toHaveBeenCalled();
});
```

- [ ] **Step 2: Run the targeted room tests and confirm failure**

Run:

```bash
cd frontend
npm test -- --run src/app/room/[roomId]/room-client.test.tsx
```

Expected: fail because audio is still manual, mute is not implemented, and ICE reconnect is not wired.

- [ ] **Step 3: Implement room client state and callbacks**

In `frontend/src/app/room/[roomId]/room-client.tsx`:

- replace `const [audioStarted, setAudioStarted] = useState(false);` with:

```ts
  const [audioStarted, setAudioStarted] = useState(false);
  const [muted, setMuted] = useState(false);
  const [socketOpen, setSocketOpen] = useState(false);
```

- add refs:

```ts
  const relayStartedRef = useRef(false);
  const reconnectingAudioRef = useRef(false);
  const reconnectServerRelayAudioRef = useRef<() => void>(() => undefined);
```

- in the WebSocket `onopen`, set `socketOpen` to `true` after setting the connected status.
- in WebSocket `onclose`, set `socketOpen` to `false`.
- in cleanup, set `relayStartedRef.current = false`, `reconnectingAudioRef.current = false`, and `setSocketOpen(false)`.
- remove the manual `connectAudio` function.
- make `connectServerRelayAudio` a `useCallback` that depends on `accessToken`, `currentUser`, `muted`, and `roomId`.
- change first-start/reconnect behavior inside `connectServerRelayAudio`:

```ts
      const shouldStartRelay = !updateRelay && !relayStartedRef.current;
      if (shouldStartRelay) {
        await startMediaRelay(roomId, noise, accessToken);
        cleanupNeeded = true;
        serverMediaCleanupNeededRef.current = true;
        await registerMediaTrack(roomId, currentUser.id, "audio-main", "audio", accessToken);
        relayStartedRef.current = true;
      }
```

- in the catch block, only clear `audioStartedRef` when `!updateRelay`; reconnect failures should close the failed local session and surface the error, but keep `audioStartedRef.current` and `relayStartedRef.current` true so a later reconnect attempt uses `updateRelay: true` and does not repeat `startMediaRelay` / `registerMediaTrack`.
- pass an interruption callback without creating a hook dependency cycle:

```ts
        onConnectionInterrupted: () => reconnectServerRelayAudioRef.current()
```

- after creating `session`, call:

```ts
      session.setMuted(muted);
```

- add this `useEffect` after `connectServerRelayAudio`:

```ts
  useEffect(() => {
    reconnectServerRelayAudioRef.current = () => {
      if (reconnectingAudioRef.current || !audioStartedRef.current) {
        return;
      }
      reconnectingAudioRef.current = true;
      setStatus("Reconnecting audio");
      closeAudioSessions();
      void connectServerRelayAudio({ updateRelay: true }).finally(() => {
        reconnectingAudioRef.current = false;
      });
    };
  }, [closeAudioSessions, connectServerRelayAudio]);
```

- add a `useEffect` after `connectServerRelayAudio`:

```ts
  useEffect(() => {
    if (!currentUser || !accessToken || !socketOpen || audioStartedRef.current) {
      return;
    }
    void connectServerRelayAudio({ updateRelay: false });
  }, [accessToken, connectServerRelayAudio, currentUser, socketOpen]);
```

- add a toggle function:

```ts
  function toggleMuted() {
    const nextMuted = !muted;
    setMuted(nextMuted);
    serverAudioSessionRef.current?.setMuted(nextMuted);
  }
```

- replace the toolbar audio button with:

```tsx
          <Button disabled={!audioStarted} onClick={toggleMuted}>{muted ? "Unmute" : "Mute"}</Button>
```

- update `saveSettings` to preserve mute on the recreated session through the existing `muted` state.
- ensure the reconnect path uses `.finally` so a failed reconnect does not permanently block later ICE reconnect attempts.
- keep the initial startup `catch` behavior: if ICE loading fails before `startMediaRelay`, do not call cleanup; if startup fails after relay start, call `closeServerMediaSession`; if the WebSocket is closed after local media opens, surface `Audio signalling websocket is not connected`.
- for `updateRelay: true` reconnect failures, do not call `closeServerMediaSession`, do not clear `relayStartedRef`, and do not clear `audioStartedRef`; the room remains in the joined relay lifecycle and a later ICE interruption callback can retry local session negotiation without repeating relay start or track registration.

- [ ] **Step 4: Run the targeted room tests and confirm pass**

Run:

```bash
cd frontend
npm test -- --run src/app/room/[roomId]/room-client.test.tsx
```

Expected: pass.

### Task 3: Documentation Updates

**Files:**
- Modify: `README.md`
- Modify: `docs/getting-started.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update README quick start**

Replace:

```md
Open `http://localhost:3000`, create or join a room, then use `Connect audio` to start microphone capture.
```

with:

```md
Open `http://localhost:3000` and create or join a room. The room connects server-relay audio automatically after joining; use `Mute` / `Unmute` to control only your local microphone.
```

- [ ] **Step 2: Update getting started frontend note**

In `docs/getting-started.md`, replace:

```md
The settings page stores nickname, preferred room, noise-cancellation settings, and browser audio-processing controls through the Zustand settings store. The room page keeps microphone access behind the `Connect audio` button.
```

with:

```md
The settings page stores nickname, preferred room, noise-cancellation settings, and browser audio-processing controls through the Zustand settings store. After joining a room, the room page starts server-relay audio automatically and exposes `Mute` / `Unmute` for the local microphone only.
```

- [ ] **Step 3: Update roadmap**

In `docs/roadmap.md`, add a completed bullet near the frontend server-media bullets:

```md
- Frontend room audio now starts server-relay audio automatically after join, reconnects local media on ICE interruption, and exposes local microphone `Mute` / `Unmute` instead of manual `Connect audio`.
```

- [ ] **Step 4: Check docs for stale Connect audio references in active docs**

Run:

```bash
rg -n "Connect audio" README.md docs/getting-started.md docs/roadmap.md frontend/src
```

Expected: no stale active UI references remain in those files.

### Task 4: Full Verification And Review Inputs

**Files:**
- No code edits unless verification finds a defect.

- [ ] **Step 1: Run targeted frontend tests**

Run:

```bash
cd frontend
npm test -- --run src/lib/server-media-audio.test.ts src/app/room/[roomId]/room-client.test.tsx
```

Expected: pass.

- [ ] **Step 2: Run frontend lint and typecheck**

Run:

```bash
cd frontend
npm run lint
npm run typecheck
```

Expected: both pass.

- [ ] **Step 3: Run repository-required formatting, clippy, and tests**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: pass.

- [ ] **Step 4: Review final diff**

Run:

```bash
git status --short
git diff -- frontend/src/lib/server-media-audio.ts frontend/src/lib/server-media-audio.test.ts frontend/src/app/room/[roomId]/room-client.tsx frontend/src/app/room/[roomId]/room-client-test-utils.ts frontend/src/app/room/[roomId]/room-client.test.tsx README.md docs/getting-started.md docs/roadmap.md docs/superpowers/specs/2026-06-16-auto-audio-reconnect-mute-design.md docs/superpowers/plans/2026-06-16-auto-audio-reconnect-mute.md
```

Expected: only intended files changed.
