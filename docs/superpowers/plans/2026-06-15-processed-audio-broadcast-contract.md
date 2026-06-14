# Processed Audio Broadcast Contract Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an internal room-scoped broadcast contract for processed server audio frames without implementing real WebRTC media termination or browser playback.

**Architecture:** `lyre-core` keeps the existing processor and sink traits. `lyre-web` replaces the recording-only sink with a cloneable `ProcessedAudioBroadcaster` that stores processed frame history and publishes new frames through fixed-capacity `tokio::sync::broadcast` channels keyed by `RoomId`. `AppState` exposes internal subscription and cleanup methods, and relay stop clears retained processed audio for that room.

**Tech Stack:** Rust, `lyre-core` media runtime traits, `lyre-web`, `dashmap`, `tokio::sync::broadcast`, Axum route state.

---

### Task 1: Add Processed Audio Broadcaster Tests

**Files:**

- Modify: `crates/lyre-web/src/api_media_tests.rs`

- [ ] **Step 1: Add broadcast receiver import**

Add `Duration` to the existing imports near the top:

```rust
use tokio::time::{timeout, Duration};
```

- [ ] **Step 2: Add active subscriber delivery test**

Append this test near the existing `app_state_process_media_frame_uses_shared_relay_state` tests:

```rust
#[tokio::test]
async fn processed_media_subscriber_receives_future_frames() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_01");
    start_relay_with_track(&state, room_id.clone(), user_id.clone(), NoiseProvider::Off);
    let mut receiver = state.subscribe_processed_media_frames(&room_id);

    state
        .process_media_frame(audio_frame(
            room_id.clone(),
            user_id,
            vec![0.25, -0.5, 0.75],
        ))
        .unwrap();

    let frame = timeout(Duration::from_secs(1), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(frame.room_id, room_id);
    assert_eq!(frame.samples, vec![0.25, -0.5, 0.75]);
    assert_eq!(frame.noise.provider, NoiseProvider::Off);
}
```

- [ ] **Step 3: Add room isolation test**

Append this test after the active subscriber test:

```rust
#[tokio::test]
async fn processed_media_subscribers_are_room_scoped() {
    let state = AppState::default();
    let default_room = RoomId::default_room();
    let other_room = RoomId::parse_boundary("OTHER").unwrap();
    let user_id = UserId::from_external("user_01");
    start_relay_with_track(
        &state,
        default_room.clone(),
        user_id.clone(),
        NoiseProvider::Off,
    );
    let mut other_receiver = state.subscribe_processed_media_frames(&other_room);

    state
        .process_media_frame(audio_frame(default_room, user_id, vec![1.0]))
        .unwrap();

    assert!(
        timeout(Duration::from_millis(25), other_receiver.recv())
            .await
            .is_err()
    );
}
```

- [ ] **Step 4: Add late subscriber test**

Append this test after the room isolation test:

```rust
#[tokio::test]
async fn processed_media_late_subscriber_only_receives_future_frames() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_01");
    start_relay_with_track(&state, room_id.clone(), user_id.clone(), NoiseProvider::Off);

    state
        .process_media_frame(audio_frame(room_id.clone(), user_id.clone(), vec![1.0]))
        .unwrap();

    let mut receiver = state.subscribe_processed_media_frames(&room_id);
    assert_eq!(state.processed_media_frames(&room_id).len(), 1);
    assert!(
        timeout(Duration::from_millis(25), receiver.recv())
            .await
            .is_err()
    );

    state
        .process_media_frame(audio_frame(room_id.clone(), user_id, vec![2.0]))
        .unwrap();

    let frame = timeout(Duration::from_secs(1), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(frame.samples, vec![2.0]);
}
```

- [ ] **Step 5: Run the targeted test and confirm it fails before implementation**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" -p lyre-web processed_media
```

Expected: compile failure because `subscribe_processed_media_frames` is not implemented yet.

### Task 2: Implement Processed Audio Broadcaster

**Files:**

- Modify: `crates/lyre-web/src/media_runtime.rs`
- Modify: `crates/lyre-web/src/api.rs`

- [ ] **Step 1: Replace the recording sink with a broadcaster**

In `crates/lyre-web/src/media_runtime.rs`, replace `RecordingProcessedAudioSink` with:

```rust
use tokio::sync::broadcast;

const PROCESSED_AUDIO_CHANNEL_CAPACITY: usize = 256;

#[derive(Debug, Clone, Default)]
pub struct ProcessedAudioBroadcaster {
    frames: Arc<DashMap<RoomId, Vec<ProcessedAudioFrame>>>,
    channels: Arc<DashMap<RoomId, broadcast::Sender<ProcessedAudioFrame>>>,
}

impl ProcessedAudioBroadcaster {
    pub fn frames_for_room(&self, room_id: &RoomId) -> Vec<ProcessedAudioFrame> {
        self.frames
            .get(room_id)
            .map(|frames| frames.clone())
            .unwrap_or_default()
    }

    pub fn subscribe(&self, room_id: &RoomId) -> broadcast::Receiver<ProcessedAudioFrame> {
        self.sender(room_id).subscribe()
    }

    pub fn clear_room(&self, room_id: &RoomId) {
        self.frames.remove(room_id);
        self.channels.remove(room_id);
    }

    fn sender(&self, room_id: &RoomId) -> broadcast::Sender<ProcessedAudioFrame> {
        self.channels
            .entry(room_id.clone())
            .or_insert_with(|| broadcast::channel(PROCESSED_AUDIO_CHANNEL_CAPACITY).0)
            .clone()
    }
}

impl ProcessedAudioSink for ProcessedAudioBroadcaster {
    fn publish(&self, frame: ProcessedAudioFrame) {
        self.frames
            .entry(frame.room_id.clone())
            .or_default()
            .push(frame.clone());
        let _ = self.sender(&frame.room_id).send(frame);
    }
}
```

This deliberately keeps Tokio broadcast semantics: no-subscriber sends are allowed, and slow receivers may observe `RecvError::Lagged` when they fall more than 256 frames behind. This increment does not implement replay or backpressure.

- [ ] **Step 2: Update `WebMediaRuntime` to use the broadcaster**

Change the runtime and sink fields to:

```rust
pub struct WebMediaRuntime {
    runtime: MediaRuntime<NoiseCancellingAudioFrameProcessor, ProcessedAudioBroadcaster>,
    sink: ProcessedAudioBroadcaster,
}
```

Update `new` to construct `ProcessedAudioBroadcaster::default()`, and add methods:

```rust
pub fn subscribe(&self, room_id: &RoomId) -> broadcast::Receiver<ProcessedAudioFrame> {
    self.sink.subscribe(room_id)
}

pub fn clear_room(&self, room_id: &RoomId) {
    self.sink.clear_room(room_id);
}
```

- [ ] **Step 3: Add `AppState` wrappers**

In `crates/lyre-web/src/api.rs`, import `tokio::sync::broadcast` alongside `mpsc`:

```rust
use tokio::sync::{broadcast, mpsc};
```

Add these methods to `impl AppState`:

```rust
pub fn subscribe_processed_media_frames(
    &self,
    room_id: &RoomId,
) -> broadcast::Receiver<ProcessedAudioFrame> {
    self.media_runtime.subscribe(room_id)
}

pub fn clear_processed_media_room(&self, room_id: &RoomId) {
    self.media_runtime.clear_room(room_id);
}
```

- [ ] **Step 4: Run targeted tests**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" -p lyre-web processed_media
```

Expected: all `processed_media_*` tests pass.

### Task 3: Clear Processed Audio on REST Relay Stop

**Files:**

- Modify: `crates/lyre-web/src/api.rs`
- Modify: `crates/lyre-web/src/api_media_tests.rs`

- [ ] **Step 1: Clear processed room state in `stop_media_relay` route**

Change `stop_media_relay` to store the status, clear processed room state, and return the status:

```rust
async fn stop_media_relay(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(request): Json<lyre_core::StopMediaRelayRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    let status = state.media_relays.stop(room_id.clone(), request);
    state.clear_processed_media_room(&room_id);
    Ok(Json(status))
}
```

- [ ] **Step 2: Add HTTP stop cleanup test**

Append a test to `api_media_tests.rs`:

```rust
#[tokio::test]
async fn media_relay_stop_route_clears_processed_frames() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_01");
    start_relay_with_track(&state, room_id.clone(), user_id.clone(), NoiseProvider::Off);
    state
        .process_media_frame(audio_frame(room_id.clone(), user_id, vec![0.5]))
        .unwrap();
    assert_eq!(state.processed_media_frames(&room_id).len(), 1);

    let app = router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rooms/DEFAULT/media-relay/stop")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"user_id":"user_01"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(state.processed_media_frames(&room_id).is_empty());
}
```

Keep `app_state_process_media_frame_stop_relay_prevents_future_processing` focused on relay-state rejection after a direct registry stop. The HTTP stop cleanup test is the required proof that the route clears retained processed frame history.

- [ ] **Step 3: Run media API tests**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" -p lyre-web api_media_tests
```

Expected: all media API tests pass.

### Task 4: Update Documentation

**Files:**

- Modify: `MEMORY.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update memory**

Append:

```markdown
## 2026-06-15 Processed Audio Broadcast Contract

- Replaced the web runtime's recording-only processed frame sink with a room-scoped processed audio broadcaster.
- Broadcasts are internal `tokio::sync::broadcast` receivers for future WebRTC/SFU integration; no browser playback or RTP forwarding is implemented yet.
- Stopping a media relay clears retained processed frame history for that room.
```

- [ ] **Step 2: Update roadmap**

Move this item into Completed:

```markdown
- Internal room-scoped processed-audio broadcast contract for future server media forwarding.
```

Keep these items in Next:

```markdown
- Implement real WebRTC media termination/SFU-like audio pipeline and broadcast architecture.
- Broadcast processed server audio frames to clients.
```

- [ ] **Step 3: Run documentation diff check**

Run:

```bash
git diff -- docs/roadmap.md MEMORY.md
```

Expected: documentation describes only the internal broadcast contract and still lists real WebRTC/client broadcast as future work.

### Task 5: Final Verification

**Files:**

- Verify the whole workspace.

- [ ] **Step 1: Check file sizes**

Run:

```bash
wc -l crates/lyre-web/src/media_runtime.rs crates/lyre-web/src/api.rs crates/lyre-web/src/api_media_tests.rs
```

Expected: every listed file is below 400 lines.

- [ ] **Step 2: Run Rust formatting check**

Run:

```bash
cargo fmt --all --check
```

Expected: exit 0.

- [ ] **Step 3: Run Rust clippy**

Run:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: exit 0.

- [ ] **Step 4: Run Rust tests**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: exit 0.

- [ ] **Step 5: Run frontend verification**

Run:

```bash
cd frontend && npm run generate:webrpc && npm test -- --run && npm run typecheck && npm run lint && npm run build
```

Expected: exit 0.

- [ ] **Step 6: Check whitespace**

Run:

```bash
git diff --check
```

Expected: exit 0.
