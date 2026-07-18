use crate::{
    JoinRoomRequest, RoomAccessToken, RoomId, RoomRegistry, RoomRegistryAggregate, UserId,
    DEFAULT_ROOM_ID,
};

#[test]
fn snapshot_unknown_room_does_not_create_room() {
    let registry = RoomRegistry::new();
    let room_id = RoomId::parse_boundary("Team-A").unwrap();
    let snapshot = registry.snapshot(room_id.clone());
    assert_eq!(snapshot.room_id, room_id);
    assert!(snapshot.users.is_empty());
    assert_eq!(
        registry.aggregate(),
        RoomRegistryAggregate { rooms: 0, users: 0 }
    );
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

    let snapshot = registry.leave(&room_id, &first.id).room;

    assert_eq!(snapshot.users.len(), 1);
    assert_eq!(snapshot.users[0].id, second.id);
}

#[test]
fn last_leave_removes_room_and_persisted_entry() {
    let registry = RoomRegistry::new();
    let room_id = RoomId::default_room();
    let joined = registry.join(room_id.clone(), JoinRoomRequest::default());

    let response = registry.leave(&room_id, &joined.user.id);

    assert!(response.removed);
    assert!(response.room.users.is_empty());
    assert_eq!(
        registry.aggregate(),
        RoomRegistryAggregate { rooms: 0, users: 0 }
    );
    assert!(registry.to_persisted().rooms.is_empty());
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
