# WebRPC Contract Implementation Plan

> **For agentic workers:** REQUIRED WORKFLOW: Execute this plan as a `$sdd-workflow` subtask. Use the SDD reviewer gates before implementation and after implementation. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a formal WebRPC RIDL contract and generated TypeScript client/types for Lyre's existing frontend-consumed REST API without changing runtime REST behavior.

**Architecture:** `proto/lyre.ridl` becomes the contract source. `frontend/src/lib/lyre.gen.ts` is generated from that schema and committed. `frontend/src/lib/api.ts` keeps the existing fetch-based REST helpers while deriving stable snake_case helper-facing types from generated WebRPC DTOs.

**Tech Stack:** WebRPC RIDL, `webrpc-gen` v0.36.0, TypeScript, Vitest, Next.js, Rust/Axum unchanged.

---

## File Structure

- Create `proto/lyre.ridl`: WebRPC schema for current REST DTOs and service methods.
- Create `frontend/src/lib/lyre.gen.ts`: generated TypeScript WebRPC client/types.
- Modify `frontend/package.json`: add `generate:webrpc`.
- Modify `frontend/src/lib/api.ts`: import generated types with `Webrpc*` aliases and derive current helper-facing types.
- Modify `frontend/src/lib/api.test.ts`: keep URL/body compatibility tests and add generated-type-derived assignment coverage.
- Modify `README.md`: document WebRPC generation.
- Modify `MEMORY.md`: record the REST-runtime/WebRPC-contract decision.
- Modify `docs/roadmap.md`: move formal WebRPC IDL/client bindings to completed and keep generated Rust server integration as future work.

## Task 1: Add WebRPC Schema and Generated Client

**Files:**
- Create: `proto/lyre.ridl`
- Create: `frontend/src/lib/lyre.gen.ts`
- Modify: `frontend/package.json`

- [x] **Step 1: Create the RIDL schema**

Create `proto/lyre.ridl` with:

```ridl
webrpc = v1

name = lyre
version = v0.1.0
basepath = /rpc

enum NoiseProvider: uint32
  - OFF = 0
  - RNNOISE = 1
  - DEEPFILTERNET = 2

struct NoiseCancellationConfig
  - provider: NoiseProvider
  - intensity: float32
  - voiceActivityThreshold: float32

struct IceServerConfig
  - urls: []string
  - username?: string
  - credential?: string

struct JoinRoomInput
  - nickname?: string
  - noise?: NoiseCancellationConfig

struct UserProfile
  - id: string
  - nickname: string
  - joinedAt: timestamp
  - noise: NoiseCancellationConfig

struct RoomSnapshot
  - roomID: string
  - users: []UserProfile

struct JoinRoomResponse
  - user: UserProfile
  - room: RoomSnapshot

service Lyre
  # Documents GET /api/rooms/{room_id}; REST fetch remains the runtime transport in this increment.
  - GetRoom(roomID: string) => (room: RoomSnapshot)
  # Documents POST /api/rooms/{room_id}/join; REST fetch remains the runtime transport in this increment.
  - JoinRoom(roomID: string, nickname?: string, noise?: NoiseCancellationConfig) => (user: UserProfile, room: RoomSnapshot)
  # Documents POST /api/rooms/{room_id}/leave; REST fetch remains the runtime transport in this increment.
  - LeaveRoom(roomID: string, userID: string) => (room: RoomSnapshot)
  # Documents GET /api/noise/providers; REST fetch remains the runtime transport in this increment.
  - GetNoiseProviders() => (providers: []NoiseCancellationConfig)
  # Documents GET /api/webrtc/ice-servers; REST fetch remains the runtime transport in this increment.
  - GetIceServers() => (iceServers: []IceServerConfig)
```

- [x] **Step 2: Add the generation script**

In `frontend/package.json`, add:

```json
"generate:webrpc": "go run -ldflags=\"-s -w -X github.com/webrpc/webrpc.VERSION=v0.36.0\" github.com/webrpc/webrpc/cmd/webrpc-gen@v0.36.0 -schema=../proto/lyre.ridl -target=typescript -client -out=./src/lib/lyre.gen.ts"
```

- [x] **Step 3: Generate the TypeScript client**

Run:

```bash
cd frontend
npm run generate:webrpc
```

Expected: `frontend/src/lib/lyre.gen.ts` is created and the generator summary reports `webrpc-gen version : v0.36.0`.

## Task 2: Derive API Helper Types From Generated DTOs

**Files:**
- Modify: `frontend/src/lib/api.ts`
- Modify: `frontend/src/lib/api.test.ts`

- [x] **Step 1: Import generated DTOs with aliases**

At the top of `frontend/src/lib/api.ts`, import generated DTOs using aliases:

```ts
import type {
  IceServerConfig as WebrpcIceServerConfig,
  JoinRoomInput as WebrpcJoinRoomInput,
  JoinRoomResponse as WebrpcJoinRoomResponse,
  NoiseCancellationConfig as WebrpcNoiseCancellationConfig,
  RoomSnapshot as WebrpcRoomSnapshot,
  UserProfile as WebrpcUserProfile
} from "./lyre.gen";
import { NoiseProvider as WebrpcNoiseProvider } from "./lyre.gen";
import { runtimeConfig } from "./runtime-config";
```

- [x] **Step 2: Replace local DTO definitions with generated-derived helper-facing types**

Keep the existing REST/helper-facing exported names and lowercase/snake_case behavior:

```ts
export type NoiseProvider = "off" | "rnnoise" | "deepfilternet";

export type NoiseCancellationConfig = Omit<WebrpcNoiseCancellationConfig, "provider" | "voiceActivityThreshold"> & {
  provider: NoiseProvider;
  voice_activity_threshold: WebrpcNoiseCancellationConfig["voiceActivityThreshold"];
};

export type IceServerConfig = Omit<WebrpcIceServerConfig, "username" | "credential"> & {
  username?: WebrpcIceServerConfig["username"] | null;
  credential?: WebrpcIceServerConfig["credential"] | null;
};

export type UserProfile = Omit<WebrpcUserProfile, "joinedAt" | "noise"> & {
  joined_at: string;
  noise: NoiseCancellationConfig;
};

export type RoomSnapshot = Omit<WebrpcRoomSnapshot, "roomID" | "users"> & {
  room_id: string;
  users: UserProfile[];
};

export type JoinRoomInput = Omit<WebrpcJoinRoomInput, "noise"> & {
  noise?: NoiseCancellationConfig;
};

export type JoinRoomResponse = Omit<WebrpcJoinRoomResponse, "user" | "room"> & {
  user: UserProfile;
  room: RoomSnapshot;
};
```

Keep `parseNoiseProvider()` returning the lowercase helper union.

Use the `WebrpcNoiseProvider` alias in a local compatibility map or type assertion so the generated provider enum remains connected to the helper-facing lowercase union without exporting uppercase provider values to the app.

- [x] **Step 3: Keep existing fetch helper behavior unchanged**

Do not instantiate the generated `Lyre` WebRPC client in `api.ts`. Keep `roomUrl`, `getRoom`, `joinRoom`, `leaveRoom`, `getNoiseProviders`, and `getIceServers` using the same URLs and JSON bodies they use today.

- [x] **Step 4: Add type assignment coverage**

In `frontend/src/lib/api.test.ts`, add type-only coverage near the imports:

```ts
import { NoiseProvider as WebrpcNoiseProvider, type JoinRoomResponse as WebrpcJoinRoomResponse } from "./lyre.gen";
import { generatedNoiseProviderToRest, type JoinRoomResponse, type NoiseProvider } from "./api";

const providerFromGenerated: NoiseProvider = generatedNoiseProviderToRest(WebrpcNoiseProvider.OFF);
void providerFromGenerated;

const joinResponseFromGeneratedDerivedShape: JoinRoomResponse = {
  user: {
    id: "user_a",
    nickname: "Ada",
    joined_at: "2026-06-14T00:00:00Z",
    noise: { provider: "off", intensity: 0.5, voice_activity_threshold: 0.35 }
  },
  room: {
    room_id: "DEFAULT",
    users: []
  }
};
void joinResponseFromGeneratedDerivedShape;

const generatedJoinRoomResponseContract: WebrpcJoinRoomResponse = {
  user: {
    id: "user_a",
    nickname: "Ada",
    joinedAt: "2026-06-14T00:00:00Z",
    noise: { provider: WebrpcNoiseProvider.OFF, intensity: 0.5, voiceActivityThreshold: 0.35 }
  },
  room: {
    roomID: "DEFAULT",
    users: []
  }
};
void generatedJoinRoomResponseContract;
```

Expected: `npm run typecheck` accepts the generated imports and helper-facing types.

## Task 3: Documentation and Roadmap

**Files:**
- Modify: `README.md`
- Modify: `MEMORY.md`
- Modify: `docs/roadmap.md`

- [x] **Step 1: Document generation**

In README, add a WebRPC section:

```markdown
## WebRPC Contract

The formal WebRPC schema lives at `proto/lyre.ridl`. The committed generated TypeScript client/types live at `frontend/src/lib/lyre.gen.ts`.

To regenerate the client:

```bash
cd frontend
npm run generate:webrpc
```

This uses `go run github.com/webrpc/webrpc/cmd/webrpc-gen@v0.36.0`; the first run needs network access for Go module download. The current runtime still uses the Axum REST routes, with WebRPC acting as the checked-in contract and generated TypeScript type source.
```

- [x] **Step 2: Update MEMORY**

Append:

```markdown
## 2026-06-14 WebRPC Contract

- Added `proto/lyre.ridl` as the formal API contract for frontend-consumed room, noise, and ICE server HTTP DTOs.
- Committed `frontend/src/lib/lyre.gen.ts` so normal frontend typecheck/build does not require WebRPC generator installation.
- Kept runtime calls on the existing Axum REST endpoints for this increment; generated WebRPC server/runtime integration remains future work.
```

- [x] **Step 3: Update roadmap**

Move formal WebRPC IDL/client bindings into Completed and add future generated WebRPC Rust server integration under Next.

## Task 4: Verification

**Files:**
- All files changed in Tasks 1-3.

- [x] **Step 1: Run frontend checks**

Run:

```bash
cd frontend
npm test -- --run
npm run typecheck
npm run lint
npm run build
```

Expected: all commands exit 0.

- [x] **Step 2: Run Rust checks**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: all commands exit 0.

- [x] **Step 3: Check generated-client reproducibility**

Run:

```bash
cd frontend
npm run generate:webrpc
git diff --exit-code -- src/lib/lyre.gen.ts
```

Expected: generation exits 0 and `lyre.gen.ts` has no diff after regeneration.

## Task 5: Independent Implementation Review

**Files:**
- All files changed in Tasks 1-4.

- [x] **Step 1: Request SDD implementation review**

Dispatch a fresh independent reviewer with:

- approved spec path: `docs/superpowers/specs/2026-06-14-webrpc-contract-design.md`
- reviewed plan path: `docs/superpowers/plans/2026-06-14-webrpc-contract.md`
- full diff for this increment
- verification output from Task 4

Require this exact verdict format:

```text
VERDICT: APPROVE | REVISE
SPEC_COVERAGE:
- [implemented requirement or missing requirement]
BLOCKERS:
- [blocking gap or "None"]
REQUIRED_CHANGES:
- [change or "None"]
```

- [x] **Step 2: Fix review blockers before final verification**

If the reviewer returns anything other than `VERDICT: APPROVE`, fix the blockers, rerun relevant verification, and repeat Task 5.
