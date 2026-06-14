# MEMORY

## 2026-06-14 Lyre MVP

- Chose peer-to-peer WebRTC for the MVP. The Rust server owns room presence and signalling, not media forwarding.
- Kept `/room/[roomId]` as a shareable Next.js dynamic route. This requires Next.js standalone output instead of static export.
- Split packaging into two Docker images: `lyre-api` for Rust REST/WebSocket and `lyre-web` for Next.js.
- Standardized frontend runtime configuration on `APP_BASE_URL` and `APP_API_URL`; the Next server injects them into `window.__LYRE_CONFIG__`, and WebSocket URLs derive from `APP_API_URL`.
- Modelled noise cancellation providers `off`, `rnnoise`, and `deepfilternet` with a passthrough processor until native integrations are added.
- Used an in-memory `DashMap` room registry for the first milestone.
- Represented the WebRPC boundary as typed JSON modules and REST/WebSocket contracts for now; generated IDL remains future work.

## 2026-06-14 ICE Server Configuration

- Added static STUN/TURN ICE server configuration through CLI `--ice-server`, `LYRE_ICE_SERVERS`, `lyre_web::ServeConfig`, and `/api/webrtc/ice-servers`.
- Preserved configured ICE server order and duplicates so operators can control browser candidate priority.
- Treated configured TURN credentials as browser-visible runtime config; long-lived privileged TURN secrets remain inappropriate for this static route.

## 2026-06-14 Noise Settings Controls

- Exposed the current noise parameter model in the frontend: provider, intensity, and voice activity threshold.
- Kept provider quick-selection on the join page while preserving the stored numeric parameters from settings.
- Real RNNoise and DeepFilterNet processing remain separate implementation work.

## 2026-06-14 WebRPC Contract

- Added `proto/lyre.ridl` as the formal API contract for frontend-consumed room, noise, and ICE server HTTP DTOs.
- Committed `frontend/src/lib/lyre.gen.ts` so normal frontend typecheck/build does not require WebRPC generator installation.
- Kept runtime calls on the existing Axum REST endpoints for this increment; generated WebRPC server/runtime integration remains future work.

## 2026-06-14 Media Topology Boundary

- Made the current media topology explicit: browser P2P mesh with TURN relay support, no server-side audio processing.
- Recorded `turn-rs` as a future TURN relay candidate, not a server-side noise cancellation mechanism.
- Server-side RNNoise/DeepFilterNet requires a future media relay that terminates WebRTC media and processes decoded PCM before broadcast.
