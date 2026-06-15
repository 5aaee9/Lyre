# Room State Persistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Add optional JSON file persistence for anonymous room users and access tokens so browser room sessions can survive an API restart.

**Architecture:** `lyre-core` owns persisted registry DTOs and conversion between in-memory room state and persisted snapshots. `lyre-web` owns filesystem I/O and serializes persisted room mutations with an async mutex. `lyre-app` exposes a `--state-file` / `LYRE_STATE_FILE` config path and passes it into `ServeConfig`.

**Tech Stack:** Rust 2021, serde/serde_json, std filesystem I/O, tokio sync mutex, Axum tests, clap env parsing.

---

## File Structure

- Create `crates/lyre-core/src/room_persistence.rs`: persisted DTOs, conversion helpers, and focused tests.
- Modify `crates/lyre-core/src/ids.rs`: route JSON deserialization for `RoomId` through `RoomId::parse_boundary`.
- Modify `crates/lyre-core/src/room.rs`: expose internal snapshot/restore hooks and persisted DTO integration without growing room persistence tests in this file.
- Modify `crates/lyre-core/src/lib.rs`: export persisted registry DTOs and errors.
- Create `crates/lyre-web/src/state_persistence.rs`: optional state file loader/saver with atomic writes.
- Modify `crates/lyre-web/src/lib.rs`: include `state_persistence` module and tests.
- Modify `crates/lyre-web/src/api.rs`: attach persistence to `AppState`, serialize persisted join/leave, rollback on write failure, and log persistence failures with full error chains.
- Modify `crates/lyre-web/src/error.rs`: add generic persistence error response that does not expose state-file paths or lower-level details.
- Modify `crates/lyre-web/src/server.rs`: load persisted state before router construction when configured.
- Create `crates/lyre-web/src/state_persistence_tests.rs`: web/API persistence integration tests.
- Modify `crates/lyre-app/src/cli.rs`: parse state file config and add tests.
- Modify `crates/lyre-app/src/main.rs`: pass state file into `ServeConfig`.
- Modify `MEMORY.md` and `docs/roadmap.md` after implementation review approval.

## Task 1: Core Persisted Room Registry DTOs

**Files:**
- Create: `crates/lyre-core/src/room_persistence.rs`
- Modify: `crates/lyre-core/src/ids.rs`
- Modify: `crates/lyre-core/src/room.rs`
- Modify: `crates/lyre-core/src/lib.rs`

- [x] **Step 1: Add the new module export**

In `crates/lyre-core/src/lib.rs`, add the module:

```rust
pub mod room_persistence;
```

Extend the existing `pub use room::{...};` block with:

```rust
PersistedRoom,
PersistedRoomRegistry,
PersistedRoomRegistryError,
PersistedRoomUser,
```

- [x] **Step 2: Add persisted DTOs and tests**

Create `crates/lyre-core/src/room_persistence.rs` with:

```rust
use crate::{
    JoinRoomRequest, PersistedRoom, PersistedRoomRegistry, PersistedRoomUser, RoomAccessToken,
    RoomId, RoomRegistry, UserId, UserProfile,
};
use chrono::{TimeZone, Utc};

fn persisted_user(
    user_id: &str,
    nickname: &str,
    access_token: &str,
) -> PersistedRoomUser {
    PersistedRoomUser {
        profile: UserProfile {
            id: UserId::from_external(user_id),
            nickname: nickname.to_owned(),
            joined_at: Utc.with_ymd_and_hms(2026, 6, 15, 0, 0, 0).unwrap(),
            noise: Default::default(),
        },
        access_token: RoomAccessToken::from_external(access_token),
    }
}

#[test]
fn empty_registry_exports_no_rooms() {
    let registry = RoomRegistry::new();

    assert!(registry.to_persisted().rooms.is_empty());
}

#[test]
fn joined_users_export_with_access_tokens() {
    let registry = RoomRegistry::new();
    let room_id = RoomId::default_room();
    let joined = registry.join(
        room_id.clone(),
        JoinRoomRequest {
            nickname: Some("Ada".to_owned()),
            noise: None,
        },
    );

    let persisted = registry.to_persisted();

    assert_eq!(persisted.rooms.len(), 1);
    assert_eq!(persisted.rooms[0].room_id, room_id);
    assert_eq!(persisted.rooms[0].users[0].profile.id, joined.user.id);
    assert_eq!(
        persisted.rooms[0].users[0].access_token,
        joined.access_token
    );
}

#[test]
fn restored_state_keeps_tokens_out_of_public_snapshot() {
    let registry = RoomRegistry::from_persisted(PersistedRoomRegistry {
        rooms: vec![PersistedRoom {
            room_id: RoomId::default_room(),
            users: vec![persisted_user("user_a", "Ada", "token_a")],
        }],
    });

    let snapshot = registry.snapshot(RoomId::default_room());
    let json = serde_json::to_value(snapshot).unwrap();

    assert_eq!(json["users"][0]["id"], "user_a");
    assert!(!json.to_string().contains("token_a"));
    assert!(!json.to_string().contains("access_token"));
}

#[test]
fn restored_access_token_validates_room_and_user() {
    let registry = RoomRegistry::from_persisted(PersistedRoomRegistry {
        rooms: vec![PersistedRoom {
            room_id: RoomId::default_room(),
            users: vec![persisted_user("user_a", "Ada", "token_a")],
        }],
    });

    assert!(registry
        .validate_access_token(
            &RoomId::default_room(),
            &UserId::from_external("user_a"),
            &RoomAccessToken::from_external("token_a"),
        )
        .is_ok());
    assert!(registry
        .validate_access_token(
            &RoomId::default_room(),
            &UserId::from_external("user_b"),
            &RoomAccessToken::from_external("token_a"),
        )
        .is_err());
}

#[test]
fn duplicate_persisted_users_use_last_entry() {
    let registry = RoomRegistry::from_persisted(PersistedRoomRegistry {
        rooms: vec![PersistedRoom {
            room_id: RoomId::default_room(),
            users: vec![
                persisted_user("user_a", "Old", "old_token"),
                persisted_user("user_a", "New", "new_token"),
            ],
        }],
    });

    let snapshot = registry.snapshot(RoomId::default_room());

    assert_eq!(snapshot.users.len(), 1);
    assert_eq!(snapshot.users[0].nickname, "New");
    assert!(registry
        .validate_access_token(
            &RoomId::default_room(),
            &UserId::from_external("user_a"),
            &RoomAccessToken::from_external("new_token"),
        )
        .is_ok());
    assert!(registry
        .validate_access_token(
            &RoomId::default_room(),
            &UserId::from_external("user_a"),
            &RoomAccessToken::from_external("old_token"),
        )
        .is_err());
}

#[test]
fn persisted_room_id_deserialization_rejects_blank() {
    let error = serde_json::from_str::<PersistedRoomRegistry>(
        r#"{"rooms":[{"room_id":" ","users":[]}]}"#,
    )
    .unwrap_err();

    assert!(error.to_string().contains("room id must not be blank"));
}
```

- [x] **Step 3: Run the new tests and verify failure**

Run:

```bash
cargo test -p lyre-core room_persistence
```

Expected: compile failures for missing persisted types and methods.

- [x] **Step 4: Make `RoomId` deserialization use the boundary parser**

In `crates/lyre-core/src/ids.rs`, replace the `RoomId` derive:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct RoomId(String);
```

Add:

```rust
impl<'de> Deserialize<'de> for RoomId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse_boundary(value).map_err(serde::de::Error::custom)
    }
}
```

Keep `UserId` as derived `Deserialize`; persisted user ids remain opaque strings.

- [x] **Step 5: Implement persisted DTOs in `room.rs`**

Add near the existing room DTOs in `crates/lyre-core/src/room.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedRoomRegistry {
    pub rooms: Vec<PersistedRoom>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedRoom {
    pub room_id: RoomId,
    pub users: Vec<PersistedRoomUser>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedRoomUser {
    pub profile: UserProfile,
    pub access_token: RoomAccessToken,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PersistedRoomRegistryError {
    #[error("persisted room id is invalid")]
    InvalidRoomId(#[from] crate::RoomIdError),
}
```

Add methods to `impl RoomRegistry`:

```rust
pub fn from_persisted(persisted: PersistedRoomRegistry) -> Self {
    let registry = Self::new();
    registry.replace_with_persisted(persisted);
    registry
}

pub fn to_persisted(&self) -> PersistedRoomRegistry {
    let mut rooms = self
        .rooms
        .iter()
        .map(|room_entry| {
            let mut users = room_entry
                .value()
                .users
                .iter()
                .filter_map(|user_entry| {
                    let access_token = room_entry
                        .value()
                        .access_tokens
                        .get(user_entry.key())
                        .map(|token| token.value().clone())?;
                    Some(PersistedRoomUser {
                        profile: user_entry.value().clone(),
                        access_token,
                    })
                })
                .collect::<Vec<_>>();
            users.sort_by(|left, right| {
                left.profile
                    .nickname
                    .cmp(&right.profile.nickname)
                    .then(left.profile.id.cmp(&right.profile.id))
            });
            PersistedRoom {
                room_id: room_entry.key().clone(),
                users,
            }
        })
        .collect::<Vec<_>>();
    rooms.sort_by(|left, right| left.room_id.cmp(&right.room_id));
    PersistedRoomRegistry { rooms }
}

pub fn replace_with_persisted(&self, persisted: PersistedRoomRegistry) {
    self.rooms.clear();
    for persisted_room in persisted.rooms {
        let room = self.rooms.entry(persisted_room.room_id).or_default();
        for persisted_user in persisted_room.users {
            let user_id = persisted_user.profile.id.clone();
            room.users.insert(user_id.clone(), persisted_user.profile);
            room.access_tokens
                .insert(user_id, persisted_user.access_token);
        }
    }
}
```

- [x] **Step 6: Run core tests**

Run:

```bash
cargo test -p lyre-core room_persistence
cargo test -p lyre-core room::tests
```

Expected: all selected tests pass.

## Task 2: Web State File Persistence

**Files:**
- Create: `crates/lyre-web/src/state_persistence.rs`
- Modify: `crates/lyre-web/src/lib.rs`
- Modify: `crates/lyre-web/src/error.rs`

- [x] **Step 1: Add module and failing unit tests**

In `crates/lyre-web/src/lib.rs`, add:

```rust
pub mod state_persistence;
```

Create `crates/lyre-web/src/state_persistence.rs` with tests first:

```rust
use anyhow::{Context, Result};
use lyre_core::PersistedRoomRegistry;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Debug, Clone)]
pub struct RoomStatePersistence {
    path: PathBuf,
    #[cfg(test)]
    fail_writes: bool,
}

static WRITE_COUNTER: AtomicU64 = AtomicU64::new(0);

impl RoomStatePersistence {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            #[cfg(test)]
            fail_writes: false,
        }
    }

    #[cfg(test)]
    pub fn always_fail_for_tests(path: PathBuf) -> Self {
        Self {
            path,
            fail_writes: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyre_core::{JoinRoomRequest, RoomId, RoomRegistry};

    fn unique_state_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "lyre-{name}-{}-{}.json",
            std::process::id(),
            WRITE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn missing_state_file_loads_empty_registry() {
        let persistence = RoomStatePersistence::new(unique_state_path("missing"));

        let registry = persistence.load_registry().unwrap();

        assert!(registry.to_persisted().rooms.is_empty());
    }

    #[test]
    fn missing_parent_directory_is_startup_error_with_context() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "lyre-missing-startup-parent-{}-{}",
            std::process::id(),
            WRITE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        path.push("state.json");
        let persistence = RoomStatePersistence::new(path);

        let error = persistence.load_registry().unwrap_err();

        assert!(format!("{error:#}").contains("Lyre room state parent directory does not exist"));
    }

    #[test]
    fn save_and_load_registry_round_trips_access_tokens() {
        let path = unique_state_path("roundtrip");
        let persistence = RoomStatePersistence::new(path.clone());
        let registry = RoomRegistry::new();
        let joined = registry.join(RoomId::default_room(), JoinRoomRequest::default());

        persistence.save_registry(&registry).unwrap();
        let restored = persistence.load_registry().unwrap();

        assert!(restored
            .validate_access_token(
                &RoomId::default_room(),
                &joined.user.id,
                &joined.access_token,
            )
            .is_ok());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn malformed_state_file_preserves_parse_context() {
        let path = unique_state_path("malformed");
        std::fs::write(&path, "{not json").unwrap();
        let persistence = RoomStatePersistence::new(path.clone());

        let error = persistence.load_registry().unwrap_err();

        assert!(format!("{error:#}").contains("failed to parse Lyre room state"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn missing_parent_directory_is_save_error_with_context() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "lyre-missing-parent-{}-{}",
            std::process::id(),
            WRITE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        path.push("state.json");
        let persistence = RoomStatePersistence::new(path);
        let registry = RoomRegistry::new();

        let error = persistence.save_registry(&registry).unwrap_err();

        assert!(format!("{error:#}").contains("failed to write Lyre room state"));
    }
}
```

- [x] **Step 2: Run tests and verify failure**

Run:

```bash
cargo test -p lyre-web state_persistence
```

Expected: compile failures for missing `load_registry` and `save_registry`.

- [x] **Step 3: Implement file I/O**

Add these methods to `RoomStatePersistence`:

```rust
pub fn load_registry(&self) -> Result<lyre_core::RoomRegistry> {
    if !self.path.exists() {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                anyhow::bail!(
                    "Lyre room state parent directory does not exist: {}",
                    parent.display()
                );
            }
        }
        return Ok(lyre_core::RoomRegistry::new());
    }
    let bytes = std::fs::read(&self.path)
        .with_context(|| format!("failed to read Lyre room state at {}", self.path.display()))?;
    let persisted: PersistedRoomRegistry = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse Lyre room state at {}", self.path.display()))?;
    Ok(lyre_core::RoomRegistry::from_persisted(persisted))
}

pub fn save_registry(&self, registry: &lyre_core::RoomRegistry) -> Result<()> {
    #[cfg(test)]
    if self.fail_writes {
        anyhow::bail!("forced Lyre room state write failure for tests");
    }
    let persisted = registry.to_persisted();
    let bytes = serde_json::to_vec_pretty(&persisted)
        .context("failed to serialize Lyre room state")?;
    let temp_path = self.temp_path();
    std::fs::write(&temp_path, bytes).with_context(|| {
        format!(
            "failed to write Lyre room state temporary file at {}",
            temp_path.display()
        )
    })?;
    std::fs::rename(&temp_path, &self.path).with_context(|| {
        format!(
            "failed to replace Lyre room state at {}",
            self.path.display()
        )
    })?;
    Ok(())
}

fn temp_path(&self) -> PathBuf {
    let counter = WRITE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = self
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("lyre-state.json");
    self.path.with_file_name(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        counter
    ))
}
```

- [x] **Step 4: Add API persistence error**

In `crates/lyre-web/src/error.rs`, add:

```rust
Persistence(anyhow::Error),
```

to `ApiError`, and in `IntoResponse` match:

```rust
Self::Persistence(error) => {
    tracing::error!(error = %format!("{error:#}"), "room state persistence failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "room state persistence failed".to_owned(),
    )
}
```

Add:

```rust
impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self::Persistence(error)
    }
}
```

- [x] **Step 5: Run web persistence unit tests**

Run:

```bash
cargo test -p lyre-web state_persistence
```

Expected: all selected tests pass.

## Task 3: API Integration and Rollback

**Files:**
- Modify: `crates/lyre-web/src/api.rs`
- Create: `crates/lyre-web/src/state_persistence_tests.rs`
- Modify: `crates/lyre-web/src/lib.rs`

- [x] **Step 1: Add integration test module**

In `crates/lyre-web/src/lib.rs`, add under the existing test modules:

```rust
#[cfg(test)]
mod state_persistence_tests;
```

Create `crates/lyre-web/src/state_persistence_tests.rs`:

```rust
use crate::{
    api::{router, AppState},
    state_persistence::RoomStatePersistence,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use lyre_core::{PersistedRoom, PersistedRoomRegistry, PersistedRoomUser, RoomAccessToken, RoomId, UserId, UserProfile};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use tower::ServiceExt;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_state_path(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "lyre-api-{name}-{}-{}.json",
        std::process::id(),
        TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&path);
    path
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn persisted_user(user_id: &str, token: &str) -> PersistedRoomUser {
    PersistedRoomUser {
        profile: UserProfile {
            id: UserId::from_external(user_id),
            nickname: "Ada".to_owned(),
            joined_at: chrono::Utc::now(),
            noise: Default::default(),
        },
        access_token: RoomAccessToken::from_external(token),
    }
}

#[tokio::test]
async fn state_file_load_makes_persisted_users_visible() {
    let path = unique_state_path("load");
    std::fs::write(
        &path,
        serde_json::to_vec(&PersistedRoomRegistry {
            rooms: vec![PersistedRoom {
                room_id: RoomId::default_room(),
                users: vec![persisted_user("user_a", "token_a")],
            }],
        })
        .unwrap(),
    )
    .unwrap();
    let state = AppState::with_room_state_persistence(
        Default::default(),
        None,
        Some(RoomStatePersistence::new(path.clone())),
    )
    .unwrap();
    let app = router(state);

    let response = app
        .oneshot(Request::builder().uri("/api/rooms/DEFAULT").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let body = body_json(response).await;
    assert_eq!(body["users"][0]["id"], "user_a");
    assert!(!body.to_string().contains("token_a"));
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn restored_token_authorizes_leave_and_rewrites_state_file() {
    let path = unique_state_path("leave");
    std::fs::write(
        &path,
        serde_json::to_vec(&PersistedRoomRegistry {
            rooms: vec![PersistedRoom {
                room_id: RoomId::default_room(),
                users: vec![persisted_user("user_a", "token_a")],
            }],
        })
        .unwrap(),
    )
    .unwrap();
    let state = AppState::with_room_state_persistence(
        Default::default(),
        None,
        Some(RoomStatePersistence::new(path.clone())),
    )
    .unwrap();
    let app = router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rooms/DEFAULT/leave")
                .header("content-type", "application/json")
                .header("authorization", "Bearer token_a")
                .body(Body::from(r#"{"user_id":"user_a"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let file = std::fs::read_to_string(&path).unwrap();
    assert!(!file.contains("user_a"));
    assert!(!file.contains("token_a"));
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn successful_join_writes_user_and_token_to_state_file() {
    let path = unique_state_path("join");
    let state = AppState::with_room_state_persistence(
        Default::default(),
        None,
        Some(RoomStatePersistence::new(path.clone())),
    )
    .unwrap();
    let app = router(state);

    let response = app
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

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = body_json(response).await;
    let file = std::fs::read_to_string(&path).unwrap();
    assert!(file.contains(body["user"]["id"].as_str().unwrap()));
    assert!(file.contains(body["access_token"].as_str().unwrap()));
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn failed_persisted_join_rolls_back_user_without_token_response() {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "lyre-missing-parent-{}-{}",
        std::process::id(),
        TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    path.push("state.json");
    let path_text = path.display().to_string();
    let state = AppState::with_room_state_persistence(
        Default::default(),
        None,
        Some(RoomStatePersistence::new(path.clone())),
    )
    .unwrap();
    let app = router(state.clone());

    let response = app
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

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = body_json(response).await;
    assert_eq!(body["error"], "room state persistence failed");
    assert!(!body.to_string().contains("access_token"));
    assert!(!body.to_string().contains(&path_text));
    assert!(!body.to_string().contains(".tmp"));
    assert!(!body.to_string().contains("No such file"));
    assert!(!body.to_string().contains("failed to write"));
    assert!(state.registry.snapshot(RoomId::default_room()).users.is_empty());
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn failed_persisted_leave_rolls_back_user_and_token() {
    let path = unique_state_path("leave-rollback");
    std::fs::write(
        &path,
        serde_json::to_vec(&PersistedRoomRegistry {
            rooms: vec![PersistedRoom {
                room_id: RoomId::default_room(),
                users: vec![persisted_user("user_a", "token_a")],
            }],
        })
        .unwrap(),
    )
    .unwrap();
    let bad_path = unique_state_path("bad-leave-rollback");
    let state = AppState::with_room_state_persistence(
        Default::default(),
        None,
        Some(RoomStatePersistence::new(path.clone())),
    )
    .unwrap();
    state
        .set_room_state_persistence_for_tests(Some(
            RoomStatePersistence::always_fail_for_tests(bad_path.clone()),
        ))
        .await;
    let app = router(state.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rooms/DEFAULT/leave")
                .header("content-type", "application/json")
                .header("authorization", "Bearer token_a")
                .body(Body::from(r#"{"user_id":"user_a"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = body_json(response).await;
    assert_eq!(body["error"], "room state persistence failed");
    assert!(!body.to_string().contains("forced Lyre room state write failure"));
    assert!(state
        .registry
        .validate_access_token(
            &RoomId::default_room(),
            &UserId::from_external("user_a"),
            &RoomAccessToken::from_external("token_a"),
        )
        .is_ok());
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(bad_path);
}

#[test]
fn malformed_state_file_fails_state_construction_with_context() {
    let path = unique_state_path("malformed-api");
    std::fs::write(&path, "{not json").unwrap();

    let error = AppState::with_room_state_persistence(
        Default::default(),
        None,
        Some(RoomStatePersistence::new(path.clone())),
    )
    .unwrap_err();

    assert!(format!("{error:#}").contains("failed to parse Lyre room state"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn missing_parent_state_file_fails_state_construction_with_context() {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "lyre-missing-startup-parent-{}-{}",
        std::process::id(),
        TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    path.push("state.json");

    let error = AppState::with_room_state_persistence(
        Default::default(),
        None,
        Some(RoomStatePersistence::new(path)),
    )
    .unwrap_err();

    assert!(format!("{error:#}").contains("Lyre room state parent directory does not exist"));
}
```

- [x] **Step 2: Run integration tests and verify failure**

Run:

```bash
cargo test -p lyre-web state_persistence_tests
```

Expected: compile failures for missing `AppState::with_room_state_persistence`, `room_state_persistence`, and mutation integration.

- [x] **Step 3: Extend `AppState`**

In `crates/lyre-web/src/api.rs`, import:

```rust
use crate::state_persistence::RoomStatePersistence;
use tokio::sync::{broadcast, mpsc, Mutex};
```

Add fields to `AppState`:

```rust
room_state_persistence: Arc<Mutex<Option<RoomStatePersistence>>>,
pub room_state_persistence_lock: Arc<Mutex<()>>,
```

Provide this test-only helper:

```rust
#[cfg(test)]
pub async fn set_room_state_persistence_for_tests(
    &self,
    persistence: Option<RoomStatePersistence>,
) {
    *self.room_state_persistence.lock().await = persistence;
}
```

Change `AppState::new` to call:

```rust
Self::with_room_state_persistence(ice_servers, turn_rest_credentials, None)
    .expect("in-memory AppState construction must not fail")
```

Add:

```rust
pub fn with_room_state_persistence(
    ice_servers: Vec<IceServerConfig>,
    turn_rest_credentials: Option<lyre_core::TurnRestCredentialsConfig>,
    room_state_persistence: Option<RoomStatePersistence>,
) -> anyhow::Result<Self> {
    let registry = match &room_state_persistence {
        Some(persistence) => persistence.load_registry()?,
        None => RoomRegistry::new(),
    };
    let media_relays = Arc::new(MediaRelayRegistry::new());
    let server_media_sessions = Arc::new(ServerMediaSessionRegistry::new());
    let server_media_negotiator = Arc::new(ServerMediaNegotiator::new(
        WebRtcStack::new(),
        Arc::clone(&server_media_sessions),
    ));
    let media_runtime = Arc::new(WebMediaRuntime::new(Arc::clone(&media_relays)));
    let server_media_runtime_pump = Arc::new(ServerMediaRuntimePump::new(
        Arc::clone(&media_runtime),
        Arc::clone(&server_media_negotiator),
    ));
    let media_egress = Arc::new(ProcessedAudioEgressFanout::new(Arc::clone(&media_relays)));
    let processed_audio_webrtc_egress_pump = Arc::new(ProcessedAudioWebRtcEgressPump::new(
        Arc::clone(&media_runtime),
        Arc::clone(&media_egress),
        Arc::clone(&server_media_negotiator),
    ));
    Ok(Self {
        registry: Arc::new(registry),
        media_runtime,
        media_egress,
        processed_audio_webrtc_egress_pump,
        server_media_sessions,
        server_media_negotiator,
        server_media_runtime_pump,
        media_relays,
        peers: Arc::new(PeerHub::new()),
        ice_servers: Arc::new(ice_servers),
        turn_rest_credentials,
        room_state_persistence: Arc::new(Mutex::new(room_state_persistence)),
        room_state_persistence_lock: Arc::new(Mutex::new(())),
    })
}
```

- [x] **Step 4: Add persisted mutation helpers**

In `impl AppState`, add:

```rust
pub async fn join_room_persisted(
    &self,
    room_id: RoomId,
    request: JoinRoomRequest,
) -> Result<lyre_core::JoinRoomResponse, ApiError> {
    let _guard = self.room_state_persistence_lock.lock().await;
    let persistence = self.room_state_persistence.lock().await.clone();
    let Some(persistence) = persistence else {
        return Ok(self.registry.join(room_id, request));
    };
    let rollback = self.registry.to_persisted();
    let response = self.registry.join(room_id, request);
    if let Err(error) = persistence.save_registry(&self.registry) {
        self.registry.replace_with_persisted(rollback);
        return Err(ApiError::from(error));
    }
    Ok(response)
}

pub async fn leave_room_persisted(
    &self,
    room_id: &RoomId,
    user_id: &lyre_core::UserId,
) -> Result<lyre_core::RoomSnapshot, ApiError> {
    let _guard = self.room_state_persistence_lock.lock().await;
    let persistence = self.room_state_persistence.lock().await.clone();
    let Some(persistence) = persistence else {
        return Ok(self.registry.leave(room_id, user_id));
    };
    let rollback = self.registry.to_persisted();
    let snapshot = self.registry.leave(room_id, user_id);
    if let Err(error) = persistence.save_registry(&self.registry) {
        self.registry.replace_with_persisted(rollback);
        return Err(ApiError::from(error));
    }
    Ok(snapshot)
}
```

- [x] **Step 5: Use persisted helpers in routes**

In `join_room`, replace:

```rust
let response = state.registry.join(room_id.clone(), request);
state.peers.user_joined(&room_id, response.user.clone());
Ok((StatusCode::CREATED, Json(response)))
```

with:

```rust
let response = state.join_room_persisted(room_id.clone(), request).await?;
state.peers.user_joined(&room_id, response.user.clone());
Ok((StatusCode::CREATED, Json(response)))
```

In `leave_room`, replace:

```rust
let snapshot = state.registry.leave(&room_id, &request.user_id);
state.peers.user_left(&room_id, &request.user_id);
Ok(Json(snapshot))
```

with:

```rust
let snapshot = state
    .leave_room_persisted(&room_id, &request.user_id)
    .await?;
state.peers.user_left(&room_id, &request.user_id);
Ok(Json(snapshot))
```

- [x] **Step 6: Run API persistence tests**

Run:

```bash
cargo test -p lyre-web state_persistence_tests
cargo test -p lyre-web api_tests::room_routes_join_snapshot_and_leave
```

Expected: all selected tests pass.

## Task 4: CLI and Server Configuration

**Files:**
- Modify: `crates/lyre-app/src/cli.rs`
- Modify: `crates/lyre-app/src/main.rs`
- Modify: `crates/lyre-web/src/server.rs`

- [x] **Step 1: Add failing CLI tests**

In `crates/lyre-app/src/cli.rs`, update `default_serve_args()` to include a `state_file: None` field after adding the field in the next step.

Add tests:

```rust
#[test]
fn parses_state_file_cli_arg() {
    let cli = Cli::try_parse_from(["lyre", "serve", "--state-file", "/tmp/lyre-state.json"]).unwrap();
    match cli.command {
        Commands::Serve(args) => {
            assert_eq!(
                args.effective_state_file().unwrap().unwrap(),
                std::path::PathBuf::from("/tmp/lyre-state.json")
            );
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn state_file_env_enables_persistence() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var("LYRE_STATE_FILE", "/tmp/lyre-env-state.json");
    let cli = Cli::try_parse_from(["lyre", "serve"]).unwrap();
    match cli.command {
        Commands::Serve(args) => {
            assert_eq!(
                args.effective_state_file().unwrap().unwrap(),
                std::path::PathBuf::from("/tmp/lyre-env-state.json")
            );
        }
        Commands::Config(_) => panic!("expected serve"),
    }
    std::env::remove_var("LYRE_STATE_FILE");
}

#[test]
fn state_file_cli_takes_precedence_over_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var("LYRE_STATE_FILE", "/tmp/lyre-env-state.json");
    let cli = Cli::try_parse_from([
        "lyre",
        "serve",
        "--state-file",
        "/tmp/lyre-cli-state.json",
    ])
    .unwrap();
    match cli.command {
        Commands::Serve(args) => {
            assert_eq!(
                args.effective_state_file().unwrap().unwrap(),
                std::path::PathBuf::from("/tmp/lyre-cli-state.json")
            );
        }
        Commands::Config(_) => panic!("expected serve"),
    }
    std::env::remove_var("LYRE_STATE_FILE");
}

#[test]
fn rejects_blank_state_file_path() {
    let cli = Cli::try_parse_from(["lyre", "serve", "--state-file", " "]).unwrap();
    match cli.command {
        Commands::Serve(args) => {
            assert_eq!(
                args.effective_state_file(),
                Err(StateFileConfigError::BlankPath)
            );
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}
```

- [x] **Step 2: Run CLI tests and verify failure**

Run:

```bash
cargo test -p lyre-app state_file
```

Expected: compile failures for missing `state_file`, `effective_state_file`, and `StateFileConfigError`.

- [x] **Step 3: Implement CLI state file config**

In `crates/lyre-app/src/cli.rs`, add imports:

```rust
use std::{env, path::PathBuf};
```

Replace the existing `use std::env;`.

Add to `ServeArgs`:

```rust
#[arg(long, env = "LYRE_STATE_FILE")]
pub state_file: Option<String>,
```

Add error enum:

```rust
#[derive(Debug, Error, PartialEq, Eq)]
pub enum StateFileConfigError {
    #[error("state file path must not be blank")]
    BlankPath,
}
```

Add method:

```rust
pub fn effective_state_file(&self) -> Result<Option<PathBuf>, StateFileConfigError> {
    let Some(path) = self
        .state_file
        .clone()
        .or_else(|| env::var("LYRE_STATE_FILE").ok())
    else {
        return Ok(None);
    };
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(StateFileConfigError::BlankPath);
    }
    Ok(Some(PathBuf::from(trimmed)))
}
```

Update `default_serve_args()` with:

```rust
state_file: None,
```

- [x] **Step 4: Wire server config**

In `crates/lyre-web/src/server.rs`, import:

```rust
use crate::state_persistence::RoomStatePersistence;
use std::{net::SocketAddr, path::PathBuf, str::FromStr};
```

Replace the existing `std::{net::SocketAddr, str::FromStr}` import.

Add field to `ServeConfig`:

```rust
pub state_file: Option<PathBuf>,
```

In `serve`, before building `api`, add:

```rust
let room_state_persistence = config.state_file.clone().map(RoomStatePersistence::new);
let state = AppState::with_room_state_persistence(
    config.ice_servers,
    config.turn_rest_credentials,
    room_state_persistence,
)
.context("failed to initialize Lyre room state")?;
```

Then change the router construction inside `api` to:

```rust
router(state)
```

In `crates/lyre-app/src/main.rs`, compute:

```rust
let state_file = args.effective_state_file()?;
```

and pass:

```rust
state_file,
```

into `ServeConfig`.

- [x] **Step 5: Run CLI/server tests**

Run:

```bash
cargo test -p lyre-app state_file
cargo test -p lyre-web server::tests
```

Expected: all selected tests pass.

## Task 5: Implementation Review, Docs, Verification, and SDD Closeout

**Files:**
- Modify: `MEMORY.md`
- Modify: `docs/roadmap.md`
- Modify plan checkboxes in `docs/superpowers/plans/2026-06-15-room-state-persistence.md`

- [x] **Step 1: Run implementation verification before review**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path Cargo.toml --workspace
cd frontend && npm test -- --run
cd frontend && npm run typecheck
cd frontend && npm run lint
cd frontend && npm run build
git diff --check
```

Expected: all commands exit 0. `next build` may print the existing localStorage experimental warning; it is acceptable only if the command exits 0.

- [x] **Step 2: Request implementation review**

Dispatch an independent implementation reviewer with:

- spec path `docs/superpowers/specs/2026-06-15-room-state-persistence-design.md`;
- this plan path;
- current diff;
- verification output.

Required verdict format:

```text
VERDICT: APPROVE | REVISE
SPEC_COVERAGE:
- [implemented requirement or missing requirement]
BLOCKERS:
- [blocking gap or "None"]
REQUIRED_CHANGES:
- [change or "None"]
```

Proceed to docs updates only after the implementation reviewer returns `VERDICT: APPROVE`.

- [x] **Step 3: Update MEMORY**

Add to `MEMORY.md`:

```markdown
## 2026-06-15 Room State Persistence

- Added optional JSON file persistence for anonymous room users and access tokens via `--state-file` / `LYRE_STATE_FILE`.
- Persisted state is limited to the room registry. WebSocket peer handles, WebRTC sessions, relay pumps, processed audio buffers, TURN state, and media runtime state remain process-local.
- Persisted join/leave mutations are serialized and roll back in-memory registry state if the state file write fails, so failed leaves do not resurrect tokens after restart.
```

- [x] **Step 4: Update roadmap**

In `docs/roadmap.md`, move:

```markdown
- Add persistent room/user/session state.
```

from `## Next` to `## Completed`, changing the completed wording to:

```markdown
- Optional JSON file persistence for anonymous room/user/session access state.
```

- [x] **Step 5: Mark this plan complete**

Update every checkbox in this plan from `[ ]` to `[x]` once the corresponding step is completed and verified.

- [x] **Step 6: Run final verification after docs**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path Cargo.toml --workspace
cd frontend && npm test -- --run
cd frontend && npm run typecheck
cd frontend && npm run lint
cd frontend && npm run build
git diff --check
```

Expected: all commands exit 0. `next build` may print the existing localStorage experimental warning; it is acceptable only if the command exits 0.

- [x] **Step 7: Commit and push after review approval**

Stage only the files for this increment and any previously untracked SDD docs that are intentionally part of the repository. Commit with Lore protocol:

```text
Persist anonymous room sessions across API restarts

Constraint: Persistence is optional and file-backed to avoid introducing account or database scope in this milestone.
Rejected: Database-backed sessions | out of scope for the current anonymous-room increment.
Rejected: Persisting WebRTC/media runtime state | those handles are process-local and must renegotiate after restart.
Confidence: high
Scope-risk: broad
Directive: Keep persisted access tokens server-private and out of public snapshots, signalling payloads, logs, and error responses.
Tested: cargo fmt --all --check; cargo clippy --workspace --all-targets -- -D warnings; cargo nextest run --manifest-path Cargo.toml --workspace; cd frontend && npm test -- --run; cd frontend && npm run typecheck; cd frontend && npm run lint; cd frontend && npm run build; git diff --check
Not-tested: Cross-process writers sharing one state file, which is explicitly unsupported.
```

Push the current branch/upstream.
