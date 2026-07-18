use crate::signalling::{route_signal_message, PeerHub, SignalMessage, SignalPayload, SignalRoute};
use lyre_core::{
    JoinRoomRequest, NoiseCancellationConfig, RoomId, RoomRegistry, UserId, UserProfile,
};
use tokio::sync::mpsc;

fn ids() -> (RoomId, UserId, UserId) {
    (
        RoomId::default_room(),
        UserId::from_external("user_a"),
        UserId::from_external("user_b"),
    )
}

#[test]
fn serializes_offer_message() {
    let (room_id, sender_id, recipient_id) = ids();
    let message = SignalMessage::new(
        room_id,
        sender_id,
        Some(recipient_id),
        SignalPayload::Offer { sdp: "sdp".into() },
    );

    let json = serde_json::to_value(message).unwrap();

    assert_eq!(json["type"], "offer");
    assert_eq!(json["payload"]["type"], "offer");
    assert_eq!(json["payload"]["sdp"], "sdp");
}

#[test]
fn serializes_all_server_payloads() {
    let registry = RoomRegistry::new();
    let room_id = RoomId::default_room();
    let user = registry
        .join(room_id.clone(), JoinRoomRequest::default())
        .user;
    let snapshot = registry.snapshot(room_id.clone());
    let sender_id = user.id.clone();
    let cases = [
        SignalPayload::Answer { sdp: "sdp".into() },
        SignalPayload::IceCandidate {
            candidate: "candidate".into(),
            sdp_mid: Some("0".into()),
            sdp_m_line_index: Some(0),
        },
        SignalPayload::UserJoined { user },
        SignalPayload::UserLeft {
            user_id: sender_id.clone(),
        },
        SignalPayload::RoomSnapshot { room: snapshot },
        SignalPayload::Error {
            message: "bad message".into(),
        },
    ];

    for payload in cases {
        let message = SignalMessage::new(room_id.clone(), sender_id.clone(), None, payload);
        assert!(serde_json::to_value(message).unwrap()["payload"].is_object());
    }
}

#[test]
fn serializes_server_media_ice_payloads() {
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_a");
    let candidate = lyre_webrtc::ServerMediaIceCandidate {
        room_id: room_id.clone(),
        user_id: user_id.clone(),
        candidate: "candidate:server".into(),
        sdp_mid: Some("0".into()),
        sdp_mline_index: Some(0),
        username_fragment: Some("ufrag".into()),
    };

    let message = SignalMessage::to_self(
        room_id.clone(),
        user_id.clone(),
        SignalPayload::ServerMediaIceCandidate {
            candidate: "candidate:local".into(),
            sdp_mid: Some("0".into()),
            sdp_mline_index: Some(0),
            username_fragment: Some("ufrag".into()),
        },
    );
    let json = serde_json::to_value(&message).unwrap();
    assert_eq!(json["type"], "server-media-ice-candidate");
    assert_eq!(json["payload"]["type"], "server-media-ice-candidate");
    assert_eq!(json["payload"]["sdp_mline_index"], 0);
    let decoded: SignalMessage = serde_json::from_value(json).unwrap();
    assert_eq!(decoded.payload, message.payload);

    let request = SignalMessage::to_self(
        room_id.clone(),
        user_id.clone(),
        SignalPayload::ServerMediaIceCandidatesRequest,
    );
    let request_json = serde_json::to_value(&request).unwrap();
    assert_eq!(request_json["type"], "server-media-ice-candidates-request");
    assert_eq!(
        request_json["payload"]["type"],
        "server-media-ice-candidates-request"
    );
    let decoded_request: SignalMessage = serde_json::from_value(request_json).unwrap();
    assert_eq!(decoded_request.payload, request.payload);

    let response = SignalMessage::to_self(
        room_id,
        user_id,
        SignalPayload::ServerMediaIceCandidates {
            candidates: vec![candidate],
        },
    );
    let response_json = serde_json::to_value(&response).unwrap();
    assert_eq!(response_json["type"], "server-media-ice-candidates");
    assert_eq!(
        response_json["payload"]["type"],
        "server-media-ice-candidates"
    );
    assert_eq!(
        response_json["payload"]["candidates"][0]["sdp_mline_index"],
        0
    );
    let decoded_response: SignalMessage = serde_json::from_value(response_json).unwrap();
    assert_eq!(decoded_response.payload, response.payload);
}

#[test]
fn routes_targeted_and_broadcast_messages() {
    let (room_id, sender_id, recipient_id) = ids();
    let targeted = SignalMessage::new(
        room_id.clone(),
        sender_id.clone(),
        Some(recipient_id.clone()),
        SignalPayload::Offer { sdp: "sdp".into() },
    );
    let broadcast = SignalMessage::new(
        room_id.clone(),
        sender_id.clone(),
        None,
        SignalPayload::IceCandidate {
            candidate: "candidate".into(),
            sdp_mid: None,
            sdp_m_line_index: None,
        },
    );

    assert_eq!(
        route_signal_message(&room_id, &sender_id, &targeted).unwrap(),
        SignalRoute::Target(recipient_id)
    );
    assert_eq!(
        route_signal_message(&room_id, &sender_id, &broadcast).unwrap(),
        SignalRoute::BroadcastExceptSender
    );
}

#[test]
fn rejects_mismatched_room_or_sender() {
    let (room_id, sender_id, _) = ids();
    let message = SignalMessage::new(
        RoomId::parse_boundary("OTHER").unwrap(),
        sender_id.clone(),
        None,
        SignalPayload::Offer { sdp: "sdp".into() },
    );
    assert!(route_signal_message(&room_id, &sender_id, &message).is_err());

    let message = SignalMessage::new(
        room_id.clone(),
        UserId::from_external("other"),
        None,
        SignalPayload::Offer { sdp: "sdp".into() },
    );
    assert!(route_signal_message(&room_id, &sender_id, &message).is_err());
}

#[test]
fn targeted_delivery_reaches_only_recipient() {
    let hub = PeerHub::new();
    let room_id = RoomId::default_room();
    let sender_id = UserId::from_external("sender");
    let recipient_id = UserId::from_external("recipient");
    let observer_id = UserId::from_external("observer");
    let (sender_tx, mut sender_rx) = mpsc::channel(crate::signalling::SIGNAL_CHANNEL_CAPACITY);
    let (recipient_tx, mut recipient_rx) =
        mpsc::channel(crate::signalling::SIGNAL_CHANNEL_CAPACITY);
    let (observer_tx, mut observer_rx) = mpsc::channel(crate::signalling::SIGNAL_CHANNEL_CAPACITY);
    let registry = RoomRegistry::new();
    hub.connect(&registry, room_id.clone(), sender_id.clone(), sender_tx);
    hub.connect(
        &registry,
        room_id.clone(),
        recipient_id.clone(),
        recipient_tx,
    );
    hub.connect(&registry, room_id.clone(), observer_id, observer_tx);

    let delivered = hub.forward(SignalMessage::new(
        room_id,
        sender_id,
        Some(recipient_id),
        SignalPayload::Offer { sdp: "sdp".into() },
    ));

    assert_eq!(delivered.delivered, 1);
    assert!(recipient_rx.try_recv().is_ok());
    assert!(sender_rx.try_recv().is_err());
    assert!(observer_rx.try_recv().is_err());
}

#[test]
fn slow_recipient_queue_is_bounded_without_blocking_healthy_peer() {
    let hub = PeerHub::new();
    let registry = RoomRegistry::new();
    let room_id = RoomId::default_room();
    let sender_id = UserId::from_external("sender");
    let slow_id = UserId::from_external("slow");
    let healthy_id = UserId::from_external("healthy");
    let (slow_tx, mut slow_rx) = mpsc::channel(1);
    let (healthy_tx, mut healthy_rx) = mpsc::channel(1);
    hub.connect(&registry, room_id.clone(), slow_id.clone(), slow_tx);
    hub.connect(&registry, room_id.clone(), healthy_id.clone(), healthy_tx);
    let message_to = |recipient_id| {
        SignalMessage::new(
            room_id.clone(),
            sender_id.clone(),
            Some(recipient_id),
            SignalPayload::ServerMediaIceCandidatesRequest,
        )
    };

    assert_eq!(hub.forward(message_to(slow_id.clone())).delivered, 1);
    assert_eq!(hub.forward(message_to(slow_id.clone())).delivered, 0);
    assert_eq!(hub.forward(message_to(healthy_id)).delivered, 1);

    assert!(slow_rx.try_recv().is_ok());
    assert_eq!(hub.forward(message_to(slow_id)).delivered, 1);
    assert!(slow_rx.try_recv().is_ok());
    assert!(healthy_rx.try_recv().is_ok());
}

#[test]
fn dropped_presence_event_marks_the_current_peer_for_authoritative_resync() {
    let hub = PeerHub::new();
    let registry = RoomRegistry::new();
    let room_id = RoomId::default_room();
    let recipient_id = UserId::from_external("recipient");
    let joined_id = UserId::from_external("joined");
    let (recipient_tx, mut recipient_rx) = mpsc::channel(1);
    hub.connect(
        &registry,
        room_id.clone(),
        recipient_id.clone(),
        recipient_tx.clone(),
    );
    hub.forward(SignalMessage::to_self(
        room_id.clone(),
        recipient_id.clone(),
        SignalPayload::Error {
            message: "fills the bounded queue".to_owned(),
        },
    ));

    let delivery = hub.user_joined(
        &room_id,
        UserProfile {
            id: joined_id.clone(),
            nickname: "Joined".to_owned(),
            joined_at: chrono::Utc::now(),
            noise: NoiseCancellationConfig::default(),
        },
    );

    assert_eq!(delivery.delivered, 0);
    assert!(recipient_rx.try_recv().is_ok());
    assert_eq!(hub.user_left(&room_id, &joined_id).delivered, 0);
    assert!(recipient_rx.try_recv().is_err());
    assert!(hub.take_room_snapshot_pending(&room_id, &recipient_id, &recipient_tx));
    assert!(!hub.take_room_snapshot_pending(&room_id, &recipient_id, &recipient_tx));
}

#[test]
fn broadcast_excludes_sender_and_presence_events_emit() {
    let hub = PeerHub::new();
    let registry = RoomRegistry::new();
    let room_id = RoomId::default_room();
    let sender_id = UserId::from_external("sender");
    let peer_id = UserId::from_external("peer");
    let (sender_tx, mut sender_rx) = mpsc::channel(crate::signalling::SIGNAL_CHANNEL_CAPACITY);
    let (peer_tx, mut peer_rx) = mpsc::channel(crate::signalling::SIGNAL_CHANNEL_CAPACITY);
    hub.connect(&registry, room_id.clone(), sender_id.clone(), sender_tx);
    hub.connect(&registry, room_id.clone(), peer_id.clone(), peer_tx);

    let delivered = hub.forward(SignalMessage::new(
        room_id.clone(),
        sender_id.clone(),
        None,
        SignalPayload::IceCandidate {
            candidate: "candidate".into(),
            sdp_mid: None,
            sdp_m_line_index: None,
        },
    ));

    assert_eq!(delivered.delivered, 1);
    assert!(peer_rx.try_recv().is_ok());
    assert!(sender_rx.try_recv().is_err());

    let user = UserProfile {
        id: sender_id.clone(),
        nickname: "Alice".into(),
        joined_at: chrono::Utc::now(),
        noise: NoiseCancellationConfig::default(),
    };
    assert_eq!(hub.user_joined(&room_id, user).delivered, 1);
    assert_eq!(hub.user_left(&room_id, &sender_id).delivered, 1);
    assert_eq!(hub.disconnect(&room_id, &peer_id).delivered, 1);
}

#[test]
fn remove_peer_drops_socket_without_presence_broadcast() {
    let hub = PeerHub::new();
    let registry = RoomRegistry::new();
    let room_id = RoomId::default_room();
    let leaving_id = UserId::from_external("leaving");
    let peer_id = UserId::from_external("peer");
    let (leaving_tx, mut leaving_rx) = mpsc::channel(crate::signalling::SIGNAL_CHANNEL_CAPACITY);
    let (peer_tx, mut peer_rx) = mpsc::channel(crate::signalling::SIGNAL_CHANNEL_CAPACITY);
    hub.connect(&registry, room_id.clone(), leaving_id.clone(), leaving_tx);
    hub.connect(&registry, room_id.clone(), peer_id.clone(), peer_tx);

    hub.remove_peer(&room_id, &leaving_id);
    let delivered = hub.forward(SignalMessage::new(
        room_id,
        peer_id,
        Some(leaving_id),
        SignalPayload::Offer { sdp: "sdp".into() },
    ));

    assert_eq!(delivered.delivered, 0);
    assert!(leaving_rx.try_recv().is_err());
    assert!(peer_rx.try_recv().is_err());
}

#[test]
fn removing_replaced_peer_keeps_reconnected_socket() {
    let hub = PeerHub::new();
    let registry = RoomRegistry::new();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user");
    let (old_tx, mut old_rx) = mpsc::channel(crate::signalling::SIGNAL_CHANNEL_CAPACITY);
    let (new_tx, mut new_rx) = mpsc::channel(crate::signalling::SIGNAL_CHANNEL_CAPACITY);
    hub.connect(&registry, room_id.clone(), user_id.clone(), old_tx.clone());
    hub.connect(&registry, room_id.clone(), user_id.clone(), new_tx);

    hub.remove_peer_sender(&room_id, &user_id, &old_tx);
    let delivered = hub.forward(SignalMessage::to_self(
        room_id,
        user_id,
        SignalPayload::Offer { sdp: "sdp".into() },
    ));

    assert_eq!(delivered.delivered, 1);
    assert!(old_rx.try_recv().is_err());
    assert!(new_rx.try_recv().is_ok());
}
