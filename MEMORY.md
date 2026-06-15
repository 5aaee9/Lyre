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

## 2026-06-14 TURN REST Credentials

- Added short-lived TURN REST credential generation for configured TURN/TURNS ICE servers.
- Kept STUN-only servers and default static ICE behavior unchanged when no shared secret is configured.
- Did not add `turn-rs` runtime yet; this prepares credentials for any TURN server that supports the shared-secret REST credential pattern.

## 2026-06-15 Embedded TURN Service

- Added an opt-in embedded UDP TURN relay using the MIT `turn-server` crate from the `turn-rs` project.
- Kept the GPL `turn-rs` crate out of the dependency graph; `lyre-turn` isolates the `turn-server` API.
- Embedded TURN advertises a local-only `turn:127.0.0.1:3478` URL by default and requires explicit IP socket configuration for public deployments.
- Confirmed TURN relay remains separate from server-side noise cancellation; media processing still needs a future WebRTC media relay.

## 2026-06-15 Media Relay Skeleton

- Added a room-scoped media relay state skeleton with REST endpoints and WebRPC contract coverage.
- Kept browser P2P signalling unchanged; this increment does not terminate WebRTC media or process audio.
- Recorded intended noise settings in relay state so future RNNoise/DeepFilterNet processing can attach to the media relay boundary.

## 2026-06-15 Mesh Audio Negotiation

- Replaced the single frontend peer connection with a room mesh session keyed by remote user id.
- Kept microphone capture user-triggered and reused one local audio stream across all peer connections.
- Kept backend signalling unchanged; frontend-generated offer, answer, and ICE messages are now targeted with `recipient_id`.
- Recorded that future client-side noise cancellation should use Rust compiled to WebAssembly instead of a JavaScript DSP path.

## 2026-06-15 Server Media Runtime Boundary

- Added a decoded-PCM media runtime boundary in `lyre-core`.
- Kept `lyre-core` independent of `lyre-noise-cancelling`; future adapters can bridge concrete processors behind the core `AudioFrameProcessor` trait.
- Gated frame processing on active media relay state and registered audio tracks without mutating relay state.
- Kept real WebRTC termination, Opus decode/encode, RNNoise, DeepFilterNet, and real server broadcast as future work.

## 2026-06-15 Noise Provider Runtime

- Added a fallible provider runtime in `lyre-noise-cancelling`.
- Implemented RNNoise-compatible 48 kHz mono 480-sample processing with `nnnoiseless`.
- Kept RNNoise VAD as metadata only; `intensity` and `voice_activity_threshold` are not applied to alter output yet.
- Kept DeepFilterNet explicit as unsupported until a real libDF/model integration is added.
- Added a `lyre-core::AudioFrameProcessor` adapter with structured warning logs at the current infallible trait boundary.
- Left client-side noise cancellation as future Rust WASM work.

## 2026-06-15 Web Media Runtime Wiring

- Wired `lyre-web::AppState` to own a decoded-PCM `MediaRuntime` using the shared media relay registry.
- Connected the web runtime to `lyre-noise-cancelling::NoiseCancellingAudioFrameProcessor`.
- Stored processed frames in an internal in-memory sink for tests and future broadcaster integration.
- Kept WebRTC media termination, Opus decode/encode, and client broadcast as future work.

## 2026-06-15 Processed Audio Broadcast Contract

- Replaced the web runtime's recording-only processed frame sink with a room-scoped processed audio broadcaster.
- Broadcasts are internal `tokio::sync::broadcast` receivers for future WebRTC/SFU integration; no browser playback or RTP forwarding is implemented yet.
- Stopping a media relay clears retained processed frame history for that room.

## 2026-06-15 Processed Audio Egress Fanout

- Added an internal processed-audio egress fanout contract that maps processed source frames to other audio-capable relay participants.
- Egress fanout validates the source track against current relay state and returns relay errors for stale or non-audio source frames.
- Split `lyre-core` media tests out of `media.rs` while adding the read-only active participant snapshot needed by fanout.
- Kept real WebRTC media termination, RTP/Opus packetization, and browser delivery as future work.

## 2026-06-15 WebRTC Session Boundary

- Added `lyre-webrtc` to isolate the direct `webrtc` crate dependency behind Lyre-owned server media session types.
- Chose `webrtc = "0.20.0-alpha.1"` over `str0m` for this boundary because its high-level PeerConnection model better matches the existing browser-style signalling path.
- Server media sessions are control-plane state only; real browser-to-server negotiation, RTP/RTCP, Opus decode/encode, RNNoise ingestion, and playback remain future work.

## 2026-06-15 Server Media Negotiation Boundary

- Added a server media offer/answer control-plane path that creates real WebRTC answers inside `lyre-webrtc`.
- Kept negotiation atomic: failed offers do not create sessions or replace stored peer handles.
- Stored peer connection handles only to keep negotiated sessions alive for later media work; RTP/RTCP, Opus, RNNoise ingestion, and browser playback remain future work.

## 2026-06-15 Server Media ICE Candidate Exchange

- Added server media ICE candidate add/query REST boundaries for negotiated server peer connections.
- Kept candidate conversion and direct `webrtc` ICE types isolated inside `lyre-webrtc`.
- Server media ICE exchange is still control-plane only; RTP/RTCP, Opus, RNNoise ingestion, and browser playback remain future work.

## 2026-06-15 Server Audio RTP Ingress

- Added the first server-side WebRTC audio RTP ingress boundary in `lyre-webrtc`.
- Server peer connections now register Opus payload type 111, add a recvonly audio transceiver, record remote track metadata, and retain incoming audio RTP packet snapshots behind Lyre-owned DTOs.
- Kept direct `webrtc` and `rtc` media/RTP types isolated inside `lyre-webrtc`; `lyre-web` only exposes internal AppState snapshot methods for tests and future runtime wiring.
- Did not add a public raw RTP REST endpoint.
- Opus decode, decoded PCM conversion, RNNoise/DeepFilterNet ingestion from real tracks, processed audio broadcast, RTP/RTCP forwarding, and browser playback remain future work.

## 2026-06-15 Opus RTP to Media Runtime

- Added a pure-Rust Opus decode bridge in `lyre-webrtc` using `opus-rs`.
- Valid incoming server-media Opus RTP packets now produce Lyre-owned 48 kHz mono PCM frame DTOs that `lyre-web` can drain into the existing `WebMediaRuntime`.
- Decode failures preserve the original decoder error message in internal snapshots; no public raw RTP, PCM, or decode-failure endpoint was added.
- Packet loss concealment, jitter buffering, processed RTP/RTCP egress, browser playback, and DeepFilterNet remain future work.

## 2026-06-15 RNNoise Opus Frame Alignment

- Updated server-side RNNoise to process real decoded 20 ms Opus PCM frames by chunking 960-sample input into two 480-sample RNNoise frames.
- Kept the public noise-cancelling API unchanged and split `lyre-noise-cancelling` tests out of `lib.rs` to keep Rust files below 400 LOC.
- Verified real server-media decoded PCM can be processed through RNNoise when the media relay is configured for RNNoise.
- DeepFilterNet, automatic server-media pumping, jitter buffering, processed RTP/RTCP egress, and browser playback remain future work.

## 2026-06-15 Server Media Runtime Pump

- Added an internal `lyre-web` runtime pump that starts after successful server-media negotiation and automatically drains decoded PCM into `WebMediaRuntime`.
- Pump tasks are keyed by room/user server-media session, replaced on renegotiation, and cancelled when server-media sessions or media relays are stopped for a room.
- The pump keeps polling through inactive relay or missing track errors so relay/track registration can arrive after negotiation.
- No public pump, raw RTP, decoded PCM, decode-failure, or debug endpoint was added; RTP/RTCP egress and browser playback remain future work.

## 2026-06-15 Processed Audio WebRTC Egress

- Added an internal server-to-client Opus RTP egress path for processed audio frames on negotiated server-media peer connections.
- `lyre-webrtc` now owns a local Opus audio track per server-media peer and keeps direct WebRTC/RTP types behind Lyre-owned processed-frame and egress-packet DTOs.
- `lyre-web` starts a room egress pump with media relay activation, fans processed frames out to audio-capable recipients, and sends them through recipient server-media peer handles.
- No public egress pump, RTP packet, decoded PCM, encode-failure, or debug endpoint was added; frontend server-media mode, browser playback verification, jitter/PLC, and DeepFilterNet remain future work.

## 2026-06-15 Frontend Server Media Playback

- Switched the room page default audio path to server relay mode while keeping peer mesh as an explicit compatibility option.
- Added a frontend server-media WebRTC session that negotiates through existing REST endpoints, exchanges ICE candidates, and attaches remote processed audio to a hidden browser audio element.
- Server relay playback is remote-participant audio only; the current server fanout excludes self-loopback.
- Leave and unmount clean up local browser media resources but do not call the room-level `stopMediaRelay`, because that endpoint stops the whole room until a per-user server-media cleanup API exists.
- Server-media REST wrappers now throw visible errors for non-2xx responses so relay start, track registration, offer negotiation, and ICE candidate failures remain visible in room status.

## 2026-06-15 Per-User Server Media Cleanup

- Added a per-user server-media close path that stops only the matching runtime pump, closes only the matching server-media peer/session, and removes only that user's media relay participant tracks.
- Kept the room media relay and room egress pump active during per-user cleanup; room-level `stopMediaRelay` remains the separate whole-room shutdown path.
- Frontend server relay Leave now closes local media, calls the per-user cleanup endpoint, then leaves room presence.
- Server relay startup failures after relay creation call the cleanup endpoint while preserving the original startup error if cleanup itself fails.
- Component unmount remains local-only and does not call server mutation endpoints.
- The WebRPC payload struct is named `ClosedServerMediaSession` to avoid a generated TypeScript name collision with the service method response wrapper; the REST wrapper still exposes `CloseServerMediaSessionResponse`.

## 2026-06-15 DeepFilterNet libDF Runtime

- Wired `NoiseProvider::Deepfilternet` to a Rust libDF DSP runtime using the `deep_filter` package's `df::DFState`.
- The runtime keeps one persistent `DFState` per noise config and processes 48 kHz mono PCM in 480-sample chunks through `DFState::process_frame`.
- This is STFT/ISTFT frame reconstruction and provider plumbing only; it does not include pretrained DeepFilterNet neural model inference, post-filtering, model configuration, or proven noise attenuation.
- Full DeepFilterNet model inference remains future work.

## 2026-06-15 Server Media Jitter Buffer

- Added a bounded server-media RTP jitter buffer in `lyre-webrtc` before Opus decode.
- The buffer reorders audio RTP by 16-bit sequence number, drops duplicate/stale packets, and emits deterministic loss events after a depth of three pending packets.
- Loss events are recorded as internal decode failures with expected RTP timestamps; Lyre still does not synthesize packet loss concealment PCM.
- Real PCM packet loss concealment remains future work unless the Opus decoder path exposes PLC/FEC or Lyre adds a dedicated concealment synthesizer.

## 2026-06-15 Server Media PCM PLC

- Added deterministic Lyre-owned PCM packet loss concealment for server-media ingress.
- Missing RTP packets after a decoded baseline now produce 48 kHz mono 960-sample synthetic PCM fallback frames using a faded copy of the previous frame.
- This is not Opus-native PLC or FEC; missing packets before a usable baseline still record an internal decode failure.

## 2026-06-15 Room Access Tokens

- `join` now returns a room-scoped opaque access token stored only in server-private room state.
- Mutating room, signalling, media relay, and server-media routes validate bearer tokens; public discovery routes remain unauthenticated.
- WebSocket signalling uses an `access_token` query parameter because browser WebSockets cannot set `Authorization`; request tracing records only redacted paths.
- TURN remains NAT traversal only. Server-side denoise still requires the server-media decode/process/broadcast path. Future client-side denoise should use Rust WASM.

## 2026-06-15 Room State Persistence

- Added optional JSON file persistence for anonymous room users and access tokens via `--state-file` / `LYRE_STATE_FILE`.
- Persisted state is limited to the room registry. WebSocket peer handles, WebRTC sessions, relay pumps, processed audio buffers, TURN state, and media runtime state remain process-local.
- Persisted join/leave mutations are serialized and roll back in-memory registry state if the state file write fails, so failed leaves do not resurrect tokens after restart.

## 2026-06-15 Production Metrics

- Added a Prometheus-compatible `/metrics` endpoint to the Rust API server.
- Kept metrics aggregate and process-local: no room IDs, user IDs, access tokens, nicknames, SDP, ICE candidates, RTP payloads, or persistence paths appear in metrics output.
- Used read-only registry aggregate snapshots so scraping metrics does not create room or media relay state.
- Counted joins/leaves only after successful in-memory or persisted mutations; failed persistence writes increment a separate process-local counter after rollback.

## 2026-06-15 WebRPC Rust Runtime

- Added Axum WebRPC runtime routes at `POST /rpc/Lyre/<Method>` aligned with `proto/lyre.ridl` and the committed generated TypeScript client.
- Kept the existing REST and WebSocket routes stable; the frontend helper layer can continue using REST while generated WebRPC clients can call the Rust API directly.
- WebRPC public error envelopes are sanitized and do not expose access tokens, SDP bodies, ICE candidate strings, RTP/media payloads, persistence paths, or lower-level cause chains.
- Promoted `chrono` to a normal `lyre-web` dependency because WebRPC DTOs expose joined timestamps from non-test library code.
