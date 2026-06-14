# Web Media Runtime Wiring Design

## Goal

Wire the existing `lyre-core::MediaRuntime` and `lyre-noise-cancelling::NoiseCancellingAudioFrameProcessor` into `lyre-web::AppState` so the Axum server owns a tested server-side decoded-PCM processing path for active media relay rooms.

## Context

Lyre currently has:

- room and signalling APIs in `lyre-web`;
- a `MediaRelayRegistry` in `AppState`;
- a `lyre-core::MediaRuntime` that validates active relay state and registered audio tracks before processing decoded PCM;
- a `lyre-noise-cancelling` adapter that can run RNNoise-compatible processing for valid 48 kHz mono 480-sample PCM frames;
- no web-layer owner for the runtime or processed frame sink.

This increment should connect those pieces at the server boundary without claiming browser WebRTC media is terminated yet.

## Scope

### In Scope

- Add `lyre-noise-cancelling` as a dependency of `lyre-web`.
- Add a focused `lyre-web` module that owns:
  - a processed-frame sink implementation;
  - a `WebMediaRuntime` wrapper that owns `MediaRuntime<NoiseCancellingAudioFrameProcessor, RecordingProcessedAudioSink>`;
  - read-only test helpers for inspecting processed frames.
- Store the runtime in `AppState` and construct it from the same `Arc<MediaRelayRegistry>` used by media relay REST endpoints.
- Expose an internal `AppState::process_media_frame(frame: AudioFrame) -> Result<(), MediaRelayError>` method for future WebRTC termination code.
- Ensure processed frames are recorded by room and can be inspected in tests.
- Update existing media relay status behavior only if needed to reflect that a server processing runtime is attached. Do not mark actual browser media termination or broadcast as complete.
- Add tests proving:
  - `AppState` wires the runtime to the same media relay registry as REST state;
  - inactive relay / unknown participant / unknown track errors still propagate;
  - a registered audio track with provider `off` publishes an internal processed frame;
  - a registered audio track with provider `rnnoise` publishes a 480-sample processed frame;
  - processing an unknown room returns `MediaRelayError::Inactive` without creating room state;
  - stopping an existing active relay prevents later processing in that same room.
- Update `README.md`, `MEMORY.md`, and `docs/roadmap.md`.

### Out of Scope

- Browser WebRTC media termination.
- RTP/RTCP, Opus decode/encode, jitter buffering, packet loss handling, or SFU forwarding.
- Real client broadcast of processed audio.
- Public HTTP endpoint for uploading PCM frames.
- DeepFilterNet inference.
- Client-side WASM noise cancellation.
- Authentication, persistence, and horizontal scaling.

## Design

Add `crates/lyre-web/src/media_runtime.rs`.

The module should define a real wrapper, not a type alias:

```rust
#[derive(Debug, Clone, Default)]
pub struct RecordingProcessedAudioSink { ... }

impl RecordingProcessedAudioSink {
    pub fn frames_for_room(&self, room_id: &RoomId) -> Vec<ProcessedAudioFrame>;
    pub fn clear_room(&self, room_id: &RoomId);
}

pub struct WebMediaRuntime {
    runtime: MediaRuntime<
        lyre_noise_cancelling::NoiseCancellingAudioFrameProcessor,
        RecordingProcessedAudioSink,
    >,
    sink: RecordingProcessedAudioSink,
}

impl WebMediaRuntime {
    pub fn new(relays: Arc<MediaRelayRegistry>) -> Self;
    pub fn process_frame(&self, frame: AudioFrame) -> Result<(), MediaRelayError>;
    pub fn frames_for_room(&self, room_id: &RoomId) -> Vec<ProcessedAudioFrame>;
}
```

`RecordingProcessedAudioSink` should be cloneable by wrapping `Arc<DashMap<RoomId, Vec<ProcessedAudioFrame>>>`. `WebMediaRuntime::new` creates one sink, passes a clone into `MediaRuntime::new`, and keeps the original sink handle for `frames_for_room`. This avoids reaching into private `MediaRuntime` fields.

`WebMediaRuntime` should implement `Debug` manually and omit `runtime` internals, for example by exposing only a non-exhaustive struct name. This keeps the existing `AppState: Debug` derive compiling without requiring `NoiseCancellingAudioFrameProcessor` or `MediaRuntime` internals to implement `Debug`.

The sink stores processed frames in memory using `DashMap<RoomId, Vec<ProcessedAudioFrame>>` protected by the map entry guard. This is an internal development/runtime boundary, not a durable media store. A future real broadcaster can replace this sink behind the same `ProcessedAudioSink` trait.

`AppState` should gain:

```rust
pub media_runtime: Arc<WebMediaRuntime>,

pub fn process_media_frame(&self, frame: AudioFrame) -> Result<(), MediaRelayError>;
pub fn processed_media_frames(&self, room_id: &RoomId) -> Vec<ProcessedAudioFrame>;
```

`AppState::new` should create one `Arc<MediaRelayRegistry>` first, then pass the same `Arc` to both `media_relays` and `WebMediaRuntime::new(...)`.

## Error Handling

`process_media_frame` should return the exact `MediaRelayError` from `lyre-core::MediaRuntime::process_frame`. It must not translate relay/runtime errors into generic API errors.

The RNNoise adapter remains infallible at the core trait boundary by design. Unsupported or invalid noise frames are observable through structured warnings from `lyre-noise-cancelling`, and this increment should not add a second fallback layer in `lyre-web`.

## Documentation

Update docs to state:

- `lyre-web::AppState` now owns an internal decoded-PCM media runtime connected to media relay state.
- Processed frames are currently stored in an internal sink for testing/future broadcaster integration.
- The server still does not terminate browser WebRTC media, decode/encode Opus, or broadcast processed audio to clients.
- RNNoise processing is available only once decoded 48 kHz mono 480-sample PCM frames are supplied to this runtime.

## Acceptance Criteria

- `lyre-web` depends on `lyre-noise-cancelling` and constructs a `MediaRuntime` with `NoiseCancellingAudioFrameProcessor`.
- `AppState` uses one shared `Arc<MediaRelayRegistry>` for REST media relay state and media runtime validation.
- Internal processing rejects inactive relay / missing participant / missing track with existing `MediaRelayError` variants.
- Internal processing publishes processed frames for registered audio tracks.
- RNNoise-configured active relays can process a valid 48 kHz mono 480-sample frame through the web runtime.
- Documentation and roadmap are updated without claiming completed WebRTC termination or client broadcast.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo nextest run --manifest-path "Cargo.toml" --workspace` pass.
- From `frontend/`, `npm run generate:webrpc`, `npm test -- --run`, `npm run typecheck`, `npm run lint`, and `npm run build` pass.
