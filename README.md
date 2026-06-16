# Lyre

Lyre is a high-performance VOIP room application built with Rust and Next.js.

The current MVP includes:

- Rust Axum API, WebSocket signalling, and CLI server.
- Next.js room UI with shareable room routes and persisted user settings.
- Peer-to-peer WebRTC mesh audio with optional TURN relay support.
- Optional server-media mode for server-side audio processing.
- RNNoise-compatible processing and DeepFilterNet DSP runtime wiring.
- Docker images and Helm chart for deployment.

## Quick Start

Start the API:

```bash
cargo run -p lyre-app -- serve --host 0.0.0.0 --port 8080
```

Start the frontend:

```bash
cd frontend
npm install
APP_BASE_URL=http://localhost:3000 APP_API_URL=http://localhost:8080 npm run dev
```

Open `http://localhost:3000` and create or join a room. The room connects server-relay audio automatically after joining; use `Mute` / `Unmute` to control only your local microphone.

## Common Commands

```bash
cargo run -p lyre-app -- config print
cargo run -p lyre-app -- serve --ice-server 'stun:stun.l.google.com:19302'
cargo run -p lyre-app -- serve --embedded-turn --turn-rest-secret 'shared-secret'
```

## Documentation

- [Getting started](docs/getting-started.md)
- [Runtime configuration](docs/runtime-configuration.md)
- [API and WebRPC contracts](docs/api-contracts.md)
- [Media architecture](docs/media-architecture.md)
- [Development and testing](docs/development.md)
- [Docker and deployment](docs/deployment.md)
- [Roadmap](docs/roadmap.md)

## Workspace

- `crates/lyre-core` - core room, media, and DTO logic.
- `crates/lyre-app` - CLI entry point.
- `crates/lyre-web` - Axum REST, WebSocket, WebRPC, and metrics API.
- `crates/lyre-noise-cancelling` - RNNoise and DeepFilterNet processing boundaries.
- `crates/lyre-turn` - optional embedded UDP TURN relay adapter.
- `crates/lyre-webrtc` - dependency-isolated server WebRTC boundary.
- `frontend/` - Next.js frontend.

## MVP Scope

Lyre currently supports peer-to-peer WebRTC signalling, optional TURN relay, and an optional server-media path. Full DeepFilterNet neural model inference, production database/session management, horizontal scaling, and optional Rust WASM client-side denoise remain follow-up work.
