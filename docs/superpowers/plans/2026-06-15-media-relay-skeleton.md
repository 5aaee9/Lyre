# Media Relay Skeleton Implementation Plan

> **For agentic workers:** REQUIRED WORKFLOW: Execute this plan as a `$sdd-workflow` subtask. Use the SDD reviewer gates before implementation and after implementation. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Add a REST/WebRPC-visible media relay state skeleton for future server-side WebRTC audio processing.

**Architecture:** Keep the current P2P WebRTC runtime unchanged. Add `lyre-core::media` for room-scoped relay state and DTOs, wire it into `lyre-web::AppState`, expose REST endpoints, then update WebRPC RIDL/generated TypeScript to document the new HTTP contract.

**Tech Stack:** Rust, DashMap, Serde, Axum, WebRPC RIDL, generated TypeScript, Vitest.

---

## File Structure

- Create `crates/lyre-core/src/media.rs`: media relay DTOs, registry, errors, and tests.
- Modify `crates/lyre-core/src/lib.rs`: export media relay types.
- Modify `crates/lyre-web/src/error.rs`: map `MediaRelayError::Inactive` to HTTP 409 JSON.
- Modify `crates/lyre-web/src/api.rs`: add media relay registry to `AppState`, routes, handlers, and route tests.
- Modify `proto/lyre.ridl`: add media relay enums/structs/service methods.
- Regenerate `frontend/src/lib/lyre.gen.ts`.
- Modify `frontend/src/lib/api.ts`: add generated enum conversion helpers and REST wrappers for media relay endpoints.
- Modify `frontend/src/lib/api.test.ts`: type/URL/body tests for the new wrappers.
- Post-review docs: `README.md`, `MEMORY.md`, `docs/roadmap.md`.

## Task 1: Core Media Relay State

**Files:**
- Create: `crates/lyre-core/src/media.rs`
- Modify: `crates/lyre-core/src/lib.rs`

**Workflow:** Execute this task under the active `$sdd-workflow`; its changes are reviewed by the final independent implementation reviewer.

- [x] **Step 1: Create media DTOs and registry**

Create `crates/lyre-core/src/media.rs`:

```rust
use crate::{NoiseCancellationConfig, RoomId, UserId};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaRelayMode {
    P2pMesh,
    MediaRelay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaRelayStatus {
    Inactive,
    Active,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaTrackKind {
    Audio,
    Video,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaRelayTrack {
    pub track_id: String,
    pub kind: MediaTrackKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaRelayParticipant {
    pub user_id: UserId,
    pub tracks: Vec<MediaRelayTrack>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaRelayRoomStatus {
    pub room_id: RoomId,
    pub status: MediaRelayStatus,
    pub mode: MediaRelayMode,
    pub server_side_audio_processing: bool,
    pub server_side_noise_cancelling: bool,
    pub noise: NoiseCancellationConfig,
    pub participants: Vec<MediaRelayParticipant>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StartMediaRelayRequest {
    pub noise: Option<NoiseCancellationConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopMediaRelayRequest {
    pub user_id: UserId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterMediaTrackRequest {
    pub user_id: UserId,
    pub track_id: String,
    pub kind: MediaTrackKind,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MediaRelayError {
    #[error("media relay is not active for room `{room_id}`")]
    Inactive { room_id: RoomId },
}

#[derive(Debug, Clone, Default)]
struct MediaRelayRoomState {
    active: bool,
    noise: NoiseCancellationConfig,
    participants: DashMap<UserId, DashMap<String, MediaTrackKind>>,
}

#[derive(Debug, Default)]
pub struct MediaRelayRegistry {
    rooms: DashMap<RoomId, MediaRelayRoomState>,
}

impl MediaRelayRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn status(&self, room_id: RoomId) -> MediaRelayRoomStatus {
        self.rooms.entry(room_id.clone()).or_default();
        self.snapshot(room_id)
    }

    pub fn start(&self, room_id: RoomId, request: StartMediaRelayRequest) -> MediaRelayRoomStatus {
        let mut room = self.rooms.entry(room_id.clone()).or_default();
        room.active = true;
        room.noise = request.noise.unwrap_or_default();
        drop(room);
        self.snapshot(room_id)
    }

    pub fn stop(&self, room_id: RoomId, _request: StopMediaRelayRequest) -> MediaRelayRoomStatus {
        let mut room = self.rooms.entry(room_id.clone()).or_default();
        room.active = false;
        room.noise = NoiseCancellationConfig::default();
        room.participants.clear();
        drop(room);
        self.snapshot(room_id)
    }

    pub fn register_track(
        &self,
        room_id: RoomId,
        request: RegisterMediaTrackRequest,
    ) -> Result<MediaRelayRoomStatus, MediaRelayError> {
        let room = self.rooms.entry(room_id.clone()).or_default();
        if !room.active {
            return Err(MediaRelayError::Inactive { room_id });
        }
        room.participants
            .entry(request.user_id)
            .or_default()
            .insert(request.track_id, request.kind);
        drop(room);
        Ok(self.snapshot(room_id))
    }

    fn snapshot(&self, room_id: RoomId) -> MediaRelayRoomStatus {
        let Some(room) = self.rooms.get(&room_id) else {
            return inactive_status(room_id);
        };
        let active = room.active;
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
        MediaRelayRoomStatus {
            room_id,
            status: if active {
                MediaRelayStatus::Active
            } else {
                MediaRelayStatus::Inactive
            },
            mode: if active {
                MediaRelayMode::MediaRelay
            } else {
                MediaRelayMode::P2pMesh
            },
            server_side_audio_processing: false,
            server_side_noise_cancelling: false,
            noise: room.noise.clone(),
            participants,
        }
    }
}

fn inactive_status(room_id: RoomId) -> MediaRelayRoomStatus {
    MediaRelayRoomStatus {
        room_id,
        status: MediaRelayStatus::Inactive,
        mode: MediaRelayMode::P2pMesh,
        server_side_audio_processing: false,
        server_side_noise_cancelling: false,
        noise: NoiseCancellationConfig::default(),
        participants: Vec::new(),
    }
}
```

- [x] **Step 2: Add core tests**

Add tests in `media.rs` covering:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::NoiseProvider;

    #[test]
    fn default_status_is_inactive() {
        let registry = MediaRelayRegistry::new();
        let status = registry.status(RoomId::default_room());

        assert_eq!(status.status, MediaRelayStatus::Inactive);
        assert_eq!(status.mode, MediaRelayMode::P2pMesh);
        assert!(!status.server_side_audio_processing);
        assert!(!status.server_side_noise_cancelling);
        assert!(status.participants.is_empty());
    }

    #[test]
    fn start_records_default_and_custom_noise() {
        let registry = MediaRelayRegistry::new();
        let default_started = registry.start(RoomId::default_room(), StartMediaRelayRequest::default());
        assert_eq!(default_started.status, MediaRelayStatus::Active);
        assert_eq!(default_started.noise.provider, NoiseProvider::Off);

        let custom = NoiseCancellationConfig {
            provider: NoiseProvider::Rnnoise,
            intensity: 0.8,
            voice_activity_threshold: 0.2,
        };
        let status = registry.start(
            RoomId::default_room(),
            StartMediaRelayRequest {
                noise: Some(custom.clone()),
            },
        );
        assert_eq!(status.noise, custom);
    }

    #[test]
    fn registering_track_requires_active_relay() {
        let registry = MediaRelayRegistry::new();
        let room_id = RoomId::default_room();

        assert_eq!(
            registry.register_track(
                room_id.clone(),
                RegisterMediaTrackRequest {
                    user_id: UserId::from_external("user_01"),
                    track_id: "audio-main".to_owned(),
                    kind: MediaTrackKind::Audio,
                },
            ),
            Err(MediaRelayError::Inactive { room_id })
        );
    }

    #[test]
    fn active_relay_tracks_are_stable_and_replace_same_track() {
        let registry = MediaRelayRegistry::new();
        let room_id = RoomId::default_room();
        registry.start(room_id.clone(), StartMediaRelayRequest::default());

        registry
            .register_track(
                room_id.clone(),
                RegisterMediaTrackRequest {
                    user_id: UserId::from_external("user_b"),
                    track_id: "video-main".to_owned(),
                    kind: MediaTrackKind::Video,
                },
            )
            .unwrap();
        let status = registry
            .register_track(
                room_id.clone(),
                RegisterMediaTrackRequest {
                    user_id: UserId::from_external("user_a"),
                    track_id: "audio-main".to_owned(),
                    kind: MediaTrackKind::Audio,
                },
            )
            .unwrap();

        assert_eq!(status.participants[0].user_id.as_str(), "user_a");
        assert_eq!(status.participants[1].user_id.as_str(), "user_b");
        assert_eq!(status.participants[0].tracks[0].track_id, "audio-main");
    }

    #[test]
    fn stop_clears_participants() {
        let registry = MediaRelayRegistry::new();
        let room_id = RoomId::default_room();
        registry.start(room_id.clone(), StartMediaRelayRequest::default());
        registry
            .register_track(
                room_id.clone(),
                RegisterMediaTrackRequest {
                    user_id: UserId::from_external("user_01"),
                    track_id: "audio-main".to_owned(),
                    kind: MediaTrackKind::Audio,
                },
            )
            .unwrap();

        let status = registry.stop(
            room_id,
            StopMediaRelayRequest {
                user_id: UserId::from_external("user_01"),
            },
        );

        assert_eq!(status.status, MediaRelayStatus::Inactive);
        assert!(status.participants.is_empty());
    }
}
```

- [x] **Step 3: Export module**

Update `crates/lyre-core/src/lib.rs`:

```rust
pub mod media;

pub use media::{
    MediaRelayError, MediaRelayMode, MediaRelayParticipant, MediaRelayRegistry,
    MediaRelayRoomStatus, MediaRelayStatus, MediaRelayTrack, MediaTrackKind,
    RegisterMediaTrackRequest, StartMediaRelayRequest, StopMediaRelayRequest,
};
```

## Task 2: REST API Routes

**Files:**
- Modify: `crates/lyre-web/src/error.rs`
- Modify: `crates/lyre-web/src/api.rs`

**Workflow:** Execute this task under the active `$sdd-workflow`; its changes are reviewed by the final independent implementation reviewer.

- [x] **Step 1: Add API error mapping**

In `error.rs`, import `MediaRelayError`, add `ApiError::MediaRelay(MediaRelayError)`, `From<MediaRelayError>`, and map it:

```rust
Self::MediaRelay(error @ MediaRelayError::Inactive { .. }) => {
    (StatusCode::CONFLICT, error.to_string())
}
```

- [x] **Step 2: Extend AppState**

In `api.rs`, import `MediaRelayRegistry` and add:

```rust
pub media_relays: Arc<MediaRelayRegistry>,
```

Initialize it in `AppState::new`.

- [x] **Step 3: Add routes**

Add routes to `router`:

```rust
.route("/api/rooms/{room_id}/media-relay", get(media_relay_status))
.route("/api/rooms/{room_id}/media-relay/start", post(start_media_relay))
.route("/api/rooms/{room_id}/media-relay/stop", post(stop_media_relay))
.route("/api/rooms/{room_id}/media-relay/tracks", post(register_media_track))
```

Implement handlers:

```rust
async fn media_relay_status(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    Ok(Json(state.media_relays.status(room_id)))
}

async fn start_media_relay(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(request): Json<lyre_core::StartMediaRelayRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    Ok(Json(state.media_relays.start(room_id, request)))
}

async fn stop_media_relay(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(request): Json<lyre_core::StopMediaRelayRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    Ok(Json(state.media_relays.stop(room_id, request)))
}

async fn register_media_track(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(request): Json<lyre_core::RegisterMediaTrackRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let room_id = RoomId::parse_boundary(room_id)?;
    Ok(Json(state.media_relays.register_track(room_id, request)?))
}
```

- [x] **Step 4: Add route tests**

Add tests in `api.rs` with names prefixed by `media_relay_` for:

- `GET /api/rooms/DEFAULT/media-relay` returns inactive status with snake_case fields.
- registering a track before start returns `409` with `{"error":"media relay is not active for room `DEFAULT`"}`.
- start with rnnoise config returns active media relay status and records noise.
- registering an audio track after start returns participant JSON with `user_id`, `track_id`, and `kind`.
- stop returns inactive and empty participants.
- malformed room id on media relay route returns bad request.
- existing `room_routes_join_snapshot_and_leave` still passes unchanged.

## Task 3: WebRPC Contract and Frontend Types

**Files:**
- Modify: `proto/lyre.ridl`
- Regenerate: `frontend/src/lib/lyre.gen.ts`
- Modify: `frontend/src/lib/api.ts`
- Modify: `frontend/src/lib/api.test.ts`

**Workflow:** Execute this task under the active `$sdd-workflow`; its changes are reviewed by the final independent implementation reviewer.

- [x] **Step 1: Add RIDL definitions**

Add the exact enums, structs, and service methods from the approved spec to `proto/lyre.ridl`.

- [x] **Step 2: Regenerate TypeScript client**

Run:

```bash
cd frontend
npm run generate:webrpc
```

- [x] **Step 3: Add frontend type conversions and wrappers**

In `frontend/src/lib/api.ts`, import generated media relay enums with aliases to avoid collisions with REST string literal types:

```ts
import {
  MediaRelayMode as WebrpcMediaRelayMode,
  MediaRelayStatus as WebrpcMediaRelayStatus,
  MediaTrackKind as WebrpcMediaTrackKind
} from "./lyre.gen";
```

Add REST-facing string literal types:

```ts
export type MediaRelayStatus = "inactive" | "active";
export type MediaRelayMode = "p2p_mesh" | "media_relay";
export type MediaTrackKind = "audio" | "video";
```

Add conversion helpers:

```ts
export function generatedMediaRelayStatusToRest(status: WebrpcMediaRelayStatus): MediaRelayStatus {
  switch (status) {
    case WebrpcMediaRelayStatus.ACTIVE:
      return "active";
    case WebrpcMediaRelayStatus.INACTIVE:
      return "inactive";
  }
}

export function generatedMediaRelayModeToRest(mode: WebrpcMediaRelayMode): MediaRelayMode {
  switch (mode) {
    case WebrpcMediaRelayMode.MEDIA_RELAY:
      return "media_relay";
    case WebrpcMediaRelayMode.P2P_MESH:
      return "p2p_mesh";
  }
}

export function generatedMediaTrackKindToRest(kind: WebrpcMediaTrackKind): MediaTrackKind {
  switch (kind) {
    case WebrpcMediaTrackKind.AUDIO:
      return "audio";
    case WebrpcMediaTrackKind.VIDEO:
      return "video";
  }
}
```

Add derived REST types matching snake_case JSON fields:

```ts
export type MediaRelayTrack = {
  track_id: string;
  kind: MediaTrackKind;
};

export type MediaRelayParticipant = {
  user_id: string;
  tracks: MediaRelayTrack[];
};

export type MediaRelayRoomStatus = {
  room_id: string;
  status: MediaRelayStatus;
  mode: MediaRelayMode;
  server_side_audio_processing: boolean;
  server_side_noise_cancelling: boolean;
  noise: NoiseCancellationConfig;
  participants: MediaRelayParticipant[];
};
```

Add wrappers:

```ts
export function mediaRelayUrl(roomId: string): string {
  return `${roomUrl(roomId)}/media-relay`;
}

export async function getMediaRelay(roomId: string): Promise<MediaRelayRoomStatus> {
  const response = await fetch(mediaRelayUrl(roomId));
  return response.json();
}

export async function startMediaRelay(
  roomId: string,
  noise?: NoiseCancellationConfig
): Promise<MediaRelayRoomStatus> {
  const response = await fetch(`${mediaRelayUrl(roomId)}/start`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ noise })
  });
  return response.json();
}

export async function stopMediaRelay(roomId: string, userId: string): Promise<MediaRelayRoomStatus> {
  const response = await fetch(`${mediaRelayUrl(roomId)}/stop`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ user_id: userId })
  });
  return response.json();
}

export async function registerMediaTrack(
  roomId: string,
  userId: string,
  trackId: string,
  kind: MediaTrackKind
): Promise<MediaRelayRoomStatus> {
  const response = await fetch(`${mediaRelayUrl(roomId)}/tracks`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ user_id: userId, track_id: trackId, kind })
  });
  return response.json();
}
```

- [x] **Step 4: Add frontend tests**

In `frontend/src/lib/api.test.ts`, add:

- generated enum/type shape compiles for media relay generated DTOs.
- generated media relay enum conversion helpers map generated uppercase enum values to REST lowercase strings.
- `mediaRelayUrl("Team A")` encodes room ID.
- wrappers call the expected URLs and serialize snake_case REST request bodies.

## Task 4: Verification and Implementation Review

**Files:**
- All changed implementation files.

**Workflow:** Execute this task under the active `$sdd-workflow`; do not update final docs, commit, or push until independent implementation review returns `VERDICT: APPROVE`.

- [x] **Step 1: Run targeted tests**

Run:

```bash
cargo test -p lyre-core media::tests -- --nocapture
cargo test -p lyre-web media_relay_ -- --nocapture
cd frontend && npm test -- --run src/lib/api.test.ts
```

- [x] **Step 2: Run full verification**

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
```

- [x] **Step 3: Request SDD implementation review**

Dispatch a fresh independent reviewer with the approved spec, reviewed plan, diff, and verification output. Require:

```text
VERDICT: APPROVE | REVISE
SPEC_COVERAGE:
- [implemented requirement or missing requirement]
BLOCKERS:
- [blocking gap or "None"]
REQUIRED_CHANGES:
- [change or "None"]
```

## Task 5: Post-Review Docs, Final Verification, Commit

**Files:**
- Modify: `README.md`
- Modify: `MEMORY.md`
- Modify: `docs/roadmap.md`
- Modify: `AGENTS.md`
- Modify: `docs/superpowers/plans/2026-06-15-media-relay-skeleton.md`

**Workflow:** Execute this task only after the independent implementation reviewer in Task 4 returns `VERDICT: APPROVE`.

- [x] **Step 1: Update README**

Document media relay endpoints and state that the relay skeleton does not yet terminate WebRTC media or process audio.

- [x] **Step 2: Update MEMORY**

Append:

```markdown
## 2026-06-15 Media Relay Skeleton

- Added a room-scoped media relay state skeleton with REST and WebRPC contract coverage.
- Kept browser P2P signalling unchanged; this increment does not terminate WebRTC media or process audio.
- Recorded intended noise settings in relay state so future RNNoise/DeepFilterNet processing can attach to the media relay boundary.
```

- [x] **Step 3: Update roadmap**

Move media relay skeleton/API contract to Completed. Keep real WebRTC media termination, RNNoise binding, and DeepFilterNet binding in Next.

- [x] **Step 4: Update AGENTS.md**

Add a short note under project guidance that `lyre-core::media` owns the media relay state skeleton and that real WebRTC media termination/noise processing remains future work. No new crate is added in this increment.

- [x] **Step 5: Final verification**

After docs update, rerun:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
cd frontend
npm test -- --run
npm run typecheck
npm run lint
npm run build
git diff --check
git diff --stat
git diff
git status --short
```

- [x] **Step 6: Commit and push**

Stage intended files, commit with Lore protocol, then run `git push`. If push fails due to missing remote/upstream/credentials, report the exact error with the successful local commit SHA.
