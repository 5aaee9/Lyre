# Lyre

Lyre is a Rust and Next.js VOIP room application. This MVP provides room state, REST APIs, WebSocket signalling for WebRTC, a CLI server, noise-cancellation configuration, and a shareable frontend room route.

## Backend

```bash
cargo run -p lyre-app -- serve --host 0.0.0.0 --port 8080
cargo run -p lyre-app -- serve --ice-server 'stun:stun.l.google.com:19302'
cargo run -p lyre-app -- serve --ice-server 'turn:turn.example:3478|user|pass'
cargo run -p lyre-app -- serve --ice-server 'turn:turn.example:3478' --turn-rest-secret 'shared-secret'
cargo run -p lyre-app -- serve --embedded-turn --turn-rest-secret 'shared-secret'
cargo run -p lyre-app -- config print
```

`LYRE_ICE_SERVERS` accepts a semicolon-separated list using `url[,url...][|username|credential]`.
Configured TURN usernames and credentials are returned to browsers by `/api/webrtc/ice-servers`; use scoped, rotated, low-lifetime TURN credentials rather than privileged long-lived secrets.

TURN REST credentials can be generated for configured `turn:` and `turns:` ICE servers with `--turn-rest-secret` or `LYRE_TURN_REST_SECRET`. Optional settings are `--turn-rest-ttl-seconds` / `LYRE_TURN_REST_TTL_SECONDS` and `--turn-rest-identity` / `LYRE_TURN_REST_IDENTITY`. The shared secret is never returned to browsers; the endpoint returns only short-lived usernames and HMAC-SHA1 credentials using the existing ICE server response shape, so `proto/lyre.ridl` does not need a separate schema change for this behavior.

An embedded UDP TURN relay can be enabled with `--embedded-turn` or `LYRE_EMBEDDED_TURN=true`. It requires `--turn-rest-secret` / `LYRE_TURN_REST_SECRET`, listens on `--embedded-turn-listen` / `LYRE_EMBEDDED_TURN_LISTEN` (`0.0.0.0:3478` by default), and advertises `--embedded-turn-external` / `LYRE_EMBEDDED_TURN_EXTERNAL` (`127.0.0.1:3478` by default). `--embedded-turn-external` must be an IP socket address, not a hostname. Optional settings are `--embedded-turn-realm` / `LYRE_EMBEDDED_TURN_REALM` and `--embedded-turn-port-range` / `LYRE_EMBEDDED_TURN_PORT_RANGE` using inclusive `<start>..<end>` syntax within `49152..65535`.

When embedded TURN is enabled and no `--ice-server` / `LYRE_ICE_SERVERS` is configured, Lyre advertises `turn:<embedded-turn-external>` through `/api/webrtc/ice-servers`. Explicit ICE server configuration disables this auto-injection. The embedded TURN runtime uses the MIT `turn-server` crate from the `turn-rs` project; that relay validates the HMAC credential but does not enforce the timestamp embedded in TURN REST usernames, so keep TURN credential TTL short.

API routes:

- `GET /health`
- `GET /api/rooms/:room_id`
- `POST /api/rooms/:room_id/join`
- `POST /api/rooms/:room_id/leave`
- `GET /api/noise/providers`
- `GET /api/webrtc/ice-servers`
- `GET /api/webrtc/topology`
- `GET /api/rooms/:room_id/media-relay`
- `POST /api/rooms/:room_id/media-relay/start`
- `POST /api/rooms/:room_id/media-relay/stop`
- `POST /api/rooms/:room_id/media-relay/tracks`
- `GET /api/rooms/:room_id/ws?user_id=...`

## Frontend

```bash
cd frontend
npm install
APP_BASE_URL=http://localhost:3000 APP_API_URL=http://localhost:8080 npm run dev
```

Routes:

- `/`
- `/room/[roomId]`
- `/settings`

The settings page stores nickname plus noise-cancellation provider, intensity, and voice activity threshold in local browser storage. The join flow sends those values to the API with the room join request.

The room page keeps microphone access behind the `Connect audio` button. After audio starts, the frontend reuses one local audio stream and creates one browser `RTCPeerConnection` per remote room user, targeting WebRTC offer, answer, and ICE messages with `recipient_id`.

## WebRPC Contract

The formal WebRPC schema lives at `proto/lyre.ridl`. The committed generated TypeScript client/types live at `frontend/src/lib/lyre.gen.ts`.

To regenerate the client:

```bash
cd frontend
npm run generate:webrpc
```

This uses `go run github.com/webrpc/webrpc/cmd/webrpc-gen@v0.36.0`; the first run needs network access for Go module download. The current runtime still uses the Axum REST routes, with WebRPC acting as the checked-in contract and generated TypeScript type source.

## Media Topology

`GET /api/webrtc/topology` reports the active media topology. The current topology is peer-to-peer mesh WebRTC with TURN relay support for NAT traversal.

TURN, including the embedded TURN relay, relays encrypted WebRTC packets and cannot run server-side RNNoise or DeepFilterNet by itself. Server-side noise cancellation requires a future media relay/SFU-like path that terminates WebRTC media, decodes audio to PCM, runs `lyre-noise-cancelling`, then re-encodes and broadcasts processed audio.

The media relay REST endpoints expose the initial room-scoped state skeleton for that future path. `GET /api/rooms/:room_id/media-relay` reports whether the relay is active, the intended noise config, and registered participant tracks. `POST /start` activates the room relay and records an optional noise config, `POST /tracks` registers track metadata while active, and `POST /stop` deactivates the relay and clears tracks. This skeleton does not terminate browser WebRTC media, decode audio, run RNNoise/DeepFilterNet, or broadcast processed audio yet.

`lyre-core` also defines a decoded-PCM media runtime boundary for the future server relay. It accepts already-decoded audio frames, requires an active relay and a registered audio track without mutating relay state, runs an `AudioFrameProcessor`, and publishes processed PCM to an internal `ProcessedAudioSink`. This boundary still does not terminate WebRTC, decode or encode Opus, perform real room broadcast, or run concrete RNNoise/DeepFilterNet implementations.

`lyre-noise-cancelling` can now run RNNoise-compatible processing for decoded 48 kHz mono PCM frames of 480 samples using `nnnoiseless`. RNNoise returns voice activity detection metadata, but Lyre's `intensity` and `voice_activity_threshold` settings do not alter or suppress output yet. DeepFilterNet remains a planned runtime backend and direct factory creation reports it as unsupported until real model loading/inference is added. This still does not terminate browser WebRTC media, decode/encode Opus, or broadcast processed audio.

If client-side noise cancellation is added before server-side media relay processing, it should be implemented as Rust compiled to WebAssembly rather than a JavaScript DSP path.

## Tests

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
cd frontend
npm test -- --run
npm run typecheck
npm run lint
```

## Docker

```bash
docker build --target api -t lyre-api:local .
docker build --target web -t lyre-web:local .
```

The `lyre-api` image serves REST/WebSocket on port `8080`. The `lyre-web` image serves Next.js on port `3000` and expects `APP_BASE_URL` plus `APP_API_URL`; the Next server injects those values into browser runtime config.

## MVP Scope

This milestone uses peer-to-peer WebRTC signalling with optional TURN relay. Server-side audio decode, RNNoise inference, DeepFilterNet inference, authentication, persistence, horizontal scaling, and generated WebRPC Rust server integration are follow-up work.
