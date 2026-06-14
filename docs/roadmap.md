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
- Opt-in embedded UDP TURN relay using the MIT `turn-server` crate from the `turn-rs` project.
- Formal WebRPC RIDL contract and generated TypeScript client/types for frontend-consumed HTTP DTOs.
- Media topology boundary API documenting current P2P mesh behavior, TURN relay support, and server-side noise cancellation requirements.
- Media relay state skeleton, REST endpoints, WebRPC contract, and frontend API wrappers for future server-side audio processing.
- Frontend multi-peer WebRTC mesh negotiation with per-user peer connections and targeted signalling.
- Decoded-PCM server media runtime boundary with processor and sink traits.
- RNNoise-compatible decoded PCM provider runtime in `lyre-noise-cancelling`.
- Web server decoded-PCM media runtime wiring with internal processed-frame sink.
- Internal room-scoped processed-audio broadcast contract for future server media forwarding.
- Internal processed-audio egress fanout contract for future server media forwarding.
- Dependency-isolated Rust WebRTC server session boundary in `lyre-webrtc`.
- Docker packaging targets for `lyre-api` and `lyre-web`.
- GitHub Actions workflow for publishing both images to GHCR.

## Next

- Implement real WebRTC media termination/SFU-like audio pipeline and broadcast architecture.
- Wire the RNNoise provider runtime into real server media termination and broadcast.
- Broadcast processed server audio frames to clients.
- Add DeepFilterNet binding and processing implementation.
- Add optional client-side noise cancellation using Rust compiled to WebAssembly.
- Add authentication and room access control.
- Add persistent room/user/session state.
- Add production observability and metrics.
- Integrate a generated WebRPC Rust server/runtime path.
