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
- Frontend multi-peer WebRTC mesh negotiation with per-user peer connections, targeted offer/answer/ICE signalling, and presence-driven peer add/remove cleanup.
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
- DeepFilterNet provider wiring through Rust libDF DSP frame reconstruction.
- DeepFilterNet libDF runtime configuration through CLI/env, server state, and media runtime construction.
- Automatic server-media draining and processing for negotiated WebRTC tracks.
- Bounded server-media RTP jitter buffering with duplicate/stale packet dropping and deterministic loss detection.
- Deterministic PCM packet loss concealment synthesis for server media ingress after jitter-buffer loss detection.
- Internal WebRTC egress path that encodes processed server audio to Opus RTP and writes it to recipient server-media peers.
- Frontend server-media audio mode with browser-to-server WebRTC negotiation and playback of remote processed server audio.
- Zustand-backed frontend settings store with persisted nickname, room preference, noise settings, and browser audio processing controls.
- Browser microphone capture now exposes persisted echo cancellation and auto gain control settings, both enabled by default.
- Per-user server-media cleanup endpoint and frontend Leave/startup-failure cleanup flow.
- Room-scoped access tokens for mutating room, signalling, media relay, and server-media routes.
- Optional JSON file persistence for anonymous room/user/session access state.
- Docker packaging targets for `lyre-api` and `lyre-web`.
- GitHub Actions workflow for publishing both images to GHCR.
- GitHub Actions workflow for deploying the Next.js frontend to Vercel production.
- Release and manual `lyre` Helm chart publishing for deploying `lyre-api` and `lyre-web`, with optional Ingress and Gateway API HTTPRoute entry points.
- Helm readiness and liveness health checks for both `lyre-api` and `lyre-web`.
- Aggregate Prometheus-compatible `/metrics` endpoint for process-local API observability.
- Rust WebRPC runtime routes at `POST /rpc/Lyre/<Method>` compatible with the generated TypeScript client while preserving REST routes.
- Configurable API CORS allowed origins through `serve --cors-allowed-origin` and `LYRE_CORS_ALLOWED_ORIGINS`.
- Concise README with detailed setup, configuration, API, media, development, and deployment documentation split under `docs/`.
- Nix flake packaging for the Rust API binary with crane, fenix, flake-utils, and a Rust development shell.
- GitHub Actions CI for Nix flake check, API package build, clippy, rustfmt, and Hestia-backed Nix cache maintenance.
- Clap help text for Lyre commands, subcommands, and serve options.
- Server-media WebRTC now advertises a reachable host ICE address instead of loopback-only candidates.

## Next

- Add Nix packaging for the Next.js frontend if Nix becomes a deployment target.
- Add full DeepFilterNet neural model inference/configuration for decoded WebRTC tracks.
- Add optional client-side noise cancellation using Rust compiled to WebAssembly.
- Add production Helm values for TLS, real hostnames, secrets, persistence, and scaling policy.
- Add production-grade database/session management if anonymous JSON persistence stops being sufficient.
