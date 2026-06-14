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
- Server media WebRTC offer/answer negotiation boundary.
- Server media ICE candidate exchange boundary.
- Server audio RTP ingress boundary with Opus receive negotiation, remote track snapshots, and internal RTP packet capture.
- Decoded incoming Opus RTP into 48 kHz mono PCM frames and fed them into the existing server media runtime.
- Server-side RNNoise processing for real decoded 20 ms Opus PCM frames.
- Automatic server-media draining and processing for negotiated WebRTC tracks.
- Internal WebRTC egress path that encodes processed server audio to Opus RTP and writes it to recipient server-media peers.
- Docker packaging targets for `lyre-api` and `lyre-web`.
- GitHub Actions workflow for publishing both images to GHCR.

## Next

- Wire DeepFilterNet provider to real decoded WebRTC tracks.
- Add jitter buffering and packet loss concealment for server media ingress.
- Switch frontend media flow to server-media mode and verify browser playback of processed audio.
- Add DeepFilterNet binding and processing implementation.
- Add optional client-side noise cancellation using Rust compiled to WebAssembly.
- Add authentication and room access control.
- Add persistent room/user/session state.
- Add production observability and metrics.
- Integrate a generated WebRPC Rust server/runtime path.
