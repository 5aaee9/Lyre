use crate::{NoiseCancellationConfig, RoomId, UserId};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeaveRoomRequest {
    pub user_id: UserId,
}

#[derive(Debug, Default)]
struct RoomState {
    users: DashMap<UserId, UserProfile>,
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
        room.users.insert(user.id.clone(), user.clone());
        drop(room);

        JoinRoomResponse {
            user,
            room: self.snapshot_existing(room_id),
        }
    }

    pub fn leave(&self, room_id: &RoomId, user_id: &UserId) -> RoomSnapshot {
        if let Some(room) = self.rooms.get(room_id) {
            room.users.remove(user_id);
        }
        self.snapshot(room_id.clone())
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
}
