# Processed Audio Egress Fanout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an internal fanout contract that turns processed PCM frames into deterministic per-recipient egress events for future server media forwarding.

**Architecture:** `lyre-core` gains one read-only, non-creating media relay participant snapshot method so callers can inspect active relay participants without mutating room state. Because `crates/lyre-core/src/media.rs` is already over the 400 line rule, this plan first moves its existing tests to `media_tests.rs` and registers that test module from `lib.rs`. `lyre-web` adds `media_egress.rs`, where `ProcessedAudioEgressFanout` validates the source frame against relay state and returns one egress event per other audio-capable participant. `AppState` owns the fanout service beside the media runtime and exposes an internal wrapper for future WebRTC/SFU code.

**Tech Stack:** Rust, `lyre-core` media relay registry, `lyre-web`, Axum app state, existing decoded PCM frame types.

---

### Task 1: Split Core Media Tests

**Files:**

- Modify: `crates/lyre-core/src/media.rs`
- Modify: `crates/lyre-core/src/lib.rs`
- Create: `crates/lyre-core/src/media_tests.rs`

- [ ] **Step 1: Move tests out of `media.rs`**

Move the entire existing `#[cfg(test)] mod tests { ... }` block from `crates/lyre-core/src/media.rs` into `crates/lyre-core/src/media_tests.rs`. In the new file, replace `use super::*;` with:

```rust
use crate::{
    MediaRelayError, MediaRelayMode, MediaRelayRegistry, MediaRelayStatus, MediaTrackKind,
    NoiseCancellationConfig, NoiseProvider, RegisterMediaTrackRequest, RoomId,
    StartMediaRelayRequest, StopMediaRelayRequest, UserId,
};
```

- [ ] **Step 2: Register the test module**

In `crates/lyre-core/src/lib.rs`, add:

```rust
#[cfg(test)]
mod media_tests;
```

- [ ] **Step 3: Run moved tests**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" -p lyre-core media_tests
```

Expected: moved media tests pass.

### Task 2: Add Read-Only Relay Participant Snapshot

**Files:**

- Modify: `crates/lyre-core/src/media.rs`

- [ ] **Step 1: Add the method**

Add this method to `impl MediaRelayRegistry` near `require_track`:

```rust
pub fn active_participants(
    &self,
    room_id: &RoomId,
) -> Result<Vec<MediaRelayParticipant>, MediaRelayError> {
    let Some(room) = self.rooms.get(room_id) else {
        return Err(MediaRelayError::Inactive {
            room_id: room_id.clone(),
        });
    };
    if !room.active {
        return Err(MediaRelayError::Inactive {
            room_id: room_id.clone(),
        });
    }
    let mut participants = room
        .participants
        .iter()
        .map(|entry| {
            let mut tracks = entry
                .value()
                .iter()
                .map(|track| MediaRelayTrack {
                    track_id: track.key().clone(),
                    kind: *track.value(),
                })
                .collect::<Vec<_>>();
            tracks.sort_by(|left, right| left.track_id.cmp(&right.track_id));
            MediaRelayParticipant {
                user_id: entry.key().clone(),
                tracks,
            }
        })
        .collect::<Vec<_>>();
    participants.sort_by(|left, right| left.user_id.cmp(&right.user_id));
    Ok(participants)
}
```

This method must not call `status`, `snapshot`, or `entry`, because unknown rooms must remain unknown.

- [ ] **Step 2: Export remains unnecessary**

No `lib.rs` export change is required because `MediaRelayRegistry` already exposes public methods through the exported type.

- [ ] **Step 3: Run focused core tests**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" -p lyre-core media_tests
```

Expected: tests pass. No new core test is required; egress behavior tests in `lyre-web` cover the new method through the public registry API.

### Task 3: Add Egress Fanout Module and Tests

**Files:**

- Create: `crates/lyre-web/src/media_egress.rs`
- Create: `crates/lyre-web/src/media_egress_tests.rs`
- Modify: `crates/lyre-web/src/lib.rs`

- [ ] **Step 1: Add `media_egress.rs`**

Create:

```rust
use lyre_core::{
    MediaRelayError, MediaRelayRegistry, MediaTrackKind, ProcessedAudioFrame, RoomId, UserId,
};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub struct ProcessedAudioEgressFrame {
    pub recipient_id: UserId,
    pub frame: ProcessedAudioFrame,
}

#[derive(Debug, Clone)]
pub struct ProcessedAudioEgressFanout {
    relays: Arc<MediaRelayRegistry>,
}

impl ProcessedAudioEgressFanout {
    pub fn new(relays: Arc<MediaRelayRegistry>) -> Self {
        Self { relays }
    }

    pub fn fanout(
        &self,
        frame: &ProcessedAudioFrame,
    ) -> Result<Vec<ProcessedAudioEgressFrame>, MediaRelayError> {
        let source_track =
            self.relays
                .require_track(&frame.room_id, &frame.user_id, &frame.track_id)?;
        if source_track.kind != MediaTrackKind::Audio {
            return Err(MediaRelayError::UnsupportedTrackKind {
                room_id: frame.room_id.clone(),
                user_id: frame.user_id.clone(),
                track_id: frame.track_id.clone(),
                kind: source_track.kind,
            });
        }

        Ok(self
            .relays
            .active_participants(&frame.room_id)?
            .into_iter()
            .filter(|participant| participant.user_id != frame.user_id)
            .filter(|participant| {
                participant
                    .tracks
                    .iter()
                    .any(|track| track.kind == MediaTrackKind::Audio)
            })
            .map(|participant| ProcessedAudioEgressFrame {
                recipient_id: participant.user_id,
                frame: frame.clone(),
            })
            .collect())
    }
}
```

Remove `RoomId` from imports if it is not used after formatting/clippy feedback.

- [ ] **Step 2: Register module and tests**

In `crates/lyre-web/src/lib.rs`, add:

```rust
pub mod media_egress;
```

and test module:

```rust
#[cfg(test)]
mod media_egress_tests;
```

- [ ] **Step 3: Add tests**

Create `crates/lyre-web/src/media_egress_tests.rs` with helper setup and tests:

```rust
use crate::media_egress::ProcessedAudioEgressFanout;
use lyre_core::{
    MediaRelayError, MediaRelayRegistry, MediaTrackKind, NoiseCancellationConfig, NoiseProvider,
    ProcessedAudioFrame, RegisterMediaTrackRequest, RoomId, StartMediaRelayRequest,
    StopMediaRelayRequest, UserId,
};
use std::sync::Arc;

fn frame(room_id: RoomId, user_id: UserId, track_id: impl Into<String>) -> ProcessedAudioFrame {
    ProcessedAudioFrame {
        room_id,
        user_id,
        track_id: track_id.into(),
        sample_rate_hz: 48_000,
        channels: 1,
        sequence: 9,
        samples: vec![0.1, -0.1],
        noise: NoiseCancellationConfig::default(),
    }
}

fn start(relays: &MediaRelayRegistry, room_id: &RoomId) {
    relays.start(
        room_id.clone(),
        StartMediaRelayRequest {
            noise: Some(NoiseCancellationConfig {
                provider: NoiseProvider::Rnnoise,
                intensity: 0.5,
                voice_activity_threshold: 0.35,
            }),
        },
    );
}

fn track(
    relays: &MediaRelayRegistry,
    room_id: &RoomId,
    user_id: &UserId,
    track_id: &str,
    kind: MediaTrackKind,
) {
    relays
        .register_track(
            room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: user_id.clone(),
                track_id: track_id.to_owned(),
                kind,
            },
        )
        .unwrap();
}

#[test]
fn fans_out_processed_audio_to_other_audio_participants() {
    let relays = Arc::new(MediaRelayRegistry::new());
    let room_id = RoomId::default_room();
    let source = UserId::from_external("user_source");
    let user_a = UserId::from_external("user_a");
    let user_b = UserId::from_external("user_b");
    start(&relays, &room_id);
    track(&relays, &room_id, &source, "audio-main", MediaTrackKind::Audio);
    track(&relays, &room_id, &user_b, "audio-main", MediaTrackKind::Audio);
    track(&relays, &room_id, &user_a, "audio-main", MediaTrackKind::Audio);

    let fanout = ProcessedAudioEgressFanout::new(Arc::clone(&relays));
    let recipients = fanout
        .fanout(&frame(room_id.clone(), source, "audio-main"))
        .unwrap()
        .into_iter()
        .map(|egress| egress.recipient_id)
        .collect::<Vec<_>>();

    assert_eq!(recipients, vec![user_a, user_b]);
}

#[test]
fn excludes_source_video_only_participants_and_duplicate_recipient_tracks() {
    let relays = Arc::new(MediaRelayRegistry::new());
    let room_id = RoomId::default_room();
    let source = UserId::from_external("user_source");
    let audio_user = UserId::from_external("user_audio");
    let video_user = UserId::from_external("user_video");
    start(&relays, &room_id);
    track(&relays, &room_id, &source, "audio-main", MediaTrackKind::Audio);
    track(&relays, &room_id, &source, "audio-monitor", MediaTrackKind::Audio);
    track(&relays, &room_id, &audio_user, "audio-main", MediaTrackKind::Audio);
    track(&relays, &room_id, &audio_user, "audio-backup", MediaTrackKind::Audio);
    track(&relays, &room_id, &video_user, "video-main", MediaTrackKind::Video);

    let fanout = ProcessedAudioEgressFanout::new(Arc::clone(&relays));
    let egress = fanout
        .fanout(&frame(room_id, source, "audio-main"))
        .unwrap();

    assert_eq!(egress.len(), 1);
    assert_eq!(egress[0].recipient_id, audio_user);
}

#[test]
fn stale_source_frame_errors_match_relay_state() {
    let relays = Arc::new(MediaRelayRegistry::new());
    let fanout = ProcessedAudioEgressFanout::new(Arc::clone(&relays));
    let room_id = RoomId::default_room();
    let source = UserId::from_external("user_source");

    assert_eq!(
        fanout.fanout(&frame(room_id.clone(), source.clone(), "audio-main")),
        Err(MediaRelayError::Inactive {
            room_id: room_id.clone()
        })
    );

    start(&relays, &room_id);
    assert_eq!(
        fanout.fanout(&frame(room_id.clone(), source.clone(), "audio-main")),
        Err(MediaRelayError::ParticipantNotFound {
            room_id: room_id.clone(),
            user_id: source.clone()
        })
    );

    track(&relays, &room_id, &source, "video-main", MediaTrackKind::Video);
    assert_eq!(
        fanout.fanout(&frame(room_id.clone(), source.clone(), "audio-main")),
        Err(MediaRelayError::TrackNotFound {
            room_id: room_id.clone(),
            user_id: source.clone(),
            track_id: "audio-main".to_owned()
        })
    );

    assert_eq!(
        fanout.fanout(&frame(room_id.clone(), source.clone(), "video-main")),
        Err(MediaRelayError::UnsupportedTrackKind {
            room_id,
            user_id: source,
            track_id: "video-main".to_owned(),
            kind: MediaTrackKind::Video
        })
    );
}

#[test]
fn stopped_relay_rejects_previous_frame() {
    let relays = Arc::new(MediaRelayRegistry::new());
    let room_id = RoomId::default_room();
    let source = UserId::from_external("user_source");
    start(&relays, &room_id);
    track(&relays, &room_id, &source, "audio-main", MediaTrackKind::Audio);
    let processed = frame(room_id.clone(), source.clone(), "audio-main");
    relays.stop(room_id.clone(), StopMediaRelayRequest { user_id: source });

    let fanout = ProcessedAudioEgressFanout::new(relays);

    assert_eq!(
        fanout.fanout(&processed),
        Err(MediaRelayError::Inactive { room_id })
    );
}
```

- [ ] **Step 4: Run targeted test and confirm it compiles/passes after implementation**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" -p lyre-web media_egress
```

Expected: all `media_egress` tests pass.

### Task 4: Wire AppState Egress Fanout

**Files:**

- Modify: `crates/lyre-web/src/api.rs`

- [ ] **Step 1: Import egress types**

Add to the existing crate imports:

```rust
media_egress::{ProcessedAudioEgressFanout, ProcessedAudioEgressFrame},
```

- [ ] **Step 2: Add AppState field**

Add to `AppState`:

```rust
pub media_egress: Arc<ProcessedAudioEgressFanout>,
```

- [ ] **Step 3: Initialize with shared registry**

In `AppState::new`, after `let media_relays = Arc::new(MediaRelayRegistry::new());`, initialize:

```rust
media_egress: Arc::new(ProcessedAudioEgressFanout::new(Arc::clone(&media_relays))),
```

- [ ] **Step 4: Add wrapper method**

Add to `impl AppState`:

```rust
pub fn processed_audio_egress_frames(
    &self,
    frame: &ProcessedAudioFrame,
) -> Result<Vec<ProcessedAudioEgressFrame>, MediaRelayError> {
    self.media_egress.fanout(frame)
}
```

- [ ] **Step 5: Add AppState wiring test**

In `crates/lyre-web/src/media_egress_tests.rs`, add a test using `AppState::default()`:

```rust
#[test]
fn app_state_egress_fanout_uses_shared_relay_registry() {
    let state = crate::api::AppState::default();
    let room_id = RoomId::default_room();
    let source = UserId::from_external("user_source");
    let recipient = UserId::from_external("user_recipient");
    start(&state.media_relays, &room_id);
    track(
        &state.media_relays,
        &room_id,
        &source,
        "audio-main",
        MediaTrackKind::Audio,
    );
    track(
        &state.media_relays,
        &room_id,
        &recipient,
        "audio-main",
        MediaTrackKind::Audio,
    );

    let egress = state
        .processed_audio_egress_frames(&frame(room_id, source, "audio-main"))
        .unwrap();

    assert_eq!(egress.len(), 1);
    assert_eq!(egress[0].recipient_id, recipient);
}
```

- [ ] **Step 6: Run targeted tests**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" -p lyre-web media_egress
```

Expected: all `media_egress` tests pass.

### Task 5: Independent Implementation Review Gate

**Files:**

- Review the implemented diff before documentation and final verification.

- [ ] **Step 1: Capture implementation diff**

Run:

```bash
git diff -- crates/lyre-core/src/media.rs crates/lyre-core/src/media_tests.rs crates/lyre-core/src/lib.rs crates/lyre-web/src/media_egress.rs crates/lyre-web/src/media_egress_tests.rs crates/lyre-web/src/api.rs crates/lyre-web/src/lib.rs > /tmp/processed-audio-egress-fanout.diff
```

Expected: diff includes only the reviewed implementation files and no docs yet.

- [ ] **Step 2: Dispatch independent implementation reviewer**

Send the approved spec, reviewed plan, implementation diff, and targeted verification output to an independent reviewer. Require this exact verdict format:

```text
VERDICT: APPROVE | REVISE
SPEC_COVERAGE:
- [implemented requirement or missing requirement]
BLOCKERS:
- [blocking gap or "None"]
REQUIRED_CHANGES:
- [change or "None"]
```

Proceed to documentation only after the reviewer returns `VERDICT: APPROVE`.

### Task 6: Update Documentation

**Files:**

- Modify: `MEMORY.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update memory**

Append:

```markdown
## 2026-06-15 Processed Audio Egress Fanout

- Added an internal processed-audio egress fanout contract that maps processed source frames to other audio-capable relay participants.
- Egress fanout validates the source track against current relay state and returns relay errors for stale or non-audio source frames.
- Kept real WebRTC media termination, RTP/Opus packetization, and browser delivery as future work.
```

- [ ] **Step 2: Update roadmap**

Add to Completed:

```markdown
- Internal processed-audio egress fanout contract for future server media forwarding.
```

Keep these in Next:

```markdown
- Implement real WebRTC media termination/SFU-like audio pipeline and broadcast architecture.
- Broadcast processed server audio frames to clients.
```

### Task 7: Final Verification

**Files:**

- Verify the whole workspace.

- [ ] **Step 1: Check file sizes**

Run:

```bash
wc -l crates/lyre-core/src/media.rs crates/lyre-core/src/media_tests.rs crates/lyre-web/src/media_egress.rs crates/lyre-web/src/media_egress_tests.rs crates/lyre-web/src/api.rs crates/lyre-web/src/lib.rs
```

Expected: no touched file exceeds 400 lines.

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
