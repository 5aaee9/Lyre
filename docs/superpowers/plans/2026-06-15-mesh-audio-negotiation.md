# Mesh Audio Negotiation Implementation Plan

> **For agentic workers:** REQUIRED WORKFLOW: Execute this plan as a `$sdd-workflow` subtask. Use the SDD reviewer gates before implementation and after implementation. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Let a browser room client maintain one WebRTC audio peer connection per remote room user.

**Architecture:** Keep backend signalling unchanged and add a frontend mesh controller that owns local media plus a `Map<remoteUserId, RTCPeerConnection>`. React keeps the session in a ref and drives it from user-triggered audio start plus presence/signalling messages.

**Tech Stack:** Next.js, React, TypeScript, Vitest, browser WebRTC APIs.

---

## File Structure

- Modify `frontend/src/lib/webrtc.ts`: split opening microphone from creating a peer connection.
- Modify `frontend/src/lib/webrtc.test.ts`: cover the split helpers and compatibility helper.
- Create `frontend/src/lib/mesh-audio.ts`: mesh audio session controller.
- Create `frontend/src/lib/mesh-audio.test.ts`: unit tests for targeted offers, answers, ICE, cleanup, and peer removal.
- Modify `frontend/src/app/room/[roomId]/room-client.tsx`: replace single peer connection with `MeshAudioSession`.
- Modify `frontend/src/app/room/[roomId]/room-client.test.tsx`: verify multi-peer startup, presence-driven offers, cleanup, ignored pre-audio signals, and ICE fetch failure.
- Post-review docs: `README.md`, `MEMORY.md`, `docs/roadmap.md`.

## Task 0: SDD Pre-Implementation Gate

**Files:**
- Read: `docs/superpowers/specs/2026-06-15-mesh-audio-negotiation-design.md`
- Read: `docs/superpowers/plans/2026-06-15-mesh-audio-negotiation.md`

- [x] **Step 1: Confirm approved spec review exists**

Use the current SDD workflow record for this increment. Required evidence before implementation:

```text
VERDICT: APPROVE
ISSUES:
- None
REQUIRED_CHANGES:
- None
```

The approved spec path is:

```text
docs/superpowers/specs/2026-06-15-mesh-audio-negotiation-design.md
```

- [x] **Step 2: Dispatch independent plan reviewer**

Dispatch an independent reviewer with the approved spec and this plan. Required verdict format:

```text
VERDICT: APPROVE | REVISE
ISSUES:
- [blocking issue or "None"]
REQUIRED_CHANGES:
- [change or "None"]
```

- [x] **Step 3: Stop before code edits unless plan is approved**

Implementation may begin only after the independent plan reviewer returns:

```text
VERDICT: APPROVE
```

If the reviewer returns `REVISE`, update this plan and re-review until approved.

## Task 1: Split WebRTC Helpers

**Files:**
- Modify: `frontend/src/lib/webrtc.ts`
- Modify: `frontend/src/lib/webrtc.test.ts`

- [x] **Step 1: Update helper tests first**

Replace `frontend/src/lib/webrtc.test.ts` with tests that prove microphone opening and peer creation are separate:

```ts
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createAudioPeerConnection, createPeerConnection, openLocalAudioStream } from "./webrtc";

describe("webrtc", () => {
  const addTrack = vi.fn();
  const peerConstructor = vi.fn();
  const stream = {
    getAudioTracks: () => [{ id: "track" }]
  } as unknown as MediaStream;

  class MockPeerConnection {
    addTrack = addTrack;

    constructor(config: RTCConfiguration) {
      peerConstructor(config);
    }
  }

  beforeEach(() => {
    addTrack.mockClear();
    peerConstructor.mockClear();
    vi.stubGlobal("RTCPeerConnection", MockPeerConnection);
    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: {
        getUserMedia: vi.fn(async () => stream)
      }
    });
  });

  it("opens one local audio stream", async () => {
    await expect(openLocalAudioStream()).resolves.toBe(stream);
    expect(navigator.mediaDevices.getUserMedia).toHaveBeenCalledWith({ audio: true });
  });

  it("constructs peer connection with configured ice servers and local tracks", () => {
    createPeerConnection([{ urls: ["stun:stun.example:3478"], username: null, credential: null }], stream);

    expect(peerConstructor).toHaveBeenCalledWith({
      iceServers: [{ urls: ["stun:stun.example:3478"], username: undefined, credential: undefined }]
    });
    expect(addTrack).toHaveBeenCalledWith({ id: "track" }, stream);
  });

  it("keeps compatibility helper for one-off audio peer connection", async () => {
    await createAudioPeerConnection([{ urls: ["stun:stun.example:3478"], username: null, credential: null }]);

    expect(navigator.mediaDevices.getUserMedia).toHaveBeenCalledWith({ audio: true });
    expect(peerConstructor).toHaveBeenCalledOnce();
    expect(addTrack).toHaveBeenCalledWith({ id: "track" }, stream);
  });
});
```

- [x] **Step 2: Run the targeted test and confirm it fails**

Run:

```bash
cd frontend
npm test -- --run src/lib/webrtc.test.ts
```

Expected: fail because `openLocalAudioStream` and `createPeerConnection` are not exported yet.

- [x] **Step 3: Implement split helpers**

Update `frontend/src/lib/webrtc.ts` to:

```ts
import type { IceServerConfig } from "./api";

export async function openLocalAudioStream(): Promise<MediaStream> {
  return navigator.mediaDevices.getUserMedia({ audio: true });
}

export function createPeerConnection(iceServers: IceServerConfig[], stream: MediaStream): RTCPeerConnection {
  const connection = new RTCPeerConnection({
    iceServers: iceServers.map((server) => ({
      urls: server.urls,
      username: server.username ?? undefined,
      credential: server.credential ?? undefined
    }))
  });
  for (const track of stream.getAudioTracks()) {
    connection.addTrack(track, stream);
  }
  return connection;
}

export async function createAudioPeerConnection(iceServers: IceServerConfig[]): Promise<RTCPeerConnection> {
  return createPeerConnection(iceServers, await openLocalAudioStream());
}
```

- [x] **Step 4: Run helper tests**

Run:

```bash
cd frontend
npm test -- --run src/lib/webrtc.test.ts
```

Expected: pass.

## Task 2: Add Mesh Audio Session Controller

**Files:**
- Create: `frontend/src/lib/mesh-audio.ts`
- Create: `frontend/src/lib/mesh-audio.test.ts`

- [x] **Step 1: Add mesh controller tests**

Create `frontend/src/lib/mesh-audio.test.ts` with tests for controller behavior:

```ts
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { IceServerConfig, UserProfile } from "./api";
import { MeshAudioSession } from "./mesh-audio";
import type { SignalMessage } from "./signalling";

const makeUser = (id: string): UserProfile => ({
  id,
  nickname: id,
  joined_at: "2026-06-15T00:00:00Z",
  noise: { provider: "off", intensity: 0.5, voice_activity_threshold: 0.35 }
});

describe("MeshAudioSession", () => {
  const iceServers: IceServerConfig[] = [{ urls: ["stun:stun.example:3478"], username: null, credential: null }];
  const send = vi.fn();
  const stop = vi.fn();
  const stream = {
    getAudioTracks: () => [{ id: "track", stop }]
  } as unknown as MediaStream;
  const peerInstances: MockPeerConnection[] = [];

  class MockPeerConnection {
    onicecandidate: ((event: RTCPeerConnectionIceEvent) => void) | null = null;
    addTrack = vi.fn();
    addIceCandidate = vi.fn();
    close = vi.fn();
    createAnswer = vi.fn(async () => ({ type: "answer", sdp: `answer-${peerInstances.length}` }));
    createOffer = vi.fn(async () => ({ type: "offer", sdp: `offer-${peerInstances.length}` }));
    setLocalDescription = vi.fn();
    setRemoteDescription = vi.fn();

    constructor() {
      peerInstances.push(this);
    }
  }

  function session() {
    return new MeshAudioSession({
      roomId: "DEFAULT",
      currentUserId: "user_a",
      iceServers,
      stream,
      send
    });
  }

  beforeEach(() => {
    send.mockClear();
    stop.mockClear();
    peerInstances.length = 0;
    vi.stubGlobal("RTCPeerConnection", MockPeerConnection);
  });

  it("connects to each remote user with targeted offers", async () => {
    const audio = session();
    await audio.connectToUsers([makeUser("user_a"), makeUser("user_b"), makeUser("user_c")]);

    expect(peerInstances).toHaveLength(2);
    expect(send).toHaveBeenCalledWith(expect.objectContaining({ type: "offer", recipient_id: "user_b" }));
    expect(send).toHaveBeenCalledWith(expect.objectContaining({ type: "offer", recipient_id: "user_c" }));
  });

  it("answers incoming offers on the sender peer", async () => {
    const audio = session();
    await audio.handleSignal({
      type: "offer",
      room_id: "DEFAULT",
      sender_id: "user_b",
      recipient_id: "user_a",
      payload: { type: "offer", sdp: "remote-offer" }
    });

    expect(peerInstances).toHaveLength(1);
    expect(peerInstances[0].setRemoteDescription).toHaveBeenCalledWith({ type: "offer", sdp: "remote-offer" });
    expect(send).toHaveBeenCalledWith(expect.objectContaining({ type: "answer", recipient_id: "user_b" }));
  });

  it("applies answers and ice candidates to the sender peer only", async () => {
    const audio = session();
    await audio.connectToUsers([makeUser("user_b"), makeUser("user_c")]);
    await audio.handleSignal({
      type: "answer",
      room_id: "DEFAULT",
      sender_id: "user_c",
      recipient_id: "user_a",
      payload: { type: "answer", sdp: "answer-c" }
    });
    await audio.handleSignal({
      type: "ice-candidate",
      room_id: "DEFAULT",
      sender_id: "user_c",
      recipient_id: "user_a",
      payload: { type: "ice-candidate", candidate: "candidate-c", sdp_mid: "0", sdp_m_line_index: 0 }
    });

    expect(peerInstances[0].setRemoteDescription).not.toHaveBeenCalledWith({ type: "answer", sdp: "answer-c" });
    expect(peerInstances[1].setRemoteDescription).toHaveBeenCalledWith({ type: "answer", sdp: "answer-c" });
    expect(peerInstances[1].addIceCandidate).toHaveBeenCalledWith({
      candidate: "candidate-c",
      sdpMid: "0",
      sdpMLineIndex: 0
    });
  });

  it("ignores targeted media signals for a different recipient", async () => {
    const audio = session();
    await audio.handleSignal({
      type: "offer",
      room_id: "DEFAULT",
      sender_id: "user_b",
      recipient_id: "other_user",
      payload: { type: "offer", sdp: "remote-offer" }
    } as SignalMessage);

    expect(peerInstances).toHaveLength(0);
    expect(send).not.toHaveBeenCalled();
  });

  it("ignores stale answers and ice candidates without an existing sender peer", async () => {
    const audio = session();
    await audio.handleSignal({
      type: "answer",
      room_id: "DEFAULT",
      sender_id: "user_b",
      recipient_id: "user_a",
      payload: { type: "answer", sdp: "stale-answer" }
    });
    await audio.handleSignal({
      type: "ice-candidate",
      room_id: "DEFAULT",
      sender_id: "user_b",
      recipient_id: "user_a",
      payload: { type: "ice-candidate", candidate: "stale-candidate" }
    });

    expect(peerInstances).toHaveLength(0);
  });

  it("closes removed peers and stops local tracks on close", async () => {
    const audio = session();
    await audio.connectToUsers([makeUser("user_b"), makeUser("user_c")]);

    audio.removePeer("user_b");
    expect(peerInstances[0].close).toHaveBeenCalledOnce();
    expect(peerInstances[1].close).not.toHaveBeenCalled();

    audio.close();
    expect(peerInstances[1].close).toHaveBeenCalledOnce();
    expect(stop).toHaveBeenCalledOnce();
  });
});
```

- [x] **Step 2: Run the mesh test and confirm it fails**

Run:

```bash
cd frontend
npm test -- --run src/lib/mesh-audio.test.ts
```

Expected: fail because `mesh-audio.ts` does not exist yet.

- [x] **Step 3: Implement `MeshAudioSession`**

Create `frontend/src/lib/mesh-audio.ts`:

```ts
import type { IceServerConfig, UserProfile } from "./api";
import { encodeAnswer, encodeIceCandidate, encodeOffer, type SignalMessage } from "./signalling";
import { createPeerConnection } from "./webrtc";

type MeshAudioSessionInput = {
  roomId: string;
  currentUserId: string;
  iceServers: IceServerConfig[];
  stream: MediaStream;
  send: (message: SignalMessage) => void;
  onError?: (message: string) => void;
};

export class MeshAudioSession {
  private readonly peers = new Map<string, RTCPeerConnection>();

  constructor(private readonly input: MeshAudioSessionInput) {}

  async connectToUsers(users: UserProfile[]): Promise<void> {
    for (const user of users) {
      if (user.id !== this.input.currentUserId) {
        await this.createOfferFor(user.id);
      }
    }
  }

  async handleSignal(signal: SignalMessage): Promise<void> {
    if (signal.sender_id === this.input.currentUserId) {
      return;
    }
    if (signal.recipient_id && signal.recipient_id !== this.input.currentUserId) {
      return;
    }

    try {
      if (signal.payload.type === "offer") {
        const peer = this.peerFor(signal.sender_id);
        await peer.setRemoteDescription({ type: "offer", sdp: signal.payload.sdp });
        const answer = await peer.createAnswer();
        await peer.setLocalDescription(answer);
        this.input.send(encodeAnswer(this.input.roomId, this.input.currentUserId, answer.sdp ?? "", signal.sender_id));
      }
      if (signal.payload.type === "answer") {
        const peer = this.peers.get(signal.sender_id);
        if (peer) {
          await peer.setRemoteDescription({ type: "answer", sdp: signal.payload.sdp });
        }
      }
      if (signal.payload.type === "ice-candidate") {
        const peer = this.peers.get(signal.sender_id);
        if (peer) {
          await peer.addIceCandidate({
            candidate: signal.payload.candidate,
            sdpMid: signal.payload.sdp_mid,
            sdpMLineIndex: signal.payload.sdp_m_line_index
          });
        }
      }
    } catch (error) {
      this.reportError(error);
    }
  }

  removePeer(userId: string): void {
    this.peers.get(userId)?.close();
    this.peers.delete(userId);
  }

  close(): void {
    for (const peer of this.peers.values()) {
      peer.close();
    }
    this.peers.clear();
    for (const track of this.input.stream.getAudioTracks()) {
      track.stop();
    }
  }

  private async createOfferFor(userId: string): Promise<void> {
    try {
      const peer = this.peerFor(userId);
      const offer = await peer.createOffer();
      await peer.setLocalDescription(offer);
      this.input.send(encodeOffer(this.input.roomId, this.input.currentUserId, offer.sdp ?? "", userId));
    } catch (error) {
      this.reportError(error);
    }
  }

  private peerFor(userId: string): RTCPeerConnection {
    const existing = this.peers.get(userId);
    if (existing) {
      return existing;
    }
    const peer = createPeerConnection(this.input.iceServers, this.input.stream);
    peer.onicecandidate = (event) => {
      if (event.candidate) {
        this.input.send(
          encodeIceCandidate(this.input.roomId, this.input.currentUserId, event.candidate.toJSON(), userId)
        );
      }
    };
    this.peers.set(userId, peer);
    return peer;
  }

  private reportError(error: unknown): void {
    this.input.onError?.(error instanceof Error ? error.message : "Audio connection failed");
  }
}
```

- [x] **Step 4: Run mesh controller tests**

Run:

```bash
cd frontend
npm test -- --run src/lib/mesh-audio.test.ts
```

Expected: pass.

## Task 3: Wire Room Client to Mesh Session

**Files:**
- Modify: `frontend/src/app/room/[roomId]/room-client.tsx`
- Modify: `frontend/src/app/room/[roomId]/room-client.test.tsx`

- [x] **Step 1: Update room client tests for multi-peer mesh**

Modify `frontend/src/app/room/[roomId]/room-client.test.tsx` so the mocked join response contains the current user plus two remote users:

```ts
const users = [
  {
    id: "user_a",
    nickname: "Ada",
    joined_at: new Date().toISOString(),
    noise: { provider: "off" as const, intensity: 0.5, voice_activity_threshold: 0.35 }
  },
  {
    id: "user_b",
    nickname: "Bob",
    joined_at: new Date().toISOString(),
    noise: { provider: "off" as const, intensity: 0.5, voice_activity_threshold: 0.35 }
  },
  {
    id: "user_c",
    nickname: "Cam",
    joined_at: new Date().toISOString(),
    noise: { provider: "off" as const, intensity: 0.5, voice_activity_threshold: 0.35 }
  }
];
```

Update the API mock room to return `{ room_id: "DEFAULT", users }`.

Change `MockPeerConnection` to push each instance into `peerConnections`, with per-instance spies for `close`, `addIceCandidate`, `setRemoteDescription`, `setLocalDescription`, `createOffer`, and `createAnswer`.

Add or update tests:

- `starts one peer connection per remote user and sends targeted offers`
- `answers incoming offers after audio is started`
- `routes incoming ice candidates to the sender peer`
- `offers to a newly joined user after audio has started`
- `closes a leaving user's peer connection`
- `closes peer connections and stops local tracks on unmount`
- keep `opens presence websocket without requesting microphone`
- keep `ignores webrtc signals before audio is started`
- keep `does not start media when ice server fetch fails`

The key expectations:

```ts
expect(send).toHaveBeenCalledWith(JSON.stringify(expect.objectContaining({ type: "offer", recipient_id: "user_b" })));
expect(send).toHaveBeenCalledWith(JSON.stringify(expect.objectContaining({ type: "offer", recipient_id: "user_c" })));
expect(peerConnections).toHaveLength(2);
```

For `user-joined`, send a WebSocket message:

```ts
sockets[0].onmessage?.(
  new MessageEvent("message", {
    data: JSON.stringify({
      type: "user-joined",
      room_id: "DEFAULT",
      sender_id: "user_d",
      payload: { type: "user-joined", user: makeUser("user_d") }
    })
  })
);
```

Expected: a targeted offer to `user_d`.

For `user-left`, send:

```ts
sockets[0].onmessage?.(
  new MessageEvent("message", {
    data: JSON.stringify({
      type: "user-left",
      room_id: "DEFAULT",
      sender_id: "user_b",
      payload: { type: "user-left", user_id: "user_b" }
    })
  })
);
```

Expected: the `user_b` peer's `close` spy is called once.

- [x] **Step 2: Run the room client targeted test and confirm it fails**

Run:

```bash
cd frontend
npm test -- --run src/app/room/[roomId]/room-client.test.tsx
```

Expected: fail while the component still owns a single `RTCPeerConnection`.

- [x] **Step 3: Update room client implementation**

In `frontend/src/app/room/[roomId]/room-client.tsx`:

- Replace imports:

```ts
import { MeshAudioSession } from "@/lib/mesh-audio";
import { openLocalAudioStream } from "@/lib/webrtc";
```

- Replace `peerRef` with:

```ts
const audioSessionRef = useRef<MeshAudioSession | null>(null);
```

- In `handleSignal`, remove direct peer logic and delegate media signals:

```ts
if (signal.payload.type === "offer" || signal.payload.type === "answer" || signal.payload.type === "ice-candidate") {
  if (!audioStartedRef.current || !audioSessionRef.current) {
    return;
  }
  await audioSessionRef.current.handleSignal(signal);
}
if (signal.payload.type === "user-joined" && audioStartedRef.current && audioSessionRef.current) {
  await audioSessionRef.current.connectToUsers([signal.payload.user]);
}
if (signal.payload.type === "user-left") {
  audioSessionRef.current?.removePeer(signal.payload.user_id);
}
```

- In `connectAudio`, after ICE servers are loaded:

```ts
const stream = await openLocalAudioStream();
const session = new MeshAudioSession({
  roomId,
  currentUserId: currentUser.id,
  iceServers,
  stream,
  send: (message) => socketRef.current?.send(JSON.stringify(message)),
  onError: setStatus
});
audioSessionRef.current = session;
await session.connectToUsers(room?.users ?? []);
setStatus("Audio offers sent");
```

- In cleanup effect:

```ts
audioSessionRef.current?.close();
audioSessionRef.current = null;
audioStartedRef.current = false;
socketRef.current?.close();
```

- In `leave()`, close the session before redirecting:

```ts
audioSessionRef.current?.close();
audioSessionRef.current = null;
audioStartedRef.current = false;
socketRef.current?.close();
socketRef.current = null;
```

- [x] **Step 4: Run room client tests**

Run:

```bash
cd frontend
npm test -- --run src/app/room/[roomId]/room-client.test.tsx
```

Expected: pass.

## Task 4: Documentation and Final Verification

**Files:**
- Modify after implementation review: `README.md`
- Modify after implementation review: `MEMORY.md`
- Modify after implementation review: `docs/roadmap.md`

- [x] **Step 1: Run targeted verification before implementation review**

Run:

```bash
cd frontend
npm test -- --run src/lib/webrtc.test.ts src/lib/mesh-audio.test.ts src/app/room/[roomId]/room-client.test.tsx
npm run typecheck
npm run lint
```

Expected: pass.

- [x] **Step 2: Run Rust verification to prove backend remains intact**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: pass.

- [x] **Step 3: Independent implementation review**

Dispatch a fresh independent implementation reviewer with:

- spec path: `docs/superpowers/specs/2026-06-15-mesh-audio-negotiation-design.md`
- plan path: `docs/superpowers/plans/2026-06-15-mesh-audio-negotiation.md`
- diff
- verification output

Required verdict:

```text
VERDICT: APPROVE | REVISE
SPEC_COVERAGE:
- ...
BLOCKERS:
- ...
REQUIRED_CHANGES:
- ...
```

If the reviewer returns `REVISE`, fix the gaps, rerun targeted verification, and re-review.

- [x] **Step 4: Update docs after implementation approval**

Update `README.md` Media Topology or Frontend section to state that the current P2P mesh frontend now creates one browser `RTCPeerConnection` per remote room user and targets WebRTC signalling messages by `recipient_id`.

Append to `MEMORY.md`:

```md
## 2026-06-15 Mesh Audio Negotiation

- Replaced the single frontend peer connection with a room mesh session keyed by remote user id.
- Kept microphone capture user-triggered and reused one local audio stream across all peer connections.
- Kept backend signalling unchanged; frontend-generated offer, answer, and ICE messages are now targeted with `recipient_id`.
```

Update `docs/roadmap.md`:

- Move "Harden real WebRTC mesh negotiation across multiple browsers" to Completed as "Frontend multi-peer WebRTC mesh negotiation hardened with per-user peer connections."
- Keep real media relay/SFU, RNNoise, and DeepFilterNet in Next.

- [x] **Step 5: Run final verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
cd frontend
npm run generate:webrpc
npm test -- --run
npm run typecheck
npm run lint
npm run build
git diff --check
git diff --stat
git status --short
```

Expected: all checks pass; only intended files changed.

- [x] **Step 6: Commit and push attempt**

Stage the intended files and commit using Lore protocol:

```text
Harden browser mesh audio negotiation

Constraint: Current media topology remains browser P2P mesh; backend signalling schema is unchanged.
Rejected: Adding SFU/media relay behavior in this increment | Server-side media termination is tracked separately.
Confidence: high
Scope-risk: moderate
Directive: Preserve user-triggered microphone access and per-remote-user peer ownership when extending room audio.
Tested: cargo fmt --all --check; cargo clippy --workspace --all-targets -- -D warnings; cargo nextest run --manifest-path Cargo.toml --workspace; npm run generate:webrpc; npm test -- --run; npm run typecheck; npm run lint; npm run build; git diff --check
Not-tested: Real multi-browser end-to-end audio quality and WebRTC glare recovery remain manual/future validation.
```

Run `git push`. If it fails because no remote is configured, report the local commit SHA and exact push error.
