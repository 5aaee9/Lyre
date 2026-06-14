# Media Relay Skeleton Design

## Goal

Add a server-side media relay state and API skeleton that establishes the boundary required for future WebRTC media termination, noise processing, and broadcast.

## Context

Lyre currently uses browser peer-to-peer WebRTC mesh. The server owns rooms, presence, signalling, ICE configuration, and optional TURN relay, but it does not receive decoded audio. Server-side RNNoise or DeepFilterNet requires a future media relay/SFU-like runtime that terminates WebRTC media and processes decoded PCM before broadcasting processed audio to other clients.

This increment does not implement actual WebRTC media termination. It adds the smallest durable contract and state model for server-owned media relay sessions so later work can attach a real WebRTC stack, decoder, noise processor, encoder, and broadcaster without overloading the existing P2P signalling path.

## Scope

Implement a media relay skeleton:

- Add `crates/lyre-core/src/media.rs`.
- Add core DTOs:
  - `MediaRelayMode`
  - `MediaRelayStatus`
  - `MediaTrackKind`
  - `MediaRelayParticipant`
  - `MediaRelayRoomStatus`
  - `StartMediaRelayRequest`
  - `StopMediaRelayRequest`
  - `RegisterMediaTrackRequest`
  - `MediaRelayRegistry`
- Add in-memory room-scoped relay state:
  - relay inactive by default
  - starting relay sets status to active and records the selected noise config
  - stopping relay clears participants/tracks for that room
  - registering a track requires an active relay and stores participant/user/track metadata
- Add REST endpoints:
  - `GET /api/rooms/{room_id}/media-relay`
  - `POST /api/rooms/{room_id}/media-relay/start`
  - `POST /api/rooms/{room_id}/media-relay/stop`
  - `POST /api/rooms/{room_id}/media-relay/tracks`
- Add WebRPC RIDL DTO/service definitions for these REST endpoints and regenerate `frontend/src/lib/lyre.gen.ts`.
- Keep frontend behavior unchanged except generated WebRPC types.
- Update `/api/webrtc/topology` so `server_noise_cancelling_requires` still points at `media_relay`; do not claim actual server-side audio processing yet.
- Update README, MEMORY, roadmap, and AGENTS.md after implementation review.

## Non-Goals

- Running a real SFU or WebRTC media server.
- Accepting SDP offers for server-terminated media.
- Decoding Opus, processing PCM, encoding audio, or broadcasting audio.
- Adding RNNoise or DeepFilterNet native bindings.
- Changing the existing browser P2P signalling flow.
- Adding frontend controls for media relay mode.
- Authentication or access control.

## Behavior

`GET /api/rooms/{room_id}/media-relay` returns a status object for the room. Unknown valid rooms are allowed and return inactive status, matching current room snapshot behavior.

`POST /api/rooms/{room_id}/media-relay/start` accepts:

- optional `noise` config

It sets the room relay to active. If `noise` is absent, use `NoiseCancellationConfig::default()`.

`POST /api/rooms/{room_id}/media-relay/stop` accepts:

- `user_id`

It sets the room relay to inactive and clears participants/tracks. This increment does not enforce that the user is an owner.

`POST /api/rooms/{room_id}/media-relay/tracks` accepts:

- `user_id`
- `track_id`
- `kind`

It rejects registration if the room relay is inactive. On success it records or replaces that user's track metadata and returns the updated room relay status.

## Data Shape

REST JSON uses the existing Rust serde convention: snake_case field names. The WebRPC RIDL uses its own existing field naming style for generated TypeScript, such as `roomID`, but runtime REST responses stay snake_case.

Enums:

- `MediaTrackKind`: `audio` / `video`
- `MediaRelayStatus`: `inactive` / `active`
- `MediaRelayMode`: `p2p_mesh` / `media_relay`

Initial `MediaRelayRoomStatus` fields:

- `room_id`
- `status`
- `mode`
- `server_side_audio_processing`
- `server_side_noise_cancelling`
- `noise`
- `participants`

For this skeleton, active relay status still reports `server_side_audio_processing = false` and `server_side_noise_cancelling = false`, because no media runtime is attached yet. The `noise` field records the intended processing config for future relay processing.

`MediaRelayParticipant` fields:

- `user_id`
- `tracks`

`MediaRelayTrack` fields:

- `track_id`
- `kind`

One user may have multiple tracks, but only one track per `(user_id, track_id)`. Registering the same `track_id` for the same user replaces that track's `kind`. Participants are sorted by `user_id`, and each participant's tracks are sorted by `track_id` for stable JSON and tests.

Default inactive status example:

```json
{
  "room_id": "DEFAULT",
  "status": "inactive",
  "mode": "p2p_mesh",
  "server_side_audio_processing": false,
  "server_side_noise_cancelling": false,
  "noise": {
    "provider": "off",
    "intensity": 0.5,
    "voice_activity_threshold": 0.35
  },
  "participants": []
}
```

Start request:

```json
{
  "noise": {
    "provider": "rnnoise",
    "intensity": 0.8,
    "voice_activity_threshold": 0.2
  }
}
```

Start response:

```json
{
  "room_id": "DEFAULT",
  "status": "active",
  "mode": "media_relay",
  "server_side_audio_processing": false,
  "server_side_noise_cancelling": false,
  "noise": {
    "provider": "rnnoise",
    "intensity": 0.8,
    "voice_activity_threshold": 0.2
  },
  "participants": []
}
```

Track registration request:

```json
{
  "user_id": "user_01",
  "track_id": "audio-main",
  "kind": "audio"
}
```

Track registration response:

```json
{
  "room_id": "DEFAULT",
  "status": "active",
  "mode": "media_relay",
  "server_side_audio_processing": false,
  "server_side_noise_cancelling": false,
  "noise": {
    "provider": "rnnoise",
    "intensity": 0.8,
    "voice_activity_threshold": 0.2
  },
  "participants": [
    {
      "user_id": "user_01",
      "tracks": [
        {
          "track_id": "audio-main",
          "kind": "audio"
        }
      ]
    }
  ]
}
```

Stop request:

```json
{
  "user_id": "user_01"
}
```

Stop response is the same shape as the default inactive status and has an empty `participants` array.

Inactive track registration returns HTTP `409 Conflict`:

```json
{
  "error": "media relay is not active for room `DEFAULT`"
}
```

Core error type:

```rust
#[derive(Debug, Error, PartialEq, Eq)]
pub enum MediaRelayError {
    #[error("media relay is not active for room `{room_id}`")]
    Inactive { room_id: RoomId },
}
```

## WebRPC RIDL Additions

Add these definitions to `proto/lyre.ridl`:

```text
enum MediaRelayStatus: uint32
  - INACTIVE = 0
  - ACTIVE = 1

enum MediaRelayMode: uint32
  - P2P_MESH = 0
  - MEDIA_RELAY = 1

enum MediaTrackKind: uint32
  - AUDIO = 0
  - VIDEO = 1

struct MediaRelayTrack
  - trackID: string
  - kind: MediaTrackKind

struct MediaRelayParticipant
  - userID: string
  - tracks: []MediaRelayTrack

struct MediaRelayRoomStatus
  - roomID: string
  - status: MediaRelayStatus
  - mode: MediaRelayMode
  - serverSideAudioProcessing: bool
  - serverSideNoiseCancelling: bool
  - noise: NoiseCancellationConfig
  - participants: []MediaRelayParticipant

struct StartMediaRelayInput
  - noise?: NoiseCancellationConfig

struct StopMediaRelayInput
  - userID: string

struct RegisterMediaTrackInput
  - userID: string
  - trackID: string
  - kind: MediaTrackKind
```

Add service methods:

```text
  # Documents GET /api/rooms/{room_id}/media-relay; REST fetch remains the runtime transport in this increment.
  - GetMediaRelay(roomID: string) => (mediaRelay: MediaRelayRoomStatus)
  # Documents POST /api/rooms/{room_id}/media-relay/start; REST fetch remains the runtime transport in this increment.
  - StartMediaRelay(roomID: string, noise?: NoiseCancellationConfig) => (mediaRelay: MediaRelayRoomStatus)
  # Documents POST /api/rooms/{room_id}/media-relay/stop; REST fetch remains the runtime transport in this increment.
  - StopMediaRelay(roomID: string, userID: string) => (mediaRelay: MediaRelayRoomStatus)
  # Documents POST /api/rooms/{room_id}/media-relay/tracks; REST fetch remains the runtime transport in this increment.
  - RegisterMediaTrack(roomID: string, userID: string, trackID: string, kind: MediaTrackKind) => (mediaRelay: MediaRelayRoomStatus)
```

## Tests

Rust tests must cover:

- default media relay room status is inactive
- starting relay marks active and stores default/provided noise config
- registering a track fails while inactive
- registering audio track while active records participant and track metadata
- stopping relay clears participants/tracks and returns inactive
- malformed room IDs return client errors on REST endpoints
- REST endpoints return expected JSON field names
- WebRPC generation changes only expected generated files

Full verification after implementation:

- targeted core/web tests
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`
- `cd frontend && npm run generate:webrpc`
- `cd frontend && npm test -- --run && npm run typecheck && npm run lint && npm run build`

## Acceptance Criteria

- A room has queryable media relay status through REST.
- Relay status can be started/stopped without affecting existing join/leave/signalling behavior.
- Track registration is impossible before relay start and visible after relay start.
- Generated WebRPC TypeScript includes the new media relay DTO/service definitions.
- Documentation states this is a skeleton and does not yet process or broadcast server-side audio.
- Roadmap moves media relay skeleton to Completed while keeping real WebRTC media termination, RNNoise, and DeepFilterNet in Next.
