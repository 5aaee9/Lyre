# Web Media Runtime Wiring Implementation Plan

> **For agentic workers:** REQUIRED WORKFLOW: Execute this plan as `$sdd-workflow`. Treat Task 1 through Task 3 as bounded SDD implementation subtasks: each subtask keeps to its listed files, follows the approved spec and plan, and is covered by independent implementation review. Task 6 is the post-review documentation subtask required by `$sdd-workflow`. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Wire the existing decoded-PCM core media runtime and RNNoise adapter into `lyre-web::AppState` behind an internal, testable processing path.

**Architecture:** Add a `lyre-web::media_runtime` module with a cloneable in-memory processed-frame sink and a `WebMediaRuntime` wrapper. `AppState` owns one shared `Arc<MediaRelayRegistry>` for REST relay state and runtime validation, then exposes internal methods for future WebRTC media termination code to process decoded frames and inspect processed frames in tests.

**Tech Stack:** Rust 2021, Axum app state, `lyre-core::MediaRuntime`, `lyre-noise-cancelling::NoiseCancellingAudioFrameProcessor`, `DashMap`, existing cargo/nextest/frontend verification.

---

## File Structure

- Modify `crates/lyre-web/Cargo.toml`: add path dependency on `lyre-noise-cancelling`.
- Modify `crates/lyre-web/src/lib.rs`: expose the new `media_runtime` module only within the crate unless public export is required by tests.
- Create `crates/lyre-web/src/media_runtime.rs`: own `RecordingProcessedAudioSink` and `WebMediaRuntime`.
- Modify `crates/lyre-web/src/api.rs`: wire `AppState` to create and expose `WebMediaRuntime`, and add tests.
- Modify `README.md`, `MEMORY.md`, and `docs/roadmap.md` after implementation is review-ready.
- Modify this plan to check completed steps.

## Task 0: SDD Gates

**Files:**
- Read: `docs/superpowers/specs/2026-06-15-web-media-runtime-wiring-design.md`
- Modify: `docs/superpowers/plans/2026-06-15-web-media-runtime-wiring.md`

- [x] **Step 1: Confirm approved spec review exists**

Expected evidence:

```text
VERDICT: APPROVE
ISSUES:
- None
REQUIRED_CHANGES:
- None
```

- [x] **Step 2: Dispatch independent plan reviewer**

Review this plan against `docs/superpowers/specs/2026-06-15-web-media-runtime-wiring-design.md`.

Expected verdict:

```text
VERDICT: APPROVE
ISSUES:
- None
REQUIRED_CHANGES:
- None
```

- [x] **Step 3: Stop before implementation unless plan is approved**

Do not modify Cargo, Rust runtime files, README, MEMORY, or roadmap until the plan reviewer returns `VERDICT: APPROVE`.

## Task 1: Add Web Runtime Tests

**SDD subtask gate:** This is a bounded `$sdd-workflow` implementation subtask. It may edit only `crates/lyre-web/src/api.rs` and must be covered by final independent implementation review.

**Files:**
- Modify: `crates/lyre-web/src/api.rs`

- [x] **Step 1: Add test imports**

In the existing `#[cfg(test)] mod tests`, extend imports to include:

```rust
use lyre_core::{
    AudioFrame, MediaRelayError, MediaTrackKind, NoiseCancellationConfig, NoiseProvider,
    RegisterMediaTrackRequest, StartMediaRelayRequest, StopMediaRelayRequest, UserId,
};
```

- [x] **Step 2: Add helpers inside the test module**

Add:

```rust
fn audio_frame(room_id: RoomId, user_id: UserId, samples: Vec<f32>) -> AudioFrame {
    AudioFrame {
        room_id,
        user_id,
        track_id: "audio-main".to_owned(),
        sample_rate_hz: 48_000,
        channels: 1,
        sequence: 1,
        samples,
    }
}

fn start_relay_with_track(
    state: &AppState,
    room_id: RoomId,
    user_id: UserId,
    provider: NoiseProvider,
) {
    state.media_relays.start(
        room_id.clone(),
        StartMediaRelayRequest {
            noise: Some(NoiseCancellationConfig {
                provider,
                intensity: 0.5,
                voice_activity_threshold: 0.35,
            }),
        },
    );
    state
        .media_relays
        .register_track(
            room_id,
            RegisterMediaTrackRequest {
                user_id,
                track_id: "audio-main".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
}
```

- [x] **Step 3: Add tests for state wiring, errors, off processing, RNNoise processing, and stop behavior**

Add:

```rust
#[test]
fn app_state_process_media_frame_uses_shared_relay_state() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_01");
    start_relay_with_track(&state, room_id.clone(), user_id.clone(), NoiseProvider::Off);

    state
        .process_media_frame(audio_frame(room_id.clone(), user_id, vec![0.25, -0.5, 0.75]))
        .unwrap();

    let frames = state.processed_media_frames(&room_id);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].samples, vec![0.25, -0.5, 0.75]);
    assert_eq!(frames[0].noise.provider, NoiseProvider::Off);
}

#[test]
fn app_state_process_media_frame_propagates_relay_errors() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_01");

    assert_eq!(
        state.process_media_frame(audio_frame(room_id.clone(), user_id.clone(), vec![0.0])),
        Err(MediaRelayError::Inactive {
            room_id: room_id.clone(),
        })
    );

    state
        .media_relays
        .start(room_id.clone(), StartMediaRelayRequest::default());
    assert_eq!(
        state.process_media_frame(audio_frame(room_id.clone(), user_id.clone(), vec![0.0])),
        Err(MediaRelayError::ParticipantNotFound {
            room_id: room_id.clone(),
            user_id: user_id.clone(),
        })
    );

    state
        .media_relays
        .register_track(
            room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: user_id.clone(),
                track_id: "other-track".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
    assert_eq!(
        state.process_media_frame(audio_frame(room_id.clone(), user_id.clone(), vec![0.0])),
        Err(MediaRelayError::TrackNotFound {
            room_id,
            user_id,
            track_id: "audio-main".to_owned(),
        })
    );
}

#[test]
fn app_state_process_media_frame_does_not_create_unknown_room() {
    let state = AppState::default();
    let room_id = RoomId::parse_boundary("UNKNOWN").unwrap();

    assert_eq!(
        state.process_media_frame(audio_frame(
            room_id.clone(),
            UserId::from_external("user_01"),
            vec![0.0],
        )),
        Err(MediaRelayError::Inactive {
            room_id: room_id.clone(),
        })
    );
    assert!(!state.media_relays.contains_room(&room_id));
}

#[test]
fn app_state_process_media_frame_runs_rnnoise_for_valid_audio() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_01");
    start_relay_with_track(&state, room_id.clone(), user_id.clone(), NoiseProvider::Rnnoise);

    state
        .process_media_frame(audio_frame(room_id.clone(), user_id, vec![120.0; 480]))
        .unwrap();

    let frames = state.processed_media_frames(&room_id);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].samples.len(), 480);
    assert_eq!(frames[0].noise.provider, NoiseProvider::Rnnoise);
}

#[test]
fn app_state_stop_relay_prevents_future_processing() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_01");
    start_relay_with_track(&state, room_id.clone(), user_id.clone(), NoiseProvider::Off);

    state.media_relays.stop(
        room_id.clone(),
        StopMediaRelayRequest {
            user_id: user_id.clone(),
        },
    );

    assert_eq!(
        state.process_media_frame(audio_frame(room_id.clone(), user_id, vec![0.0])),
        Err(MediaRelayError::Inactive { room_id })
    );
}
```

- [x] **Step 4: Run tests and confirm failure before implementation**

Run:

```bash
cargo test -p lyre-web app_state_process_media_frame -- --nocapture
```

Expected: compile failures for missing `AppState::process_media_frame`, `AppState::processed_media_frames`, and missing `lyre-web::media_runtime` wiring.

## Task 2: Add Web Runtime Module

**SDD subtask gate:** This is a bounded `$sdd-workflow` implementation subtask. It may edit only `crates/lyre-web/Cargo.toml`, `crates/lyre-web/src/lib.rs`, and `crates/lyre-web/src/media_runtime.rs`, and must be covered by final independent implementation review.

**Files:**
- Modify: `crates/lyre-web/Cargo.toml`
- Modify: `crates/lyre-web/src/lib.rs`
- Create: `crates/lyre-web/src/media_runtime.rs`

- [x] **Step 1: Add dependency**

In `crates/lyre-web/Cargo.toml`, add:

```toml
lyre-noise-cancelling = { path = "../lyre-noise-cancelling" }
```

- [x] **Step 2: Expose module**

In `crates/lyre-web/src/lib.rs`, add:

```rust
pub mod media_runtime;
```

- [x] **Step 3: Create media runtime module**

Create `crates/lyre-web/src/media_runtime.rs`:

```rust
use dashmap::DashMap;
use lyre_core::{
    AudioFrame, MediaRelayError, MediaRelayRegistry, MediaRuntime, ProcessedAudioFrame,
    ProcessedAudioSink, RoomId,
};
use lyre_noise_cancelling::NoiseCancellingAudioFrameProcessor;
use std::{fmt, sync::Arc};

#[derive(Debug, Clone, Default)]
pub struct RecordingProcessedAudioSink {
    frames: Arc<DashMap<RoomId, Vec<ProcessedAudioFrame>>>,
}

impl RecordingProcessedAudioSink {
    pub fn frames_for_room(&self, room_id: &RoomId) -> Vec<ProcessedAudioFrame> {
        self.frames
            .get(room_id)
            .map(|frames| frames.clone())
            .unwrap_or_default()
    }

    pub fn clear_room(&self, room_id: &RoomId) {
        self.frames.remove(room_id);
    }
}

impl ProcessedAudioSink for RecordingProcessedAudioSink {
    fn publish(&self, frame: ProcessedAudioFrame) {
        self.frames
            .entry(frame.room_id.clone())
            .or_default()
            .push(frame);
    }
}

pub struct WebMediaRuntime {
    runtime: MediaRuntime<NoiseCancellingAudioFrameProcessor, RecordingProcessedAudioSink>,
    sink: RecordingProcessedAudioSink,
}

impl WebMediaRuntime {
    pub fn new(relays: Arc<MediaRelayRegistry>) -> Self {
        let sink = RecordingProcessedAudioSink::default();
        let runtime = MediaRuntime::new(
            relays,
            NoiseCancellingAudioFrameProcessor::default(),
            sink.clone(),
        );
        Self { runtime, sink }
    }

    pub fn process_frame(&self, frame: AudioFrame) -> Result<(), MediaRelayError> {
        self.runtime.process_frame(frame)
    }

    pub fn frames_for_room(&self, room_id: &RoomId) -> Vec<ProcessedAudioFrame> {
        self.sink.frames_for_room(room_id)
    }
}

impl fmt::Debug for WebMediaRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WebMediaRuntime")
            .finish_non_exhaustive()
    }
}
```

- [x] **Step 4: Run module compile check**

Run:

```bash
cargo check -p lyre-web --all-targets
```

Expected: still fails until `AppState` wiring is added in Task 3, or passes if Task 3 has already been implemented in the same local edit batch.

## Task 3: Wire AppState

**SDD subtask gate:** This is a bounded `$sdd-workflow` implementation subtask. It may edit only `crates/lyre-web/src/api.rs`, and must be covered by final independent implementation review.

**Files:**
- Modify: `crates/lyre-web/src/api.rs`

- [x] **Step 1: Add import**

Add:

```rust
use crate::media_runtime::WebMediaRuntime;
```

Extend the `lyre_core` import with:

```rust
AudioFrame, MediaRelayError, ProcessedAudioFrame
```

- [x] **Step 2: Add AppState field**

Add to `AppState`:

```rust
pub media_runtime: Arc<WebMediaRuntime>,
```

- [x] **Step 3: Construct shared relay registry and runtime**

Change `AppState::new` so it creates `media_relays` first:

```rust
let media_relays = Arc::new(MediaRelayRegistry::new());
Self {
    registry: Arc::new(RoomRegistry::new()),
    media_runtime: Arc::new(WebMediaRuntime::new(Arc::clone(&media_relays))),
    media_relays,
    peers: Arc::new(PeerHub::new()),
    ice_servers: Arc::new(ice_servers),
    turn_rest_credentials,
}
```

- [x] **Step 4: Add internal processing methods**

Add to `impl AppState`:

```rust
pub fn process_media_frame(&self, frame: AudioFrame) -> Result<(), MediaRelayError> {
    self.media_runtime.process_frame(frame)
}

pub fn processed_media_frames(&self, room_id: &RoomId) -> Vec<ProcessedAudioFrame> {
    self.media_runtime.frames_for_room(room_id)
}
```

- [x] **Step 5: Run targeted web tests**

Run:

```bash
cargo test -p lyre-web app_state_process_media_frame -- --nocapture
```

Expected: the five new web runtime tests pass.

## Task 4: Rust Verification Before Docs

**Files:**
- Read: changed Rust files and Cargo manifests

- [x] **Step 1: Run targeted crate tests**

Run:

```bash
cargo test -p lyre-web app_state_process_media_frame -- --nocapture
cargo test -p lyre-web media_relay_ -- --nocapture
```

Expected: targeted web runtime and existing media relay tests pass.

- [x] **Step 2: Run full Rust verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: format check, clippy, and all workspace tests pass.

## Task 5: Independent Implementation Review

**Files:**
- Read: full implementation diff, spec, plan, verification output

- [x] **Step 1: Dispatch independent implementation reviewer**

Reviewer input must include:

- spec path `docs/superpowers/specs/2026-06-15-web-media-runtime-wiring-design.md`
- plan path `docs/superpowers/plans/2026-06-15-web-media-runtime-wiring.md`
- relevant implementation diff
- targeted/full Rust verification output

Expected verdict:

```text
VERDICT: APPROVE
SPEC_COVERAGE:
- ...
BLOCKERS:
- None
REQUIRED_CHANGES:
- None
```

Do not update README, MEMORY, roadmap, commit, or push until the implementation reviewer approves the implementation diff.

## Task 6: Documentation Updates After Implementation Approval

**SDD subtask gate:** This is the post-review documentation subtask required by `$sdd-workflow`. It may edit only `README.md`, `MEMORY.md`, `docs/roadmap.md`, and this plan after independent implementation review returns `VERDICT: APPROVE`.

**Files:**
- Modify: `README.md`
- Modify: `MEMORY.md`
- Modify: `docs/roadmap.md`
- Modify: `docs/superpowers/plans/2026-06-15-web-media-runtime-wiring.md`

- [x] **Step 1: Update README**

In `README.md` Media Topology section, add:

```md
`lyre-web::AppState` now owns an internal decoded-PCM media runtime wired to the media relay registry and RNNoise-capable processor. Processed frames are stored in an internal in-memory sink for tests and future broadcaster integration. This is not browser WebRTC media termination or client broadcast yet.
```

- [x] **Step 2: Update MEMORY**

Append:

```md
## 2026-06-15 Web Media Runtime Wiring

- Wired `lyre-web::AppState` to own a decoded-PCM `MediaRuntime` using the shared media relay registry.
- Connected the web runtime to `lyre-noise-cancelling::NoiseCancellingAudioFrameProcessor`.
- Stored processed frames in an internal in-memory sink for tests and future broadcaster integration.
- Kept WebRTC media termination, Opus decode/encode, and client broadcast as future work.
```

- [x] **Step 3: Update roadmap**

Add Completed:

```md
- Web server decoded-PCM media runtime wiring with internal processed-frame sink.
```

Keep Next:

```md
- Implement real WebRTC media termination/SFU-like audio pipeline and broadcast architecture.
- Broadcast processed server audio frames to clients.
```

- [x] **Step 4: Mark completed boxes**

Only check boxes for steps actually completed.

## Task 7: Final Verification, Commit, Push

**Files:**
- Modify: `docs/superpowers/plans/2026-06-15-web-media-runtime-wiring.md`

- [x] **Step 1: Run final verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
cd frontend
npm run generate:webrpc
npm test -- --run
npm run typecheck
npm run lint
npm run build
git diff --check
git diff --stat
git status --short
```

Expected: all commands pass, and status shows only intended files before commit.

- [x] **Step 2: Commit with Lore protocol**

Stage:

```bash
git add crates/lyre-web/Cargo.toml crates/lyre-web/src/lib.rs crates/lyre-web/src/api.rs crates/lyre-web/src/media_runtime.rs README.md MEMORY.md docs/roadmap.md docs/superpowers/specs/2026-06-15-web-media-runtime-wiring-design.md docs/superpowers/plans/2026-06-15-web-media-runtime-wiring.md
```

Commit:

```text
Wire web media runtime to relay state

Constraint: This wires decoded PCM processing inside AppState only; browser WebRTC media termination and client broadcast remain future work.
Rejected: Adding a public PCM upload endpoint | It would create a misleading external API before real WebRTC termination exists.
Confidence: high
Scope-risk: moderate
Directive: Do not mark end-to-end server audio broadcast complete until processed frames are sent to clients.
Tested: cargo fmt --all --check; cargo clippy --workspace --all-targets -- -D warnings; cargo nextest run --manifest-path Cargo.toml --workspace; npm run generate:webrpc; npm test -- --run; npm run typecheck; npm run lint; npm run build; git diff --check
Not-tested: Browser WebRTC media termination, RTP/RTCP, Opus decode/encode, SFU forwarding, and client playback of processed server audio remain future work.
```

- [x] **Step 3: Push**

Run:

```bash
git push
```

Expected: push succeeds or exact remote/credential error is reported with local commit SHA.
