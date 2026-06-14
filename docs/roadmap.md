# Roadmap

## Completed

- Rust workspace with `lyre-core`, `lyre-noise-cancelling`, `lyre-web`, and `lyre-app`.
- In-memory room registry with default room, auto-created valid rooms, user profiles, and noise settings.
- Axum REST API for health, room snapshot, join, leave, and noise providers.
- WebSocket signalling contract for WebRTC offer, answer, ICE candidate, presence, snapshots, and errors.
- Clap CLI for serving the API and printing default config.
- Next.js frontend with room entry, shareable room route, settings, local storage, and user-triggered audio connection.
- Frontend noise settings for provider, intensity, and voice activity threshold.
- Static STUN/TURN ICE server configuration exposed to the browser WebRTC flow.
- Short-lived TURN REST credential generation for configured TURN/TURNS ICE servers.
- Formal WebRPC RIDL contract and generated TypeScript client/types for frontend-consumed HTTP DTOs.
- Media topology boundary API documenting current P2P mesh behavior, TURN relay support, and server-side noise cancellation requirements.
- Docker packaging targets for `lyre-api` and `lyre-web`.
- GitHub Actions workflow for publishing both images to GHCR.

## Next

- Harden real WebRTC mesh negotiation across multiple browsers.
- Add embedded `turn-rs` TURN service evaluation.
- Implement media relay/SFU-like server-side audio pipeline and broadcast architecture.
- Add RNNoise binding and processing implementation.
- Add DeepFilterNet binding and processing implementation.
- Add authentication and room access control.
- Add persistent room/user/session state.
- Add production observability and metrics.
- Integrate a generated WebRPC Rust server/runtime path.
