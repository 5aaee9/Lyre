# Server Media Runtime Pump Design

## Scope

This increment adds automatic server-side draining for negotiated server-media WebRTC sessions. Once a browser negotiates a server-media session, Lyre should periodically drain decoded PCM frames from the stored `lyre-webrtc` peer handle and feed them into the existing `WebMediaRuntime`.

This increment does not implement browser playback, processed RTP/RTCP egress, SFU packetization, jitter buffering, packet loss concealment, DeepFilterNet, or frontend switching from the current mesh path to server-media mode. It also does not expose raw RTP, decoded PCM, decode failures, or pump metrics through public HTTP endpoints.

## Problem

Lyre can now negotiate a server-side WebRTC receive path, decode valid Opus RTP into PCM, and process that PCM through RNNoise when tests call `AppState::process_server_media_pcm_frames` manually. In the running server, no component calls that method after negotiation, so decoded server-media audio can sit in the session recorder without entering the noise-cancelling runtime.

The missing piece is a small lifecycle-owned pump that connects negotiated server-media sessions to `WebMediaRuntime` automatically.

## Design

Add a `ServerMediaRuntimePump` inside `crates/lyre-web`. It owns a map of running pump tasks keyed by `ServerMediaSessionKey`.

Each pump task should:

- run after a successful server-media offer answer,
- repeatedly call the existing `server_media_runtime::process_pcm_frames` for one session,
- sleep for a short fixed interval when no frames are available,
- continue after `MediaRelayError` by logging the full error context and sleeping,
- stop when its cancellation token is triggered,
- not expose any public route or DTO.

Keep the pump simple and local to `lyre-web`; do not move it into `lyre-core` or `lyre-webrtc`.

Use `tokio-util` `CancellationToken` unless a simpler existing project dependency already provides equivalent cancellation. If `tokio-util` is added, add it as a workspace dependency and only use it where needed.

The pump owns task lifecycle, not media semantics. Existing `server_media_runtime::process_pcm_frames` remains the only function that drains and submits frames for one session. Its current drain-on-error behavior remains unchanged: if processing a drained batch fails, the failing frame and later frames from that drained batch are discarded.

### AppState Integration

`AppState` should own:

```rust
pub server_media_runtime_pump: Arc<ServerMediaRuntimePump>
```

`AppState::new` constructs the pump from the existing `media_runtime`, `server_media_negotiator`, and `server_media_sessions` dependencies.

`AppState::answer_server_media_offer` should start or replace the pump for the session key only after negotiation succeeds. Failed offers must not start a pump.

`AppState::close_server_media_sessions_for_room` and `AppState::stop_media_relay` should stop pumps for that room. Closing a room must cancel and remove the corresponding pump handles before or while peer handles are removed so no task keeps polling a dead session indefinitely.

For tests, expose internal-only methods behind `#[cfg(test)]`:

```rust
pub fn server_media_runtime_pump_count(&self) -> usize;
```

The public product API should not expose pump counts or pump state.

## Error Handling

Pump tasks should log processing errors with complete context:

- room id,
- user id,
- and `{error:#}` / equivalent full error chain formatting.

The pump should not terminate on ordinary `MediaRelayError`, because relay state and track registration can arrive after negotiation. It should keep polling so a session negotiated before relay activation can start processing once the relay and track exist.

Task startup and cancellation are local runtime operations and should not change HTTP response shapes.

## Acceptance Criteria

- A successful `AppState::answer_server_media_offer` starts one pump task for that session.
- Re-answering the same room/user server-media session replaces the previous pump and leaves only one active pump for that key.
- A failed server-media offer does not start a pump.
- Stopping a media relay for a room cancels/removes that room's server-media pumps.
- Closing server-media sessions for a room cancels/removes that room's pumps.
- The pump processes real decoded server-media PCM frames into `WebMediaRuntime` without a manual `process_server_media_pcm_frames` call in the test.
- The pump tolerates an initially inactive relay or missing track, then processes frames after the relay is started and the decoded track is registered.
- Pump processing preserves existing RNNoise behavior when the room relay is configured for RNNoise.
- No public REST endpoint exposes pump state, raw RTP, decoded PCM, or decode failures.
- Existing server-media negotiation, media runtime, and frontend tests continue to pass.
- Changed Rust files remain below 400 LOC. If `api_server_media_state.rs` or `server_media_runtime.rs` would exceed that limit, split pump code into a new focused module.

## Tests

Add focused Rust tests covering:

- pump start/replace/count behavior on successful and repeated offer answers,
- no pump on failed offer,
- pump cancellation on room close / media relay stop,
- automatic real decoded PCM processing into `processed_media_frames`,
- delayed relay/track registration after pump startup,
- no public pump/debug route exists.

Run:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`
- `cd frontend && npm run generate:webrpc && npm test -- --run && npm run typecheck && npm run lint && npm run build`
- `git diff --check`
- LOC checks for changed Rust files

## Documentation

Update `MEMORY.md` and `docs/roadmap.md` after implementation approval. Record that negotiated server-media sessions now automatically drain decoded PCM into the server media runtime, while DeepFilterNet, jitter buffering, processed RTP/RTCP egress, browser playback, and frontend server-media mode remain future work.
