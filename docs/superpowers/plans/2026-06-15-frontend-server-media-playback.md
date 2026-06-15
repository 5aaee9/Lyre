# Frontend Server Media Playback Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `/room/[roomId]` default to server relay audio, negotiate browser-to-server WebRTC through existing REST APIs, and play remote processed server audio in the browser.

**Architecture:** Add a focused frontend `ServerMediaAudioSession` that owns one WebRTC peer, local microphone stream, remote playback stream, and server ICE polling. Update the room client to choose between the new server relay session and the existing mesh session, with server relay as default and mode selection disabled after audio starts.

**Tech Stack:** Next.js, React, TypeScript, Vitest, browser WebRTC APIs, existing REST API wrappers.

**Reviewed Spec:** `docs/superpowers/specs/2026-06-15-frontend-server-media-playback-design.md`

---

## File Structure

- Create `frontend/src/lib/server-media-audio.ts`: server relay WebRTC session controller.
- Create `frontend/src/lib/server-media-audio.test.ts`: unit tests for negotiation, candidate exchange, playback attachment, dedupe, and cleanup.
- Modify `frontend/src/app/room/[roomId]/room-client.tsx`: add audio mode state, default server relay startup, mesh compatibility mode, and cleanup.
- Modify `frontend/src/app/room/[roomId]/room-client.test.tsx`: cover room-level server relay behavior plus retained mesh behavior.
- Modify `MEMORY.md` and `docs/roadmap.md` only after independent implementation review approves.

Do not add backend endpoints. Do not call `stopMediaRelay` from the room page in this increment because it is room-level.

## Task 0: SDD Pre-Implementation Gate

**Files:**
- Read: `docs/superpowers/specs/2026-06-15-frontend-server-media-playback-design.md`
- Read: `docs/superpowers/plans/2026-06-15-frontend-server-media-playback.md`

- [ ] **Step 1: Confirm approved spec review exists**

Required evidence before implementation:

```text
VERDICT: APPROVE
ISSUES:
- None
REQUIRED_CHANGES:
- None
```

- [ ] **Step 2: Dispatch independent plan reviewer**

Dispatch an independent reviewer with the approved spec and this plan. Required verdict format:

```text
VERDICT: APPROVE | REVISE
ISSUES:
- [blocking issue or "None"]
REQUIRED_CHANGES:
- [change or "None"]
```

- [ ] **Step 3: Stop before code edits unless plan is approved**

Implementation may begin only after the independent plan reviewer returns:

```text
VERDICT: APPROVE
```

If the reviewer returns `REVISE`, update this plan and re-review until approved.

## Task 1: Add Server Media Audio Session

**Files:**
- Create: `frontend/src/lib/server-media-audio.ts`
- Create: `frontend/src/lib/server-media-audio.test.ts`

- [ ] **Step 1: Add session tests**

Create tests that mock `RTCPeerConnection`, `MediaStream`, `document.createElement("audio")`, and API functions. Cover:

- `start()` creates an offer, calls `peer.setLocalDescription(offer)`, calls `answerServerMediaOffer(roomId, userId, audioTrackId, offer.sdp)`, sets the returned answer as remote description, fetches server candidates once, and starts polling.
- Local ICE candidates call `addServerMediaIceCandidate` with `user_id`, `candidate`, `sdp_mid`, `sdp_mline_index`, and `username_fragment`.
- Incoming `ontrack` adds remote tracks to a playback stream and calls `audio.play()`.
- Repeated server candidates are added only once.
- `close()` closes the peer, stops local audio tracks, stops polling, clears `audio.srcObject`, and removes the audio element.

- [ ] **Step 2: Implement the session**

Implement:

```ts
type ServerMediaAudioSessionInput = {
  roomId: string;
  userId: string;
  audioTrackId?: string;
  iceServers: IceServerConfig[];
  stream: MediaStream;
  pollIntervalMs?: number;
  onError?: (message: string) => void;
};

export class ServerMediaAudioSession {
  constructor(input: ServerMediaAudioSessionInput);
  async start(): Promise<void>;
  close(): void;
}
```

Use `audio-main` as the default audio track id. Deduplicate server candidates by candidate string plus optional `sdp_mid`, `sdp_mline_index`, and `username_fragment`. Use `window.setInterval` / `window.clearInterval` so tests can use fake timers.

- [ ] **Step 3: Run targeted tests**

Run:

```bash
cd frontend
npm test -- --run src/lib/server-media-audio.test.ts
```

Expected: pass.

## Task 2: Integrate Server Relay Mode into Room Client

**Files:**
- Modify: `frontend/src/app/room/[roomId]/room-client.tsx`
- Modify: `frontend/src/app/room/[roomId]/room-client.test.tsx`

- [ ] **Step 1: Update room tests first**

Adjust mocks for API functions and WebRTC globals. Add/retain tests for:

- Presence WebSocket opens without microphone permission.
- Default `Connect audio` path uses server relay: `getIceServers` before `getUserMedia`, `startMediaRelay(roomId, readNoiseConfig())`, `registerMediaTrack(roomId, user.id, "audio-main", "audio")`, `answerServerMediaOffer`, and no mesh signalling offers.
- The audio mode select defaults to `Server relay` and is disabled after audio starts.
- Selecting `Peer mesh` before connecting preserves existing targeted mesh offer behavior.
- Server relay startup failure closes the session or stops local tracks and does not call `stopMediaRelay`.
- Explicit Leave after server relay startup closes local media and calls `leaveRoom`, but does not call `stopMediaRelay`.
- Component unmount after server relay startup performs local cleanup only: it closes local media and WebSocket but does not call `leaveRoom` or `stopMediaRelay`.
- Peer mesh mode never calls `stopMediaRelay`, including connect, failure, leave, and unmount paths.
- Server relay failures keep the underlying error message visible in room status for ICE loading, microphone capture, relay start, track registration, offer negotiation, candidate exchange, and playback setup.

- [ ] **Step 2: Implement room integration**

Update `RoomClient` to:

- import `Select`, `ServerMediaAudioSession`, `startMediaRelay`, and `registerMediaTrack`; runtime code must not import or call `stopMediaRelay`.
- add `audioMode` state with values `"server_relay"` and `"peer_mesh"`, defaulting to `"server_relay"`.
- keep separate refs for `MeshAudioSession` and `ServerMediaAudioSession`.
- route signalling WebRTC messages only to mesh when audio has started in peer mesh mode.
- implement `connectServerRelayAudio()` using the spec startup order.
- implement failure handling so every thrown `Error` message from ICE loading, microphone capture, relay start, track registration, offer negotiation, candidate exchange, or playback setup is copied into `status`; non-`Error` failures use `Audio connection failed`.
- keep existing mesh startup logic in `connectMeshAudio()`.
- make `connectAudio()` dispatch based on the selected mode.
- close both possible session refs on leave/unmount.
- keep unmount local-only: close sessions and WebSocket, but do not call `leaveRoom` or `stopMediaRelay`.
- keep explicit Leave as the only server mutation cleanup path and limit it to `leaveRoom`; do not call `stopMediaRelay`.
- disable the mode select and connect button after audio starts.

- [ ] **Step 3: Run room tests**

Run:

```bash
cd frontend
npm test -- --run 'src/app/room/[roomId]/room-client.test.tsx'
```

Expected: pass.

## Task 3: Final Frontend Verification Before Implementation Review

**Files:**
- Frontend files changed in Tasks 1 and 2.

- [ ] **Step 1: Run frontend test suite**

Run:

```bash
cd frontend
npm test -- --run
```

Expected: pass.

- [ ] **Step 2: Run frontend typecheck, lint, and build**

Run:

```bash
cd frontend
npm run typecheck
npm run lint
npm run build
```

Expected: all pass.

- [ ] **Step 3: Check changed frontend file sizes**

Run:

```bash
wc -l frontend/src/lib/server-media-audio.ts frontend/src/app/room/[roomId]/room-client.tsx
```

Expected: each file remains under 400 LOC.

## Task 4: Post-Review Documentation and Workspace Verification

**Files:**
- Modify after independent implementation review approval: `MEMORY.md`
- Modify after independent implementation review approval: `docs/roadmap.md`

- [ ] **Step 1: Wait for implementation review approval**

Before editing docs, dispatch an independent implementation reviewer with the spec, plan, diff, and verification output. Required verdict:

```text
VERDICT: APPROVE
```

- [ ] **Step 2: Update `MEMORY.md`**

Add a `2026-06-15 Frontend Server Media Playback` entry recording:

- server relay is now the default room audio path,
- peer mesh remains explicit compatibility mode,
- playback is remote-participant audio only,
- Leave/unmount perform local media cleanup but do not call the room-level `stopMediaRelay`.

- [ ] **Step 3: Update `docs/roadmap.md`**

Move `Switch frontend media flow to server-media mode and verify browser playback of processed audio` to Completed. Keep or add Next items for:

- per-user server-media cleanup endpoint/session teardown,
- DeepFilterNet provider,
- jitter buffering and packet loss concealment,
- optional Rust WASM client-side noise cancellation.

- [ ] **Step 4: Run final workspace verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path Cargo.toml --workspace
cd frontend
npm test -- --run
npm run typecheck
npm run lint
npm run build
git diff --check
```

Expected: all pass.

- [ ] **Step 5: Commit and push**

Create a Lore-format local commit. Do not include unrelated untracked SDD artifacts unless this workflow explicitly decides to commit the current spec and plan. The `$sdd-workflow` leader performs the final push after reviewing the local commit and verification evidence.
