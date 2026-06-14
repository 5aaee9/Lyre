# ICE Server Configuration Design

## Goal

Add configurable ICE server support so Lyre can use STUN and TURN servers during browser WebRTC negotiation. This moves the current peer-to-peer MVP closer to real VOIP operation without implementing server-side media forwarding.

## Scope

Implement a typed ICE server configuration path across backend, CLI, REST API, frontend runtime, tests, and docs.

In scope:

- Backend model for `RTCIceServer`-compatible config.
- CLI/env parsing for ICE servers.
- REST route returning ICE configuration to frontend clients.
- Frontend fetches ICE config and passes it into `RTCPeerConnection`.
- Tests for parsing, route output, frontend API serialization, and peer connection construction.
- README, MEMORY, and roadmap updates.

Out of scope:

- Provisioning TURN credentials dynamically.
- Validating credentials against a real TURN server.
- Server-side audio forwarding.
- Authentication or room access policy.

## Backend Contract

Add `IceServerConfig` to `lyre-core`:

```json
{
  "urls": ["stun:stun.l.google.com:19302"],
  "username": null,
  "credential": null
}
```

Rules:

- `urls` must contain at least one nonblank URL when parsed at a configuration boundary.
- `username` and `credential` are optional and preserved as given.
- Default config contains one public STUN server: `stun:stun.l.google.com:19302`.

CLI/env:

- `lyre serve` accepts repeated `--ice-server` values.
- `LYRE_ICE_SERVERS` accepts a semicolon-separated list.
- Each ICE server entry format is `url[,url...][|username|credential]`.
- Invalid entries return an error with the invalid value preserved. Do not silently skip bad config.
- Explicit `--ice-server` values take precedence over `LYRE_ICE_SERVERS`; when neither is present, use the default STUN server.

Propagation:

- `ServeArgs` parses effective ICE servers before starting the runtime.
- `lyre-app` passes the parsed vector into `lyre_web::ServeConfig`.
- `ServeConfig` stores `ice_servers: Vec<IceServerConfig>`.
- `lyre_web::serve` creates `AppState` with that exact vector.
- `GET /api/webrtc/ice-servers` returns the exact configured vector, preserving order and duplicates.

Parsing examples:

- Valid: `stun:stun.l.google.com:19302`
- Valid: `stun:a.example:3478,stun:b.example:3478`
- Valid: `turn:turn.example:3478|user|pass`
- Valid: `turn:turn.example:3478|user|` where `credential` is empty string.
- Valid: `turn:turn.example:3478||pass` where `username` is empty string.
- Invalid: `""`, `" "`, `";"` or any blank semicolon entry.
- Invalid: `stun:a.example,` or `,stun:a.example` because blank URLs inside comma lists are not allowed.
- Invalid: `turn:turn.example|user|pass|extra` because extra separators are malformed.

Duplicate URLs are preserved. Static TURN credentials are frontend-visible because this route is unauthenticated and returns data to browsers. Docs must warn operators to use scoped, low-lifetime, rotated TURN credentials and not place long-lived privileged secrets in `LYRE_ICE_SERVERS`.

REST API:

- `GET /api/webrtc/ice-servers` returns `Vec<IceServerConfig>`.
- Existing routes remain unchanged.

Config print:

- `lyre config print` includes `ice_servers`.

## Frontend Contract

API:

- `frontend/src/lib/api.ts` exposes `getIceServers(): Promise<IceServerConfig[]>`.
- TypeScript type names mirror backend JSON fields.

WebRTC:

- `createAudioPeerConnection(iceServers)` creates `new RTCPeerConnection({ iceServers })`.
- The room page fetches ICE servers before creating the peer connection.
- If fetching fails, the UI reports the failure and does not start the audio flow. It does not silently use an implicit fallback.

Tests:

- API test verifies `/api/webrtc/ice-servers` URL.
- WebRTC helper test verifies the exact `RTCPeerConnection` config.
- Room client test verifies clicking "Connect audio" fetches ICE servers before constructing the peer connection.

## Documentation

Update:

- `README.md` with `--ice-server`, `LYRE_ICE_SERVERS`, and API route.
- `MEMORY.md` with the decision to use static configured ICE servers first.
- `docs/roadmap.md` moving TURN/STUN configuration from TODO to completed, while keeping dynamic TURN credentials as future work.

## Verification

Run:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`
- `npm test -- --run`
- `npm run typecheck`
- `npm run lint`
- `npm run build`

Docker build remains useful if container DNS works, but this feature can be verified without Docker because no Dockerfile behavior changes are required.
