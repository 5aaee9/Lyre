#[cfg(test)]
mod tests {
    use crate::{
        JoinRoomRequest, PersistedRoom, PersistedRoomRegistry, PersistedRoomUser, RoomAccessToken,
        RoomId, RoomRegistry, UserId, UserProfile,
    };
    use chrono::{TimeZone, Utc};

    fn persisted_user(user_id: &str, nickname: &str, access_token: &str) -> PersistedRoomUser {
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
}
