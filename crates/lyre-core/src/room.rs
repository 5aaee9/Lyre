use crate::{NoiseCancellationConfig, RoomId, UserId};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use rand::{rngs::StdRng, Rng as _, SeedableRng};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RoomAccessToken(String);

impl RoomAccessToken {
    pub fn new() -> Self {
        let mut rng = StdRng::from_os_rng();
        let bytes: [u8; 32] = rng.random();
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

#[derive(Debug, Default)]
struct RoomState {
    users: DashMap<UserId, UserProfile>,
    access_tokens: DashMap<UserId, RoomAccessToken>,
}

#[derive(Debug, Default)]
pub struct RoomRegistry {
    rooms: DashMap<RoomId, RoomState>,
}

impl RoomRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self, room_id: RoomId) -> RoomSnapshot {
        self.rooms.entry(room_id.clone()).or_default();
        self.snapshot_existing(room_id)
    }

    pub fn join(&self, room_id: RoomId, request: JoinRoomRequest) -> JoinRoomResponse {
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
        drop(room);

        JoinRoomResponse {
            user,
            room: self.snapshot_existing(room_id),
            access_token,
        }
    }

    pub fn leave(&self, room_id: &RoomId, user_id: &UserId) -> RoomSnapshot {
        if let Some(room) = self.rooms.get(room_id) {
            room.users.remove(user_id);
            room.access_tokens.remove(user_id);
        }
        self.snapshot(room_id.clone())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DEFAULT_ROOM_ID;

    #[test]
    fn snapshot_auto_creates_arbitrary_room() {
        let registry = RoomRegistry::new();
        let room_id = RoomId::parse_boundary("Team-A").unwrap();
        let snapshot = registry.snapshot(room_id.clone());
        assert_eq!(snapshot.room_id, room_id);
        assert!(snapshot.users.is_empty());
    }

    #[test]
    fn blank_nickname_gets_guest_name() {
        let registry = RoomRegistry::new();
        let response = registry.join(
            RoomId::default_room(),
            JoinRoomRequest {
                nickname: Some(" ".to_owned()),
                noise: None,
            },
        );

        assert_eq!(response.user.nickname, "Guest 1");
        assert_eq!(response.room.users.len(), 1);
    }

    #[test]
    fn leave_removes_only_matching_user() {
        let registry = RoomRegistry::new();
        let room_id = RoomId::default_room();
        let first = registry
            .join(room_id.clone(), JoinRoomRequest::default())
            .user;
        let second = registry
            .join(room_id.clone(), JoinRoomRequest::default())
            .user;

        let snapshot = registry.leave(&room_id, &first.id);

        assert_eq!(snapshot.users.len(), 1);
        assert_eq!(snapshot.users[0].id, second.id);
    }

    #[test]
    fn default_room_is_available() {
        let registry = RoomRegistry::new();
        let snapshot = registry.snapshot(RoomId::default_room());

        assert_eq!(snapshot.room_id.as_str(), DEFAULT_ROOM_ID);
    }

    #[test]
    fn join_returns_distinct_access_tokens() {
        let registry = RoomRegistry::new();
        let room_id = RoomId::default_room();

        let first = registry.join(room_id.clone(), JoinRoomRequest::default());
        let second = registry.join(room_id.clone(), JoinRoomRequest::default());

        assert!(!first.access_token.as_str().is_empty());
        assert_ne!(first.access_token, second.access_token);
    }

    #[test]
    fn access_token_validates_room_and_user_tuple() {
        let registry = RoomRegistry::new();
        let room_id = RoomId::default_room();
        let response = registry.join(room_id.clone(), JoinRoomRequest::default());

        assert!(registry
            .validate_access_token(&room_id, &response.user.id, &response.access_token)
            .is_ok());
        assert!(registry
            .validate_access_token(
                &RoomId::parse_boundary("OTHER").unwrap(),
                &response.user.id,
                &response.access_token,
            )
            .is_err());
        assert!(registry
            .validate_access_token(
                &room_id,
                &UserId::from_external("other_user"),
                &response.access_token,
            )
            .is_err());
        assert!(registry
            .validate_access_token(
                &room_id,
                &response.user.id,
                &RoomAccessToken::from_external("unknown"),
            )
            .is_err());
    }

    #[test]
    fn access_token_validates_any_room_member() {
        let registry = RoomRegistry::new();
        let room_id = RoomId::default_room();
        let response = registry.join(room_id.clone(), JoinRoomRequest::default());

        assert!(registry
            .validate_any_access_token(&room_id, &response.access_token)
            .is_ok());
        assert!(registry
            .validate_any_access_token(
                &RoomId::parse_boundary("OTHER").unwrap(),
                &response.access_token,
            )
            .is_err());
    }

    #[test]
    fn leave_invalidates_access_token() {
        let registry = RoomRegistry::new();
        let room_id = RoomId::default_room();
        let response = registry.join(room_id.clone(), JoinRoomRequest::default());

        registry.leave(&room_id, &response.user.id);

        assert!(registry
            .validate_access_token(&room_id, &response.user.id, &response.access_token)
            .is_err());
    }

    #[test]
    fn room_snapshot_does_not_serialize_access_token() {
        let registry = RoomRegistry::new();
        let room_id = RoomId::default_room();
        registry.join(room_id.clone(), JoinRoomRequest::default());

        let json = serde_json::to_value(registry.snapshot(room_id)).unwrap();

        assert!(json.to_string().contains("users"));
        assert!(!json.to_string().contains("access_token"));
    }
}
