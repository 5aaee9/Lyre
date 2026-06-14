# Lyre

Lyre is a Rust and Next.js VOIP room application. This MVP provides room state, REST APIs, WebSocket signalling for WebRTC, a CLI server, noise-cancellation configuration, and a shareable frontend room route.

## Backend

```bash
cargo run -p lyre-app -- serve --host 0.0.0.0 --port 8080
cargo run -p lyre-app -- serve --ice-server 'stun:stun.l.google.com:19302'
cargo run -p lyre-app -- serve --ice-server 'turn:turn.example:3478|user|pass'
cargo run -p lyre-app -- config print
```

`LYRE_ICE_SERVERS` accepts a semicolon-separated list using `url[,url...][|username|credential]`.
Configured TURN usernames and credentials are returned to browsers by `/api/webrtc/ice-servers`; use scoped, rotated, low-lifetime TURN credentials rather than privileged long-lived secrets.

API routes:

- `GET /health`
- `GET /api/rooms/:room_id`
- `POST /api/rooms/:room_id/join`
- `POST /api/rooms/:room_id/leave`
- `GET /api/noise/providers`
- `GET /api/webrtc/ice-servers`
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

## WebRPC Contract

The formal WebRPC schema lives at `proto/lyre.ridl`. The committed generated TypeScript client/types live at `frontend/src/lib/lyre.gen.ts`.

To regenerate the client:

```bash
cd frontend
npm run generate:webrpc
```

This uses `go run github.com/webrpc/webrpc/cmd/webrpc-gen@v0.36.0`; the first run needs network access for Go module download. The current runtime still uses the Axum REST routes, with WebRPC acting as the checked-in contract and generated TypeScript type source.

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

This milestone uses peer-to-peer WebRTC signalling only. Server-side audio decode, RNNoise inference, DeepFilterNet inference, dynamic TURN credentials, authentication, persistence, horizontal scaling, and generated WebRPC Rust server integration are follow-up work.
