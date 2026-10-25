use crate::{NoiseCancellationConfig, RoomId, UserId};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RoomAccessToken(String);

impl RoomAccessToken {
    pub fn new() -> Self {
        let bytes: [u8; 32] = rand::random();
        Self(URL_SAFE_NO_PAD.encode(bytes))
    }

    pub fn from_external(input: impl Into<String>) -> Self {
        Self(input.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for RoomAccessToken {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RoomAccessError {
    #[error("room access token is invalid")]
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: UserId,
    pub nickname: String,
    pub joined_at: DateTime<Utc>,
    pub noise: NoiseCancellationConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoomSnapshot {
    pub room_id: RoomId,
    pub users: Vec<UserProfile>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct JoinRoomRequest {
    pub nickname: Option<String>,
    pub noise: Option<NoiseCancellationConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JoinRoomResponse {
    pub user: UserProfile,
    pub room: RoomSnapshot,
    pub access_token: RoomAccessToken,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeaveRoomRequest {
    pub user_id: UserId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeaveRoomResponse {
    pub room: RoomSnapshot,
    pub removed: bool,
}

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
    pub websocket_admission: PersistedWebSocketAdmission,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PersistedWebSocketAdmission {
    Confirmed,
    Pending { expires_at: DateTime<Utc> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoomRegistryAggregate {
    pub rooms: usize,
    pub users: usize,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PersistedRoomRegistryError {
    #[error("persisted room id is invalid")]
    InvalidRoomId(#[from] crate::RoomIdError),
}

#[derive(Debug, Default)]
struct RoomState {
    users: DashMap<UserId, UserProfile>,
    access_tokens: DashMap<UserId, RoomAccessToken>,
    pending_websocket_admissions: DashMap<UserId, DateTime<Utc>>,
}

#[derive(Debug, Default)]
pub struct RoomRegistry {
    rooms: DashMap<RoomId, RoomState>,
}

impl RoomRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_persisted(persisted: PersistedRoomRegistry) -> Self {
        let registry = Self::new();
        registry.replace_with_persisted(persisted);
        registry
    }

    pub fn snapshot(&self, room_id: RoomId) -> RoomSnapshot {
        self.snapshot_existing(room_id)
    }

    pub fn join(&self, room_id: RoomId, request: JoinRoomRequest) -> JoinRoomResponse {
        self.join_with_websocket_admission(room_id, request, None)
    }

    pub fn join_pending_websocket_admission(
        &self,
        room_id: RoomId,
        request: JoinRoomRequest,
        expires_at: DateTime<Utc>,
    ) -> JoinRoomResponse {
        self.join_with_websocket_admission(room_id, request, Some(expires_at))
    }

    fn join_with_websocket_admission(
        &self,
        room_id: RoomId,
        request: JoinRoomRequest,
        pending_expires_at: Option<DateTime<Utc>>,
    ) -> JoinRoomResponse {
        let room = self.rooms.entry(room_id.clone()).or_default();
        let nickname = request
            .nickname
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("Guest {}", room.users.len() + 1));
        let user = UserProfile {
            id: UserId::new(),
            nickname,
            joined_at: Utc::now(),
            noise: request.noise.unwrap_or_default(),
        };
        let access_token = RoomAccessToken::new();
        room.users.insert(user.id.clone(), user.clone());
        room.access_tokens
            .insert(user.id.clone(), access_token.clone());
        if let Some(expires_at) = pending_expires_at {
            room.pending_websocket_admissions
                .insert(user.id.clone(), expires_at);
        }
        drop(room);

        JoinRoomResponse {
            user,
            room: self.snapshot_existing(room_id),
            access_token,
        }
    }

    pub fn leave(&self, room_id: &RoomId, user_id: &UserId) -> LeaveRoomResponse {
        let removed = if let Some(room) = self.rooms.get(room_id) {
            let removed = room.users.remove(user_id).is_some();
            room.access_tokens.remove(user_id);
            room.pending_websocket_admissions.remove(user_id);
            let room_is_empty = room.users.is_empty();
            drop(room);
            if removed && room_is_empty {
                self.rooms
                    .remove_if(room_id, |_, room| room.users.is_empty());
            }
            removed
        } else {
            false
        };
        LeaveRoomResponse {
            room: self.snapshot_existing(room_id.clone()),
            removed,
        }
    }

    pub fn validate_access_token(
        &self,
        room_id: &RoomId,
        user_id: &UserId,
        token: &RoomAccessToken,
    ) -> Result<(), RoomAccessError> {
        let Some(room) = self.rooms.get(room_id) else {
            return Err(RoomAccessError::Invalid);
        };
        let valid = room
            .access_tokens
            .get(user_id)
            .is_some_and(|stored| stored.value() == token);
        if valid {
            Ok(())
        } else {
            Err(RoomAccessError::Invalid)
        }
    }

    pub fn validate_any_access_token(
        &self,
        room_id: &RoomId,
        token: &RoomAccessToken,
    ) -> Result<(), RoomAccessError> {
        let Some(room) = self.rooms.get(room_id) else {
            return Err(RoomAccessError::Invalid);
        };
        if room
            .access_tokens
            .iter()
            .any(|entry| entry.value() == token)
        {
            Ok(())
        } else {
            Err(RoomAccessError::Invalid)
        }
    }

    pub fn confirm_websocket_admission(&self, room_id: &RoomId, user_id: &UserId) -> bool {
        self.rooms
            .get(room_id)
            .is_some_and(|room| room.pending_websocket_admissions.remove(user_id).is_some())
    }

    pub fn pending_websocket_admissions(&self) -> Vec<(RoomId, UserId, DateTime<Utc>)> {
        let mut pending = self
            .rooms
            .iter()
            .flat_map(|room| {
                room.pending_websocket_admissions
                    .iter()
                    .map(|entry| (room.key().clone(), entry.key().clone(), *entry.value()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        pending.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then(left.1.cmp(&right.1))
                .then(left.2.cmp(&right.2))
        });
        pending
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
                            websocket_admission: room_entry
                                .value()
                                .pending_websocket_admissions
                                .get(user_entry.key())
                                .map_or(PersistedWebSocketAdmission::Confirmed, |expires_at| {
                                    PersistedWebSocketAdmission::Pending {
                                        expires_at: *expires_at.value(),
                                    }
                                }),
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

    pub fn replace_with_persisted(&self, persisted: PersistedRoomRegistry) {
        self.rooms.clear();
        for persisted_room in persisted.rooms {
            let room = self.rooms.entry(persisted_room.room_id).or_default();
            for persisted_user in persisted_room.users {
                let user_id = persisted_user.profile.id.clone();
                match persisted_user.websocket_admission {
                    PersistedWebSocketAdmission::Confirmed => {
                        room.pending_websocket_admissions.remove(&user_id);
                    }
                    PersistedWebSocketAdmission::Pending { expires_at } => {
                        room.pending_websocket_admissions
                            .insert(user_id.clone(), expires_at);
                    }
                }
                room.users.insert(user_id.clone(), persisted_user.profile);
                room.access_tokens
                    .insert(user_id, persisted_user.access_token);
            }
        }
    }

    fn snapshot_existing(&self, room_id: RoomId) -> RoomSnapshot {
        let mut users: Vec<UserProfile> = self
            .rooms
            .get(&room_id)
            .map(|room| {
                room.users
                    .iter()
                    .map(|entry| entry.value().clone())
                    .collect()
            })
            .unwrap_or_default();
        users.sort_by(|left: &UserProfile, right: &UserProfile| {
            left.nickname
                .cmp(&right.nickname)
                .then(left.id.cmp(&right.id))
        });
        RoomSnapshot { room_id, users }
    }
}
