# Media Topology Boundary Design

## Goal

Make Lyre's current media topology explicit in code and documentation so TURN relay work and server-side noise cancellation do not get conflated.

## Context

Lyre currently uses browser peer-to-peer WebRTC audio. The Rust server owns REST room state and WebSocket signalling, but it does not terminate WebRTC media or decode audio frames.

`turn-rs` can be used later as a Rust TURN/STUN relay candidate for NAT traversal. A TURN relay forwards WebRTC packets; it is not a media processor. Because browser WebRTC media is encrypted end-to-end between peers at the DTLS-SRTP layer, a TURN relay cannot run RNNoise or DeepFilterNet on decoded audio samples.

Server-side noise cancellation requires a different media topology:

- clients send media to a Lyre media relay/SFU-like service,
- the server terminates WebRTC media,
- the server decodes audio to PCM,
- `lyre-noise-cancelling` processes PCM,
- the server re-encodes and broadcasts processed audio to room clients.

## Scope

In scope:

- Add a core media topology model that represents the current runtime shape:
  - mode: `p2p_mesh`
  - TURN relay support: `true`
  - server-side audio processing: `false`
  - server-side noise cancellation: `false`
  - required topology for server-side noise cancellation: `media_relay`
- Add `GET /api/webrtc/topology` to the Axum API.
- Add the topology DTO and method to `proto/lyre.ridl`, regenerate `frontend/src/lib/lyre.gen.ts`, and keep the generated file committed.
- Add frontend API helper `getMediaTopology()`.
- Add backend and frontend tests for the topology contract.
- Update README, MEMORY, and roadmap to record that `turn-rs` is a TURN relay candidate, not the server-side noise processing layer.

Out of scope:

- Adding the `turn-rs` dependency.
- Running an embedded TURN server.
- Generating dynamic TURN credentials.
- Implementing media relay/SFU behavior.
- Implementing RNNoise or DeepFilterNet processing.
- Changing existing peer-to-peer WebRTC signalling or room join behavior.

## API Contract

REST endpoint:

```text
GET /api/webrtc/topology
```

Response:

```json
{
  "mode": "p2p_mesh",
  "turn_relay_supported": true,
  "server_side_audio_processing": false,
  "server_side_noise_cancelling": false,
  "server_noise_cancelling_requires": "media_relay"
}
```

The response is static for now. It describes the active architecture, not operator preference.

## WebRPC Contract

Update `proto/lyre.ridl`:

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

service Lyre
  # Documents GET /api/webrtc/topology; REST fetch remains the runtime transport in this increment.
  - GetMediaTopology() => (topology: MediaTopology)
```

Frontend helper types must preserve the REST snake_case response shape and map generated enum values to REST strings where needed.

## Testing

- Rust core test verifies the default topology values.
- Axum route test verifies `GET /api/webrtc/topology` returns the JSON contract above.
- Frontend API test verifies `getMediaTopology()` calls `/api/webrtc/topology`.
- Frontend typecheck verifies the generated WebRPC topology DTO is imported and mapped.
- Generated WebRPC client reproducibility is checked with `npm run generate:webrpc` and `git diff --exit-code -- src/lib/lyre.gen.ts`.

## Acceptance Criteria

- The API exposes the current media topology boundary.
- Documentation clearly states `turn-rs` can help NAT traversal but cannot implement server-side noise cancellation by itself.
- Roadmap keeps embedded TURN and media relay/server-side noise cancellation as separate future work.
- Existing REST, WebSocket, frontend room, and ICE server behavior remain unchanged.
