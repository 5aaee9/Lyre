# Server Media Runtime Boundary Implementation Plan

> **For agentic workers:** REQUIRED WORKFLOW: Execute this plan as a `$sdd-workflow` subtask. Use the SDD reviewer gates before implementation and after implementation. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Add a decoded-PCM server media runtime boundary in `lyre-core` for future WebRTC media relay processing.

**Architecture:** Extend `lyre-core::media` with read-only relay lookup and richer relay errors. Add `lyre-core::media_runtime` for audio frame DTOs, processor/sink traits, and a synchronous runtime that gates processing through active registered audio tracks.

**Tech Stack:** Rust, `DashMap`, `serde`, `thiserror`, existing `lyre-core` types.

---

## File Structure

- Modify `crates/lyre-core/src/media.rs`: add read-only track lookup and new relay errors.
- Create `crates/lyre-core/src/media_runtime.rs`: audio frame DTOs, processor/sink traits, runtime, and tests.
- Modify `crates/lyre-core/src/lib.rs`: export the new module and types.
- Modify `crates/lyre-web/src/error.rs`: keep API error mapping exhaustive for new `MediaRelayError` variants.
- Post-review docs: `README.md`, `MEMORY.md`, `docs/roadmap.md`.

## Task 0: SDD Pre-Implementation Gate

**Files:**
- Read: `docs/superpowers/specs/2026-06-15-server-media-runtime-boundary-design.md`
- Read: `docs/superpowers/plans/2026-06-15-server-media-runtime-boundary.md`

- [x] **Step 1: Confirm approved spec review exists**

Required evidence before implementation:

```text
VERDICT: APPROVE
ISSUES:
- None
REQUIRED_CHANGES:
- None
```

The approved spec path is:

```text
docs/superpowers/specs/2026-06-15-server-media-runtime-boundary-design.md
```

- [x] **Step 2: Dispatch independent plan reviewer**

Dispatch an independent reviewer with the approved spec and this plan. Required verdict format:

```text
VERDICT: APPROVE | REVISE
ISSUES:
- [blocking issue or "None"]
REQUIRED_CHANGES:
- [change or "None"]
```

- [x] **Step 3: Stop before code edits unless plan is approved**

Implementation may begin only after the independent plan reviewer returns:

```text
VERDICT: APPROVE
```

If the reviewer returns `REVISE`, update this plan and re-review until approved.

## Task 1: Add Read-Only Media Relay Lookup

**Files:**
- Modify: `crates/lyre-core/src/media.rs`
- Modify: `crates/lyre-web/src/error.rs`

- [x] **Step 1: Add failing core tests**

Add tests to `crates/lyre-core/src/media.rs`:

```rust
#[test]
fn read_only_track_lookup_does_not_create_unknown_room() {
    let registry = MediaRelayRegistry::new();
    let room_id = RoomId::parse_boundary("UNKNOWN").unwrap();

    assert_eq!(
        registry.require_track(&room_id, &UserId::from_external("user_01"), "audio-main"),
        Err(MediaRelayError::Inactive {
            room_id: room_id.clone(),
        })
    );
    assert!(!registry.contains_room(&room_id));
}

#[test]
fn read_only_track_lookup_reports_participant_track_and_kind() {
    let registry = MediaRelayRegistry::new();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_01");
    registry.start(
        room_id.clone(),
        StartMediaRelayRequest {
            noise: Some(NoiseCancellationConfig {
                provider: NoiseProvider::Rnnoise,
                intensity: 0.8,
                voice_activity_threshold: 0.2,
            }),
        },
    );

    assert_eq!(
        registry.require_track(&room_id, &user_id, "audio-main"),
        Err(MediaRelayError::ParticipantNotFound {
            room_id: room_id.clone(),
            user_id: user_id.clone(),
        })
    );

    registry
        .register_track(
            room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: user_id.clone(),
                track_id: "audio-main".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();

    assert_eq!(
        registry.require_track(&room_id, &user_id, "missing-track"),
        Err(MediaRelayError::TrackNotFound {
            room_id: room_id.clone(),
            user_id: user_id.clone(),
            track_id: "missing-track".to_owned(),
        })
    );

    let track = registry.require_track(&room_id, &user_id, "audio-main").unwrap();
    assert_eq!(track.kind, MediaTrackKind::Audio);
    assert_eq!(track.noise.provider, NoiseProvider::Rnnoise);
}
```

- [x] **Step 2: Run media tests and confirm failure**

Run:

```bash
cargo test -p lyre-core media::tests -- --nocapture
```

Expected: fail because `require_track`, `contains_room`, new error variants, and lookup return type do not exist.

- [x] **Step 3: Implement read-only lookup and errors**

In `crates/lyre-core/src/media.rs`:

- Extend `MediaRelayError`:

```rust
#[derive(Debug, Error, PartialEq, Eq)]
pub enum MediaRelayError {
    #[error("media relay is not active for room `{room_id}`")]
    Inactive { room_id: RoomId },
    #[error("media relay participant `{user_id}` is not registered in room `{room_id}`")]
    ParticipantNotFound { room_id: RoomId, user_id: UserId },
    #[error("media relay track `{track_id}` is not registered for participant `{user_id}` in room `{room_id}`")]
    TrackNotFound {
        room_id: RoomId,
        user_id: UserId,
        track_id: String,
    },
    #[error("media relay track `{track_id}` for participant `{user_id}` in room `{room_id}` is `{kind:?}`, not audio")]
    UnsupportedTrackKind {
        room_id: RoomId,
        user_id: UserId,
        track_id: String,
        kind: MediaTrackKind,
    },
}
```

- Add lookup DTO:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct MediaRelayTrackLookup {
    pub room_id: RoomId,
    pub user_id: UserId,
    pub track_id: String,
    pub kind: MediaTrackKind,
    pub noise: NoiseCancellationConfig,
}
```

- Add `MediaRelayRegistry` methods:

```rust
pub fn contains_room(&self, room_id: &RoomId) -> bool {
    self.rooms.contains_key(room_id)
}

pub fn require_track(
    &self,
    room_id: &RoomId,
    user_id: &UserId,
    track_id: &str,
) -> Result<MediaRelayTrackLookup, MediaRelayError> {
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
    let Some(participant) = room.participants.get(user_id) else {
        return Err(MediaRelayError::ParticipantNotFound {
            room_id: room_id.clone(),
            user_id: user_id.clone(),
        });
    };
    let Some(kind) = participant.get(track_id).map(|entry| *entry.value()) else {
        return Err(MediaRelayError::TrackNotFound {
            room_id: room_id.clone(),
            user_id: user_id.clone(),
            track_id: track_id.to_owned(),
        });
    };
    Ok(MediaRelayTrackLookup {
        room_id: room_id.clone(),
        user_id: user_id.clone(),
        track_id: track_id.to_owned(),
        kind,
        noise: room.noise.clone(),
    })
}
```

Do not call `status()` inside `require_track`; that would create missing room state.

- [x] **Step 4: Update web API error mapping**

In `crates/lyre-web/src/error.rs`, keep current HTTP behavior for `Inactive` and map the internal-only variants to `409 CONFLICT` as well:

```rust
Self::MediaRelay(error) => (StatusCode::CONFLICT, error.to_string()),
```

This keeps the match exhaustive without introducing unreachable wildcard code.

- [x] **Step 5: Run media and web tests**

Run:

```bash
cargo test -p lyre-core media::tests -- --nocapture
cargo test -p lyre-web media_relay_ -- --nocapture
```

Expected: pass.

## Task 2: Add Core Media Runtime

**Files:**
- Create: `crates/lyre-core/src/media_runtime.rs`
- Modify: `crates/lyre-core/src/lib.rs`

- [x] **Step 1: Add media runtime module and tests**

Create `crates/lyre-core/src/media_runtime.rs` with the public types and tests in one file. The implementation should include:

```rust
use crate::{
    MediaRelayError, MediaRelayRegistry, MediaTrackKind, NoiseCancellationConfig, RoomId, UserId,
};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq)]
pub struct AudioFrame {
    pub room_id: RoomId,
    pub user_id: UserId,
    pub track_id: String,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub sequence: u64,
    pub samples: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProcessedAudioFrame {
    pub room_id: RoomId,
    pub user_id: UserId,
    pub track_id: String,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub sequence: u64,
    pub samples: Vec<f32>,
    pub noise: NoiseCancellationConfig,
}

pub trait AudioFrameProcessor: Send + Sync + 'static {
    fn process(&self, frame: &AudioFrame, noise: &NoiseCancellationConfig) -> Vec<f32>;
}

#[derive(Debug, Default)]
pub struct PassthroughAudioFrameProcessor;

impl AudioFrameProcessor for PassthroughAudioFrameProcessor {
    fn process(&self, frame: &AudioFrame, _noise: &NoiseCancellationConfig) -> Vec<f32> {
        frame.samples.clone()
    }
}

pub trait ProcessedAudioSink: Send + Sync + 'static {
    fn publish(&self, frame: ProcessedAudioFrame);
}

#[derive(Debug)]
pub struct MediaRuntime<P, S> {
    relays: Arc<MediaRelayRegistry>,
    processor: P,
    sink: S,
}

impl<P, S> MediaRuntime<P, S>
where
    P: AudioFrameProcessor,
    S: ProcessedAudioSink,
{
    pub fn new(relays: Arc<MediaRelayRegistry>, processor: P, sink: S) -> Self {
        Self {
            relays,
            processor,
            sink,
        }
    }

    pub fn process_frame(&self, frame: AudioFrame) -> Result<(), MediaRelayError> {
        let lookup = self
            .relays
            .require_track(&frame.room_id, &frame.user_id, &frame.track_id)?;
        if lookup.kind != MediaTrackKind::Audio {
            return Err(MediaRelayError::UnsupportedTrackKind {
                room_id: frame.room_id,
                user_id: frame.user_id,
                track_id: frame.track_id,
                kind: lookup.kind,
            });
        }
        let samples = self.processor.process(&frame, &lookup.noise);
        self.sink.publish(ProcessedAudioFrame {
            room_id: frame.room_id,
            user_id: frame.user_id,
            track_id: frame.track_id,
            sample_rate_hz: frame.sample_rate_hz,
            channels: frame.channels,
            sequence: frame.sequence,
            samples,
            noise: lookup.noise,
        });
        Ok(())
    }
}
```

Tests must include the spec cases and use a `RecordingSink` with `Arc<Mutex<Vec<ProcessedAudioFrame>>>` plus a processor that records the noise config it received and returns transformed samples.

- [x] **Step 2: Export runtime types**

Update `crates/lyre-core/src/lib.rs`:

```rust
pub mod media_runtime;
pub use media_runtime::{
    AudioFrame, AudioFrameProcessor, MediaRuntime, PassthroughAudioFrameProcessor,
    ProcessedAudioFrame, ProcessedAudioSink,
};
```

Also export `MediaRelayTrackLookup` from `media`.

- [x] **Step 3: Run media runtime tests and fix compile issues**

Run:

```bash
cargo test -p lyre-core media_runtime::tests -- --nocapture
```

Expected: pass.

## Task 3: Verification and Implementation Review

**Files:**
- No code edits unless tests fail.

- [x] **Step 1: Run targeted Rust tests**

Run:

```bash
cargo test -p lyre-core media::tests -- --nocapture
cargo test -p lyre-core media_runtime::tests -- --nocapture
cargo test -p lyre-web media_relay_ -- --nocapture
```

Expected: pass.

- [x] **Step 2: Run full Rust verification before implementation review**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: pass.

- [x] **Step 3: Independent implementation review**

Dispatch a fresh independent implementation reviewer with:

- spec path: `docs/superpowers/specs/2026-06-15-server-media-runtime-boundary-design.md`
- plan path: `docs/superpowers/plans/2026-06-15-server-media-runtime-boundary.md`
- diff
- verification output

Required verdict:

```text
VERDICT: APPROVE | REVISE
SPEC_COVERAGE:
- ...
BLOCKERS:
- ...
REQUIRED_CHANGES:
- ...
```

If the reviewer returns `REVISE`, fix the gaps, rerun verification, and re-review.

## Task 4: Documentation and Final Verification

**Files:**
- Modify after implementation review: `README.md`
- Modify after implementation review: `MEMORY.md`
- Modify after implementation review: `docs/roadmap.md`

- [x] **Step 1: Update docs after implementation approval**

Update `README.md` Media Topology section to mention the decoded-PCM media runtime boundary:

- it accepts already-decoded audio frames,
- gates processing on active relay state and registered audio tracks,
- delivers processed frames to an internal sink,
- it is not WebRTC termination, Opus/PCM decode, real broadcast, RNNoise, or DeepFilterNet.

Append to `MEMORY.md`:

```md
## 2026-06-15 Server Media Runtime Boundary

- Added a decoded-PCM media runtime boundary in `lyre-core`.
- Kept `lyre-core` independent of `lyre-noise-cancelling`; future adapters can bridge concrete processors behind the core `AudioFrameProcessor` trait.
- Gated frame processing on active media relay state and registered audio tracks without mutating relay state.
- Kept real WebRTC termination, Opus decode/encode, RNNoise, DeepFilterNet, and real server broadcast as future work.
```

Update `docs/roadmap.md`:

- Add Completed item: decoded-PCM server media runtime boundary with processor/sink traits.
- Keep real WebRTC termination/SFU-like audio pipeline, RNNoise binding, DeepFilterNet binding, and real broadcast in Next.

- [x] **Step 2: Run final verification**

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

Expected: all checks pass; only intended files changed.

- [x] **Step 3: Commit and push attempt**

Stage intended files and commit using Lore protocol:

```text
Define decoded PCM media runtime boundary

Constraint: This increment accepts already-decoded audio frames only; real WebRTC termination, Opus decode/encode, and broadcast remain future work.
Rejected: Depending on lyre-noise-cancelling from lyre-core | That would create a crate dependency cycle.
Confidence: high
Scope-risk: moderate
Directive: Do not mark server-side audio processing active until real media termination feeds this runtime and processed frames are broadcast to clients.
Tested: cargo fmt --all --check; cargo clippy --workspace --all-targets -- -D warnings; cargo nextest run --manifest-path Cargo.toml --workspace; npm run generate:webrpc; npm test -- --run; npm run typecheck; npm run lint; npm run build; git diff --check
Not-tested: Real WebRTC termination, RTP/RTCP, Opus decode/encode, RNNoise, DeepFilterNet, and client broadcast remain future work.
```

Run `git push`. If it fails because no remote is configured, report the local commit SHA and exact push error.
