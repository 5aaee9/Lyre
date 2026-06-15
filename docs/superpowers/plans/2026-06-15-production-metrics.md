# Production Metrics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a privacy-preserving Prometheus `/metrics` endpoint to the Rust API server.

**Architecture:** Add aggregate read-only snapshots to `lyre-core`, then compose and render metrics in a focused `lyre-web::metrics` module. `AppState` owns atomic counters and increments them only after successful persisted mutations.

**Tech Stack:** Rust, Axum, atomics, existing `lyre-core` registries, existing `lyre-web` router/tests.

---

## Files

- Modify: `crates/lyre-core/src/room.rs`
  - Add `RoomRegistryAggregate` and `RoomRegistry::aggregate()` without calling `snapshot()`.
  - Change `RoomRegistry::leave()` to return whether a user was removed, using a new response type.
- Modify: `crates/lyre-core/src/lib.rs`
  - Re-export `RoomRegistryAggregate`, `LeaveRoomResponse`, and `MediaRelayRegistryAggregate`.
- Create: `crates/lyre-core/src/room_aggregate_tests.rs`
  - Keep new room aggregate and leave-removal tests out of the already-over-threshold `room.rs`.
- Modify: `crates/lyre-core/src/media_tests.rs`
  - Add the new media relay aggregate test to the existing separate media test file.
- Modify: `crates/lyre-core/src/media.rs`
  - Add `MediaRelayRegistryAggregate` and `MediaRelayRegistry::aggregate()` without creating room entries.
- Modify: `crates/lyre-web/src/api.rs`
  - Add `metrics: Arc<MetricsState>` to `AppState`.
  - Wire `.route("/metrics", get(crate::metrics::metrics))`.
  - Increment counters in `join_room_persisted` and `leave_room_persisted`.
- Create: `crates/lyre-web/src/metrics.rs`
  - Define `MetricsState`, `MetricsSnapshot`, rendering, and Axum handler.
- Create: `crates/lyre-web/src/metrics_tests.rs`
  - Add focused endpoint/counter/non-mutating tests.
- Modify: `crates/lyre-web/src/lib.rs`
  - Register the new metrics module and test module.
- Modify: `crates/lyre-web/src/state_persistence_tests.rs`
  - Extend failed persistence tests to assert persistence failure counters.
- Modify after implementation review: `MEMORY.md`, `docs/roadmap.md`.

## Task 1: Core Aggregate Snapshots

**Files:**
- Modify: `crates/lyre-core/src/room.rs`
- Create: `crates/lyre-core/src/room_aggregate_tests.rs`
- Modify: `crates/lyre-core/src/lib.rs`
- Modify: `crates/lyre-core/src/media.rs`
- Modify: `crates/lyre-core/src/media_tests.rs`

- [ ] **Step 1: Add room aggregate and leave response implementation**

Add near the persisted room structs:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoomRegistryAggregate {
    pub rooms: usize,
    pub users: usize,
}
```

Add near `LeaveRoomRequest`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeaveRoomResponse {
    pub room: RoomSnapshot,
    pub removed: bool,
}
```

Add to `impl RoomRegistry`:

```rust
pub fn aggregate(&self) -> RoomRegistryAggregate {
    RoomRegistryAggregate {
        rooms: self.rooms.len(),
        users: self
            .rooms
            .iter()
            .map(|entry| entry.value().users.len())
            .sum(),
    }
}
```

Change `RoomRegistry::leave` to return `LeaveRoomResponse`:

```rust
pub fn leave(&self, room_id: &RoomId, user_id: &UserId) -> LeaveRoomResponse {
    let removed = self
        .rooms
        .get(room_id)
        .and_then(|room| {
            room.access_tokens.remove(user_id);
            room.users.remove(user_id)
        })
        .is_some();
    LeaveRoomResponse {
        room: self.snapshot(room_id.clone()),
        removed,
    }
}
```

Adjust existing core tests in `room.rs` that call `registry.leave(...)` to read `.room` where they currently expect a `RoomSnapshot`.

Register the test module and re-export new public types in `crates/lyre-core/src/lib.rs`:

```rust
#[cfg(test)]
mod room_aggregate_tests;
```

Update the existing room export list to include:

```rust
LeaveRoomResponse, RoomRegistryAggregate,
```

- [ ] **Step 2: Add room aggregate and leave-response tests**

Create `crates/lyre-core/src/room_aggregate_tests.rs`:

```rust
use crate::{JoinRoomRequest, RoomId, RoomRegistry, RoomRegistryAggregate, UserId};

#[test]
fn aggregate_counts_rooms_and_users_without_creating_rooms() {
    let registry = RoomRegistry::new();

    assert_eq!(
        registry.aggregate(),
        RoomRegistryAggregate {
            rooms: 0,
            users: 0,
        }
    );

    registry.join(
        RoomId::default_room(),
        JoinRoomRequest {
            nickname: Some("Ada".to_owned()),
            noise: None,
        },
    );

    assert_eq!(
        registry.aggregate(),
        RoomRegistryAggregate {
            rooms: 1,
            users: 1,
        }
    );
}

#[test]
fn leave_response_reports_whether_user_was_removed() {
    let registry = RoomRegistry::new();
    let room_id = RoomId::default_room();
    let joined = registry.join(room_id.clone(), JoinRoomRequest::default());

    let removed = registry.leave(&room_id, &joined.user.id);

    assert!(removed.removed);
    assert!(removed.room.users.is_empty());

    let missing = registry.leave(&room_id, &UserId::from_external("missing"));

    assert!(!missing.removed);
}
```

- [ ] **Step 3: Run room aggregate tests**

Run:

```bash
cargo test -p lyre-core room_aggregate_tests
```

Expected: test passes.

- [ ] **Step 4: Add media relay aggregate tests**

Add this test to existing `crates/lyre-core/src/media_tests.rs`:

```rust
#[test]
fn aggregate_counts_only_active_media_relays_without_creating_rooms() {
    let registry = MediaRelayRegistry::new();

    assert_eq!(
        registry.aggregate(),
        MediaRelayRegistryAggregate {
            active_rooms: 0,
            participants: 0,
        }
    );

    registry.start(RoomId::default_room(), StartMediaRelayRequest::default());
    registry
        .register_track(
            RoomId::default_room(),
            RegisterMediaTrackRequest {
                user_id: UserId::from_external("user_a"),
                track_id: "audio".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();

    assert_eq!(
        registry.aggregate(),
        MediaRelayRegistryAggregate {
            active_rooms: 1,
            participants: 1,
        }
    );
}
```

- [ ] **Step 5: Add media relay aggregate implementation**

Add near media relay DTOs:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaRelayRegistryAggregate {
    pub active_rooms: usize,
    pub participants: usize,
}
```

Update the existing media export list in `crates/lyre-core/src/lib.rs` to include:

```rust
MediaRelayRegistryAggregate,
```

Add to `impl MediaRelayRegistry`:

```rust
pub fn aggregate(&self) -> MediaRelayRegistryAggregate {
    self.rooms.iter().fold(
        MediaRelayRegistryAggregate {
            active_rooms: 0,
            participants: 0,
        },
        |mut aggregate, entry| {
            if entry.value().active {
                aggregate.active_rooms += 1;
                aggregate.participants += entry.value().participants.len();
            }
            aggregate
        },
    )
}
```

- [ ] **Step 6: Run core targeted tests**

Run:

```bash
cargo test -p lyre-core room_aggregate_tests
```

Expected: aggregate and leave-response tests pass.

## Task 2: Metrics State, Rendering, and Route

**Files:**
- Create: `crates/lyre-web/src/metrics.rs`
- Modify: `crates/lyre-web/src/api.rs`
- Modify: `crates/lyre-web/src/lib.rs`

- [ ] **Step 1: Add metrics module skeleton**

Create `crates/lyre-web/src/metrics.rs`:

```rust
use crate::api::AppState;
use axum::{
    extract::State,
    http::{header, HeaderValue},
    response::{IntoResponse, Response},
};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct MetricsState {
    joins: AtomicU64,
    leaves: AtomicU64,
    persistence_failures: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricsSnapshot {
    pub rooms: usize,
    pub users: usize,
    pub active_media_relays: usize,
    pub media_relay_participants: usize,
    pub active_server_media_sessions: usize,
    pub server_media_runtime_pumps: usize,
    pub processed_audio_egress_pumps: usize,
    pub joins: u64,
    pub leaves: u64,
    pub persistence_failures: u64,
}

impl MetricsState {
    pub fn record_join(&self) {
        self.joins.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_leave(&self) {
        self.leaves.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_persistence_failure(&self) {
        self.persistence_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub fn counters(&self) -> (u64, u64, u64) {
        (
            self.joins.load(Ordering::Relaxed),
            self.leaves.load(Ordering::Relaxed),
            self.persistence_failures.load(Ordering::Relaxed),
        )
    }
}
```

- [ ] **Step 2: Add render helpers**

Continue in `metrics.rs`:

```rust
pub fn render_metrics(snapshot: MetricsSnapshot) -> String {
    let mut output = String::new();
    write_metric(&mut output, "lyre_rooms_total", "gauge", "Known rooms.", snapshot.rooms);
    write_metric(&mut output, "lyre_users_total", "gauge", "Joined users.", snapshot.users);
    write_metric(
        &mut output,
        "lyre_media_relays_active",
        "gauge",
        "Active media relay rooms.",
        snapshot.active_media_relays,
    );
    write_metric(
        &mut output,
        "lyre_media_relay_participants_total",
        "gauge",
        "Participants in active media relays.",
        snapshot.media_relay_participants,
    );
    write_metric(
        &mut output,
        "lyre_server_media_sessions_active",
        "gauge",
        "Active server media sessions.",
        snapshot.active_server_media_sessions,
    );
    write_metric(
        &mut output,
        "lyre_server_media_runtime_pumps_active",
        "gauge",
        "Active server media runtime pump tasks.",
        snapshot.server_media_runtime_pumps,
    );
    write_metric(
        &mut output,
        "lyre_processed_audio_egress_pumps_active",
        "gauge",
        "Active processed audio WebRTC egress pump tasks.",
        snapshot.processed_audio_egress_pumps,
    );
    write_metric(
        &mut output,
        "lyre_room_joins_total",
        "counter",
        "Successful room joins since process start.",
        snapshot.joins,
    );
    write_metric(
        &mut output,
        "lyre_room_leaves_total",
        "counter",
        "Successful room leaves since process start.",
        snapshot.leaves,
    );
    write_metric(
        &mut output,
        "lyre_room_state_persistence_failures_total",
        "counter",
        "Failed room state persistence writes since process start.",
        snapshot.persistence_failures,
    );
    output
}

fn write_metric(
    output: &mut String,
    name: &str,
    kind: &str,
    help: &str,
    value: impl std::fmt::Display,
) {
    output.push_str("# HELP ");
    output.push_str(name);
    output.push(' ');
    output.push_str(help);
    output.push('\n');
    output.push_str("# TYPE ");
    output.push_str(name);
    output.push(' ');
    output.push_str(kind);
    output.push('\n');
    output.push_str(name);
    output.push(' ');
    output.push_str(&value.to_string());
    output.push('\n');
}
```

- [ ] **Step 3: Add snapshot and handler**

Continue in `metrics.rs`:

```rust
pub fn snapshot(state: &AppState) -> MetricsSnapshot {
    let rooms = state.registry.aggregate();
    let media_relays = state.media_relays.aggregate();
    let (joins, leaves, persistence_failures) = state.metrics.counters();
    MetricsSnapshot {
        rooms: rooms.rooms,
        users: rooms.users,
        active_media_relays: media_relays.active_rooms,
        media_relay_participants: media_relays.participants,
        active_server_media_sessions: state.active_server_media_sessions().len(),
        server_media_runtime_pumps: state.server_media_runtime_pump_count(),
        processed_audio_egress_pumps: state.processed_audio_webrtc_egress_pump_count(),
        joins,
        leaves,
        persistence_failures,
    }
}

pub async fn metrics(State(state): State<AppState>) -> Response {
    let body = render_metrics(snapshot(&state));
    let mut response = body.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; version=0.0.4"),
    );
    response
}
```

- [ ] **Step 4: Wire module and AppState**

In `crates/lyre-web/src/lib.rs`, add:

```rust
pub mod metrics;
```

In `crates/lyre-web/src/api.rs`:

- Add `metrics::MetricsState` to the crate imports.
- Add `pub metrics: Arc<MetricsState>,` to `AppState`.
- Initialize `metrics: Arc::new(MetricsState::default()),`.
- Add `.route("/metrics", get(crate::metrics::metrics))` near `/health`.

- [ ] **Step 5: Expose pump counts outside test cfg**

In `crates/lyre-web/src/api_server_media_state.rs`, remove `#[cfg(test)]` from:

```rust
pub fn server_media_runtime_pump_count(&self) -> usize
pub fn processed_audio_webrtc_egress_pump_count(&self) -> usize
```

Keep `server_media_peer_connection_count` and `server_media_peer_connection_for_test` test-only.

- [ ] **Step 6: Run route compile check**

Run:

```bash
cargo check -p lyre-web --all-targets
```

Expected: compile succeeds.

## Task 3: Counter Semantics

**Files:**
- Modify: `crates/lyre-web/src/api.rs`

- [ ] **Step 1: Increment counters only after success**

In `AppState::join_room_persisted`:

- In no-persistence branch, call `self.metrics.record_join()` after `self.registry.join(...)`.
- In persistence branch, call `self.metrics.record_join()` only after `persistence.save_registry(...)` succeeds.
- On persistence error, call `self.metrics.record_persistence_failure()` before returning the API error.

In `AppState::leave_room_persisted`:

- Return `Ok(response.room)` to preserve current public API.
- In no-persistence branch, call `self.metrics.record_leave()` only when `response.removed` is true.
- In persistence branch, call `self.metrics.record_leave()` only after persistence succeeds and `response.removed` is true.
- On persistence error after attempted mutation, call `self.metrics.record_persistence_failure()` before returning the API error.

- [ ] **Step 2: Run targeted web tests**

Run:

```bash
cargo test -p lyre-web room_routes_join_snapshot_and_leave
```

Expected: relevant tests pass.

## Task 4: Metrics Tests

**Files:**
- Create: `crates/lyre-web/src/metrics_tests.rs`
- Modify: `crates/lyre-web/src/lib.rs`
- Modify: `crates/lyre-web/src/state_persistence_tests.rs`

- [ ] **Step 1: Register metrics tests**

In `crates/lyre-web/src/lib.rs`, add:

```rust
#[cfg(test)]
mod metrics_tests;
```

- [ ] **Step 2: Add endpoint and join/leave tests**

Create `crates/lyre-web/src/metrics_tests.rs` with helpers similar to `api_tests.rs` and tests:

```rust
use crate::api::{router, AppState};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use tower::ServiceExt;

async fn response_text(response: axum::response::Response) -> String {
    String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
}

fn metric_value(body: &str, name: &str) -> u64 {
    body.lines()
        .find_map(|line| {
            let (metric, value) = line.split_once(' ')?;
            (metric == name).then(|| value.parse().unwrap())
        })
        .unwrap()
}

#[tokio::test]
async fn metrics_route_returns_prometheus_text() {
    let app = router(AppState::default());
    let response = app
        .oneshot(Request::builder().uri("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"],
        "text/plain; version=0.0.4"
    );
    let body = response_text(response).await;
    assert!(body.contains("# TYPE lyre_rooms_total gauge"));
    assert!(body.contains("# TYPE lyre_room_joins_total counter"));
    assert!(body.contains("lyre_room_state_persistence_failures_total 0"));
    assert!(!body.contains("DEFAULT"));
    assert!(!body.contains("access_token"));
}

#[tokio::test]
async fn metrics_track_join_and_leave_counts() {
    let app = router(AppState::default());
    let join = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rooms/DEFAULT/join")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"nickname":"Ada"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let join_body: serde_json::Value =
        serde_json::from_slice(&join.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let user_id = join_body["user"]["id"].as_str().unwrap();
    let access_token = join_body["access_token"].as_str().unwrap();

    let before_leave = app
        .clone()
        .oneshot(Request::builder().uri("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let body = response_text(before_leave).await;
    assert_eq!(metric_value(&body, "lyre_rooms_total"), 1);
    assert_eq!(metric_value(&body, "lyre_users_total"), 1);
    assert_eq!(metric_value(&body, "lyre_room_joins_total"), 1);
    assert_eq!(metric_value(&body, "lyre_room_leaves_total"), 0);

    let leave = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rooms/DEFAULT/leave")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {access_token}"))
                .body(Body::from(format!(r#"{{"user_id":"{user_id}"}}"#)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(leave.status(), StatusCode::OK);

    let after_leave = app
        .oneshot(Request::builder().uri("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let body = response_text(after_leave).await;
    assert_eq!(metric_value(&body, "lyre_rooms_total"), 1);
    assert_eq!(metric_value(&body, "lyre_users_total"), 0);
    assert_eq!(metric_value(&body, "lyre_room_joins_total"), 1);
    assert_eq!(metric_value(&body, "lyre_room_leaves_total"), 1);
}
```

- [ ] **Step 3: Add non-mutating metrics test**

Continue in `metrics_tests.rs`:

```rust
#[tokio::test]
async fn metrics_route_does_not_create_room_entries() {
    let state = AppState::default();
    let app = router(state.clone());
    let response = app
        .oneshot(Request::builder().uri("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(state.registry.aggregate().rooms, 0);
}
```

- [ ] **Step 4: Extend persistence failure tests**

In `crates/lyre-web/src/state_persistence_tests.rs`:

- In `failed_persisted_join_rolls_back_user_without_token_response`, after the failed join response, call the metrics route or `crate::metrics::snapshot(&state)` and assert:

```rust
let metrics = crate::metrics::snapshot(&state);
assert_eq!(metrics.joins, 0);
assert_eq!(metrics.persistence_failures, 1);
```

- In `failed_persisted_leave_rolls_back_user_and_token`, after the failed leave response, assert:

```rust
let metrics = crate::metrics::snapshot(&state);
assert_eq!(metrics.leaves, 0);
assert_eq!(metrics.persistence_failures, 1);
```

- [ ] **Step 5: Run targeted metrics tests**

Run:

```bash
cargo test -p lyre-web metrics
cargo test -p lyre-web state_persistence_tests::failed_persisted_join_rolls_back_user_without_token_response state_persistence_tests::failed_persisted_leave_rolls_back_user_and_token
```

Expected: metrics and persistence counter tests pass. If cargo rejects multiple exact filters, run each persistence test separately.

## Task 5: Documentation and Full Verification

**Files:**
- Modify: `MEMORY.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update MEMORY.md**

Add:

```markdown
## 2026-06-15 Production Metrics

- Added a Prometheus-compatible `/metrics` endpoint to the Rust API server.
- Kept metrics aggregate and process-local: no room IDs, user IDs, access tokens, nicknames, SDP, ICE candidates, RTP payloads, or persistence paths appear in metrics output.
- Used read-only registry aggregate snapshots so scraping metrics does not create room or media relay state.
- Counted joins/leaves only after successful in-memory or persisted mutations; failed persistence writes increment a separate process-local counter after rollback.
```

- [ ] **Step 2: Update docs/roadmap.md**

Move `Add production observability and metrics.` from `Next` to `Completed`, and optionally leave richer metrics/tracing as future work only if a concrete need exists.

- [ ] **Step 3: Run full verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
git diff --check
```

Expected: all pass.
