# Lyre

Lyre is a Rust and Next.js VOIP room application. This MVP provides room state, REST APIs, WebSocket signalling for WebRTC, a CLI server, noise-cancellation configuration, and a shareable frontend room route.

## Backend

```bash
cargo run -p lyre-app -- serve --host 0.0.0.0 --port 8080
cargo run -p lyre-app -- config print
```

API routes:

- `GET /health`
- `GET /api/rooms/:room_id`
- `POST /api/rooms/:room_id/join`
- `POST /api/rooms/:room_id/leave`
- `GET /api/noise/providers`
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

This milestone uses peer-to-peer WebRTC signalling only. Server-side audio decode, RNNoise inference, DeepFilterNet inference, TURN/STUN configuration, authentication, persistence, horizontal scaling, and generated WebRPC IDL are follow-up work.
