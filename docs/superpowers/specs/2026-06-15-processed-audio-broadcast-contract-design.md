# Processed Audio Broadcast Contract Design

## Scope

This increment adds an internal processed-audio broadcast contract for server-side media relay output. It does not implement WebRTC media termination, Opus encode/decode, RTP forwarding, browser playback of server media, or a public PCM upload API.

## Problem

`lyre-web::AppState::process_media_frame` can process decoded PCM frames through the web media runtime, but processed frames are only stored in an internal test sink. The next media-relay layer needs a small, testable contract that lets connected server-side consumers subscribe to processed frames for a room without coupling the core runtime to Axum, WebSockets, or future WebRTC implementation details.

## Design

Add a cloneable room-scoped processed-audio broadcaster in `crates/lyre-web/src/media_runtime.rs`.

The broadcaster will:

- keep the existing in-memory processed frame history by room for tests and diagnostics,
- publish new processed frames to a `tokio::sync::broadcast` channel keyed by `RoomId`,
- expose `subscribe(room_id)` so future WebRTC/SFU code can receive frames after subscription,
- expose `clear_room(room_id)` so relay stop can drop retained history and the room broadcaster,
- remain internal to `lyre-web`; `lyre-core` keeps only the processor/sink traits.

Broadcast channels will use a fixed capacity of 256 processed frames per room. Publishing with no subscribers is allowed and still records frame history. Slow receivers use normal Tokio broadcast semantics: if more than 256 newer frames arrive before a receiver reads, that receiver gets `RecvError::Lagged`; this increment does not add retry, replay, or backpressure. Acceptance tests only assert non-lagged single-frame delivery and room isolation.

Rename the web sink from `RecordingProcessedAudioSink` to `ProcessedAudioBroadcaster` to describe the new responsibility. `WebMediaRuntime` will use that broadcaster as its `ProcessedAudioSink`.

`AppState` will expose two internal methods:

- `subscribe_processed_media_frames(&RoomId) -> tokio::sync::broadcast::Receiver<ProcessedAudioFrame>`
- `clear_processed_media_room(&RoomId)`

`stop_media_relay` will call `clear_processed_media_room` after stopping relay state so a stopped room does not retain processed audio history for later diagnostics.

## Acceptance Criteria

- Processing a valid decoded PCM frame stores it in room history and delivers it to an active room subscriber.
- A subscriber for a different room does not receive frames from the processed room.
- A subscriber created after a frame is processed does not receive old frames through the broadcast channel, while history remains available via `processed_media_frames` until the room is cleared.
- Stopping a media relay clears processed frame history for that room and prevents future processing through the existing relay-state checks.
- Existing REST/WebSocket signaling behavior remains unchanged.
- No public endpoint or frontend behavior claims server broadcast playback yet.

## Tests

Add focused Rust tests in `crates/lyre-web/src/api_media_tests.rs` covering:

- active subscriber receives processed frames,
- cross-room subscribers are isolated,
- late subscribers only receive future broadcast frames,
- stopping the relay clears processed frame history.

Run full Rust verification and frontend verification because generated/API docs remain part of the project contract.

## Documentation

Update `MEMORY.md` and `docs/roadmap.md` after implementation. `MEMORY.md` must record that the web runtime now has an internal room-scoped processed-audio broadcaster, that it is not browser playback or RTP forwarding, and that stopping a relay clears retained processed audio history. The roadmap should add `Internal room-scoped processed-audio broadcast contract for future server media forwarding.` to Completed while keeping real WebRTC media termination and client playback as Next work.

No generated API, WebRPC, or frontend source changes are expected. Frontend verification still runs as a repo-level guard because the project requires full verification after completed increments.
