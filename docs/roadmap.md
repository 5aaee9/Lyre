# Roadmap

## Completed

- Rust workspace with `lyre-core`, `lyre-noise-cancelling`, `lyre-web`, and `lyre-app`.
- In-memory room registry with default room, auto-created valid rooms, user profiles, and noise settings.
- Axum REST API for health, room snapshot, join, leave, and noise providers.
- WebSocket signalling contract for WebRTC offer, answer, ICE candidate, presence, snapshots, and errors.
- Clap CLI for serving the API and printing default config.
- Next.js frontend with room entry, shareable room route, settings, local storage, and user-triggered audio connection.
- Docker packaging targets for `lyre-api` and `lyre-web`.
- GitHub Actions workflow for publishing both images to GHCR.

## Next

- Harden real WebRTC mesh negotiation across multiple browsers.
- Add TURN/STUN configuration and deployment documentation.
- Implement server-side audio pipeline and broadcast architecture.
- Add RNNoise binding and processing implementation.
- Add DeepFilterNet binding and processing implementation.
- Add authentication and room access control.
- Add persistent room/user/session state.
- Add production observability and metrics.
- Generate a formal WebRPC IDL and client bindings.
