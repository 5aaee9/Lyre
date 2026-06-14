# WebRPC Contract Design

## Goal

Introduce a formal WebRPC contract for Lyre's existing HTTP API surface so the frontend has a generated, reviewable client/types source instead of hand-maintained request and response shapes.

## Scope

In scope:

- Add a RIDL schema at `proto/lyre.ridl`.
- Model the existing REST endpoints that are currently used by the frontend:
  - `GET /api/rooms/{room_id}`
  - `POST /api/rooms/{room_id}/join`
  - `POST /api/rooms/{room_id}/leave`
  - `GET /api/noise/providers`
  - `GET /api/webrtc/ice-servers`
- Model the DTOs already shared conceptually between Rust and TypeScript:
  - `NoiseProvider`
  - `NoiseCancellationConfig`
  - `IceServerConfig`
  - `UserProfile`
  - `RoomSnapshot`
  - `JoinRoomInput`
  - `JoinRoomResponse`
- Add a generated TypeScript WebRPC client file under `frontend/src/lib/`.
- Add an npm script that regenerates the client from `proto/lyre.ridl`.
- Keep the existing `frontend/src/lib/api.ts` public helpers as the application-facing API surface, keep their current `fetch` transport, and source their DTO types from the generated WebRPC file.
- Add tests that prove the existing app helpers still call the same URLs and serialize the same JSON bodies.
- Update README, MEMORY, and roadmap.

Out of scope:

- Replacing the Axum route implementation with a WebRPC-generated Rust server.
- Switching the frontend to generated WebRPC transport.
- Changing WebSocket signalling.
- Changing REST URL paths or JSON field names.
- Adding a new runtime endpoint beyond the existing REST API.
- Modelling health checks in WebRPC; health remains an operational endpoint.

## Contract

The RIDL schema must use:

```ridl
webrpc = v1

name = lyre
version = v0.1.0
basepath = /rpc
```

`basepath = /rpc` is documentation for the future generated WebRPC runtime. It is not a replacement for the current REST routes in this increment.

The schema must expose one service named `Lyre`.

Method names and mappings:

- `GetRoom(roomID: string) => (room: RoomSnapshot)` documents `GET /api/rooms/{room_id}`.
- `JoinRoom(roomID: string, nickname?: string, noise?: NoiseCancellationConfig) => (user: UserProfile, room: RoomSnapshot)` documents `POST /api/rooms/{room_id}/join`.
- `LeaveRoom(roomID: string, userID: string) => (room: RoomSnapshot)` documents `POST /api/rooms/{room_id}/leave`.
- `GetNoiseProviders() => (providers: []NoiseCancellationConfig)` documents `GET /api/noise/providers`.
- `GetIceServers() => (iceServers: []IceServerConfig)` documents `GET /api/webrtc/ice-servers`.

These REST mappings must be comments next to service methods, not custom RIDL annotations. Runtime compatibility remains enforced by helper tests in `frontend/src/lib/api.test.ts`.

Exact DTO definitions:

```ridl
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
```

Request/response helper types must be derived from generated DTOs in `frontend/src/lib/api.ts`. Generated imports must use `Webrpc*` aliases so the module can keep exporting the existing helper-facing names:

```ts
import type {
  IceServerConfig as WebrpcIceServerConfig,
  JoinRoomInput as WebrpcJoinRoomInput,
  JoinRoomResponse as WebrpcJoinRoomResponse,
  NoiseCancellationConfig as WebrpcNoiseCancellationConfig,
  NoiseProvider as WebrpcNoiseProvider,
  RoomSnapshot as WebrpcRoomSnapshot,
  UserProfile as WebrpcUserProfile
} from "./lyre.gen";
```

The generated DTO names use WebRPC-friendly camelCase fields. The existing REST server JSON uses snake_case fields and lowercase provider strings. To preserve compatibility without duplicating DTO definitions, `frontend/src/lib/api.ts` must export helper-facing mapped types from the generated DTOs:

```ts
export type NoiseProvider = "off" | "rnnoise" | "deepfilternet";

export type NoiseCancellationConfig = Omit<WebrpcNoiseCancellationConfig, "provider" | "voiceActivityThreshold"> & {
  provider: NoiseProvider;
  voice_activity_threshold: WebrpcNoiseCancellationConfig["voiceActivityThreshold"];
};
```

The actual exported names used by the rest of the app (`NoiseCancellationConfig`, `RoomSnapshot`, and related types) may remain stable by aliasing or mapping from generated types, but they must not be fully hand-redeclared independently of `lyre.gen.ts`.

`joinedAt` is `timestamp` in RIDL because WebRPC timestamps are ISO 8601 over the wire. The current REST helper-facing type may continue exposing `joined_at: string` as a compatibility mapping.

`NoiseProvider` compatibility must be explicit:

- Generated `NoiseProvider.OFF` maps to REST/helper `"off"`.
- Generated `NoiseProvider.RNNOISE` maps to REST/helper `"rnnoise"`.
- Generated `NoiseProvider.DEEPFILTERNET` maps to REST/helper `"deepfilternet"`.
- `parseNoiseProvider()` continues returning the lowercase helper-facing union.

`IceServerConfig` compatibility must preserve current REST nullability:

```ts
export type IceServerConfig = Omit<WebrpcIceServerConfig, "username" | "credential"> & {
  username?: WebrpcIceServerConfig["username"] | null;
  credential?: WebrpcIceServerConfig["credential"] | null;
};
```

`JoinRoomInput` and `JoinRoomResponse` must exist as explicit RIDL structs even if the service method could generate request/response wrappers. `api.ts` should derive helper-facing join types from those generated structs, with the snake_case and lowercase-provider compatibility mappings above.

## Frontend Integration

`frontend/src/lib/api.ts` keeps the existing helper function names:

- `getRoom`
- `joinRoom`
- `leaveRoom`
- `getNoiseProviders`
- `getIceServers`

Those helpers must keep their current URL paths and JSON request bodies. They must continue using `fetch` directly in this increment. This increment's required behavioral change is type/source-of-truth alignment, not transport replacement.

## Tooling

Add an npm script in `frontend/package.json`:

```bash
npm run generate:webrpc
```

The script must run WebRPC generation from the repository root schema into `frontend/src/lib/lyre.gen.ts` with a pinned generator version:

```bash
go run -ldflags="-s -w -X github.com/webrpc/webrpc.VERSION=v0.36.0" github.com/webrpc/webrpc/cmd/webrpc-gen@v0.36.0 -schema=../proto/lyre.ridl -target=typescript -client -out=./src/lib/lyre.gen.ts
```

The script runs from `frontend/`, so the schema path is `../proto/lyre.ridl`. README must document that regenerating the WebRPC client requires Go and network access the first time Go downloads the pinned generator module.

The generated file must be committed so a clean checkout can typecheck without requiring developers or CI to have `webrpc-gen` installed before running normal frontend checks.

## Testing

- Existing API helper tests must continue to pass and cover URL/body compatibility.
- Add a test that verifies the helper-facing room join response type is assignable from values shaped by the generated DTO-derived mappings.
- Full verification remains:
  - `npm test -- --run`
  - `npm run typecheck`
  - `npm run lint`
  - `npm run build`
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo nextest run --manifest-path "Cargo.toml" --workspace`

## Acceptance Criteria

- `proto/lyre.ridl` exists and documents the current frontend-consumed REST contract.
- `frontend/src/lib/lyre.gen.ts` exists and exports WebRPC-generated client/types from that schema.
- `frontend/src/lib/api.ts` imports contract types from `lyre.gen.ts` and derives existing snake_case helper-facing types from those generated DTOs.
- Frontend API behavior remains compatible with the existing REST server.
- README documents WebRPC generation.
- MEMORY records why this increment keeps REST runtime behavior while introducing WebRPC as the contract source.
- Roadmap moves formal WebRPC IDL/client bindings from TODO to completed, while leaving generated Rust server integration as future work.
