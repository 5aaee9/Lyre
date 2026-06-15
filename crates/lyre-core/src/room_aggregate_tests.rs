use crate::{JoinRoomRequest, RoomId, RoomRegistry, RoomRegistryAggregate, UserId};

#[test]
fn aggregate_counts_rooms_and_users_without_creating_rooms() {
    let registry = RoomRegistry::new();

    assert_eq!(
        registry.aggregate(),
        RoomRegistryAggregate { rooms: 0, users: 0 }
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
        RoomRegistryAggregate { rooms: 1, users: 1 }
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

#[test]
fn missing_leave_does_not_create_room() {
    let registry = RoomRegistry::new();
    let room_id = RoomId::parse_boundary("MISSING").unwrap();

    let response = registry.leave(&room_id, &UserId::from_external("missing"));

    assert!(!response.removed);
    assert_eq!(response.room.room_id, room_id);
    assert!(response.room.users.is_empty());
    assert_eq!(
        registry.aggregate(),
        RoomRegistryAggregate { rooms: 0, users: 0 }
    );
}
