# Media Topology Boundary Implementation Plan

> **For agentic workers:** REQUIRED WORKFLOW: Execute this plan as a `$sdd-workflow` subtask. Use the SDD reviewer gates before implementation and after implementation. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose Lyre's current WebRTC media topology so TURN relay support is clearly separated from future server-side media processing and noise cancellation.

**Architecture:** Add the topology model to `lyre-core::webrtc`, expose it through `lyre-web` at `/api/webrtc/topology`, mirror it in the WebRPC schema/generated TypeScript types, and add a frontend API helper. Runtime WebRTC signalling and REST room behavior remain unchanged.

**Tech Stack:** Rust, Axum, Serde, WebRPC RIDL/generator, TypeScript, Vitest.

---

## File Structure

Every implementation task below is part of this `$sdd-workflow` increment. Do not treat any task as complete until the implementation has passed the final independent SDD implementation review gate in Task 5.

- Modify `crates/lyre-core/src/webrtc.rs`: add topology enum/model/default function and tests.
- Modify `crates/lyre-core/src/lib.rs`: re-export topology types/functions.
- Modify `crates/lyre-web/src/api.rs`: add route handler and route test.
- Modify `proto/lyre.ridl`: add topology DTO/method.
- Regenerate `frontend/src/lib/lyre.gen.ts`.
- Modify `frontend/src/lib/api.ts`: add helper-facing topology types and `getMediaTopology()`.
- Modify `frontend/src/lib/api.test.ts`: add URL and mapping tests.
- Modify `README.md`, `MEMORY.md`, `docs/roadmap.md`.

## Task 1: Core Topology Model

**Files:**
- Modify: `crates/lyre-core/src/webrtc.rs`
- Modify: `crates/lyre-core/src/lib.rs`

**Workflow:** Execute this task under the active `$sdd-workflow`; its changes are reviewed by the final independent implementation reviewer in Task 5.

- [x] **Step 1: Add model types**

In `crates/lyre-core/src/webrtc.rs`, add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaTopologyMode {
    P2pMesh,
    MediaRelay,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaTopology {
    pub mode: MediaTopologyMode,
    pub turn_relay_supported: bool,
    pub server_side_audio_processing: bool,
    pub server_side_noise_cancelling: bool,
    pub server_noise_cancelling_requires: MediaTopologyMode,
}

pub fn current_media_topology() -> MediaTopology {
    MediaTopology {
        mode: MediaTopologyMode::P2pMesh,
        turn_relay_supported: true,
        server_side_audio_processing: false,
        server_side_noise_cancelling: false,
        server_noise_cancelling_requires: MediaTopologyMode::MediaRelay,
    }
}
```

- [x] **Step 2: Add core tests**

In the same test module, add:

```rust
#[test]
fn current_topology_separates_turn_relay_from_server_processing() {
    let topology = current_media_topology();

    assert_eq!(topology.mode, MediaTopologyMode::P2pMesh);
    assert!(topology.turn_relay_supported);
    assert!(!topology.server_side_audio_processing);
    assert!(!topology.server_side_noise_cancelling);
    assert_eq!(
        topology.server_noise_cancelling_requires,
        MediaTopologyMode::MediaRelay
    );
}

#[test]
fn media_topology_serializes_contract_fields() {
    let json = serde_json::to_value(current_media_topology()).unwrap();

    assert_eq!(json["mode"], "p2p_mesh");
    assert_eq!(json["turn_relay_supported"], true);
    assert_eq!(json["server_side_audio_processing"], false);
    assert_eq!(json["server_side_noise_cancelling"], false);
    assert_eq!(json["server_noise_cancelling_requires"], "media_relay");
}
```

- [x] **Step 3: Re-export from core**

In `crates/lyre-core/src/lib.rs`, change the `webrtc` export to include:

```rust
pub use webrtc::{current_media_topology, default_ice_servers, IceServerConfig, MediaTopology, MediaTopologyMode};
```

## Task 2: Axum Topology Endpoint

**Files:**
- Modify: `crates/lyre-web/src/api.rs`

**Workflow:** Execute this task under the active `$sdd-workflow`; its changes are reviewed by the final independent implementation reviewer in Task 5.

- [x] **Step 1: Add route and handler**

Add route:

```rust
.route("/api/webrtc/topology", get(media_topology))
```

Add handler:

```rust
async fn media_topology() -> Json<lyre_core::MediaTopology> {
    Json(lyre_core::current_media_topology())
}
```

- [x] **Step 2: Add route test**

In `crates/lyre-web/src/api.rs` tests, add:

```rust
#[tokio::test]
async fn media_topology_route_documents_current_runtime_boundary() {
    let app = router(AppState::default());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/webrtc/topology")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["mode"], "p2p_mesh");
    assert_eq!(body["turn_relay_supported"], true);
    assert_eq!(body["server_side_audio_processing"], false);
    assert_eq!(body["server_side_noise_cancelling"], false);
    assert_eq!(body["server_noise_cancelling_requires"], "media_relay");
}
```

## Task 3: WebRPC and Frontend API Contract

**Files:**
- Modify: `proto/lyre.ridl`
- Regenerate: `frontend/src/lib/lyre.gen.ts`
- Modify: `frontend/src/lib/api.ts`
- Modify: `frontend/src/lib/api.test.ts`

**Workflow:** Execute this task under the active `$sdd-workflow`; its changes are reviewed by the final independent implementation reviewer in Task 5.

- [x] **Step 1: Update RIDL**

Add to `proto/lyre.ridl`:

```ridl
enum MediaTopologyMode: uint32
  - P2P_MESH = 0
  - MEDIA_RELAY = 1

struct MediaTopology
  - mode: MediaTopologyMode
  - turnRelaySupported: bool
  - serverSideAudioProcessing: bool
  - serverSideNoiseCancelling: bool
  - serverNoiseCancellingRequires: MediaTopologyMode
```

Add service method:

```ridl
  # Documents GET /api/webrtc/topology; REST fetch remains the runtime transport in this increment.
  - GetMediaTopology() => (topology: MediaTopology)
```

- [x] **Step 2: Regenerate client**

Run:

```bash
cd frontend
npm run generate:webrpc
```

Expected: generator exits 0 and updates `frontend/src/lib/lyre.gen.ts`.

- [x] **Step 3: Add helper-facing topology types and helper**

In `frontend/src/lib/api.ts`, import:

```ts
import type { MediaTopology as WebrpcMediaTopology } from "./lyre.gen";
import { MediaTopologyMode as WebrpcMediaTopologyMode } from "./lyre.gen";
```

Add:

```ts
export type MediaTopologyMode = "p2p_mesh" | "media_relay";

export function generatedMediaTopologyModeToRest(mode: WebrpcMediaTopologyMode): MediaTopologyMode {
  switch (mode) {
    case WebrpcMediaTopologyMode.MEDIA_RELAY:
      return "media_relay";
    case WebrpcMediaTopologyMode.P2P_MESH:
      return "p2p_mesh";
  }
}

export type MediaTopology = Omit<
  WebrpcMediaTopology,
  | "mode"
  | "turnRelaySupported"
  | "serverSideAudioProcessing"
  | "serverSideNoiseCancelling"
  | "serverNoiseCancellingRequires"
> & {
  mode: MediaTopologyMode;
  turn_relay_supported: WebrpcMediaTopology["turnRelaySupported"];
  server_side_audio_processing: WebrpcMediaTopology["serverSideAudioProcessing"];
  server_side_noise_cancelling: WebrpcMediaTopology["serverSideNoiseCancelling"];
  server_noise_cancelling_requires: MediaTopologyMode;
};

export async function getMediaTopology(): Promise<MediaTopology> {
  const response = await fetch(`${apiBaseUrl()}/api/webrtc/topology`);
  return response.json();
}
```

- [x] **Step 4: Add frontend tests**

In `frontend/src/lib/api.test.ts`, import `getMediaTopology`, `generatedMediaTopologyModeToRest`, `type MediaTopology`, and `MediaTopologyMode as WebrpcMediaTopologyMode`.

Add type assignment:

```ts
const mediaTopologyFromGeneratedDerivedShape: MediaTopology = {
  mode: "p2p_mesh",
  turn_relay_supported: true,
  server_side_audio_processing: false,
  server_side_noise_cancelling: false,
  server_noise_cancelling_requires: "media_relay"
};
void mediaTopologyFromGeneratedDerivedShape;
```

Add tests:

```ts
it("maps generated topology mode values to REST topology strings", () => {
  expect(generatedMediaTopologyModeToRest(WebrpcMediaTopologyMode.P2P_MESH)).toBe("p2p_mesh");
  expect(generatedMediaTopologyModeToRest(WebrpcMediaTopologyMode.MEDIA_RELAY)).toBe("media_relay");
});

it("fetches media topology from API", async () => {
  await getMediaTopology();

  expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/webrtc/topology");
});
```

## Task 4: Documentation and Roadmap

**Files:**
- Modify: `README.md`
- Modify: `MEMORY.md`
- Modify: `docs/roadmap.md`

**Workflow:** Execute this task under the active `$sdd-workflow`; its changes are reviewed by the final independent implementation reviewer in Task 5.

- [x] **Step 1: Update README**

Add a short section stating:

```markdown
## Media Topology

`GET /api/webrtc/topology` reports the active media topology. The current topology is peer-to-peer mesh WebRTC with TURN relay support for NAT traversal.

TURN, including a future `turn-rs` integration, relays encrypted WebRTC packets and cannot run server-side RNNoise or DeepFilterNet by itself. Server-side noise cancellation requires a future media relay/SFU-like path that terminates WebRTC media, decodes audio to PCM, runs `lyre-noise-cancelling`, then re-encodes and broadcasts processed audio.
```

Also list `GET /api/webrtc/topology` in API routes.

- [x] **Step 2: Update MEMORY**

Append:

```markdown
## 2026-06-14 Media Topology Boundary

- Made the current media topology explicit: browser P2P mesh with TURN relay support, no server-side audio processing.
- Recorded `turn-rs` as a future TURN relay candidate, not a server-side noise cancellation mechanism.
- Server-side RNNoise/DeepFilterNet requires a future media relay that terminates WebRTC media and processes decoded PCM before broadcast.
```

- [x] **Step 3: Update roadmap**

Move media topology boundary API to Completed. Keep embedded `turn-rs` TURN service/dynamic credentials and media relay/server-side noise cancellation as separate Next items.

## Task 5: Verification and Review

**Files:**
- All changed files.

**Workflow:** Execute this task under the active `$sdd-workflow`; do not commit or push until the independent implementation reviewer returns `VERDICT: APPROVE`.

- [x] **Step 1: Run frontend verification**

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

Expected: all commands exit 0.

- [x] **Step 2: Run Rust verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: all commands exit 0.

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

- [x] **Step 4: Fix review blockers before final verification**

If the reviewer returns anything other than `VERDICT: APPROVE`, fix blockers, rerun relevant verification, and repeat Task 5.

- [x] **Step 5: Run fresh final verification after reviewer approval**

After `VERDICT: APPROVE`, rerun:

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

Then run from the repository root:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
git diff --check
git diff --stat
git diff
git status --short
```

Expected: all verification commands exit 0, `git diff --check` prints no whitespace errors, `git diff --stat` and `git diff` show only intended files and changes for this increment, and `git status --short` shows only intended files.

- [ ] **Step 6: Commit with Lore protocol and push**

Stage only intended files, commit with the repository Lore commit protocol, then run:

```bash
git push
```

If push fails because no remote/upstream/credentials exist, report the exact push error together with the successful local commit SHA.
