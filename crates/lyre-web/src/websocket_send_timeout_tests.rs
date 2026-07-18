use crate::{
    api::AppState,
    signalling::{SignalMessage, SignalPayload, SIGNAL_CHANNEL_CAPACITY},
};
use axum::extract::ws::Message;
use futures_util::future::pending;
use lyre_core::{JoinRoomRequest, RoomId, UserId};
use std::{convert::Infallible, sync::Arc, time::Duration};

#[tokio::test]
async fn stalled_websocket_send_times_out_and_releases_room_membership() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let joined = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap();
    let (snapshot_sent_tx, snapshot_sent_rx) = tokio::sync::oneshot::channel();
    let sink = Box::pin(futures_util::sink::unfold(
        (false, Some(snapshot_sent_tx)),
        |(snapshot_sent, mut snapshot_sent_tx), _message: Message| async move {
            if !snapshot_sent {
                snapshot_sent_tx.take().unwrap().send(()).unwrap();
                return Ok::<_, Infallible>((true, snapshot_sent_tx));
            }
            pending::<Result<(bool, Option<tokio::sync::oneshot::Sender<()>>), Infallible>>().await
        },
    ));
    let stream = futures_util::stream::pending::<Result<Message, Infallible>>();
    let handler = tokio::spawn(crate::api_websocket::handle_socket_io_with_send_timeout(
        state.clone(),
        room_id.clone(),
        joined.user.id.clone(),
        sink,
        stream,
        Duration::from_millis(25),
    ));
    snapshot_sent_rx.await.unwrap();

    assert_eq!(
        state
            .peers
            .forward(SignalMessage::error(
                room_id.clone(),
                UserId::from_external("peer"),
                "test",
            ))
            .delivered,
        1
    );

    tokio::time::timeout(Duration::from_secs(1), handler)
        .await
        .expect("stalled websocket send should not retain the handler")
        .unwrap();
    assert!(state.registry.snapshot(room_id).users.is_empty());
    assert_eq!(crate::metrics::snapshot(&state).leaves, 1);
}

#[tokio::test]
async fn recovered_slow_websocket_receives_snapshot_after_dropped_presence_event() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let observer = state
        .registry
        .join(room_id.clone(), JoinRoomRequest::default())
        .user;
    let (outbound_tx, mut outbound_rx) = tokio::sync::mpsc::unbounded_channel();
    let send_gate = Arc::new(tokio::sync::Semaphore::new(0));
    let (blocked_tx, blocked_rx) = tokio::sync::oneshot::channel();
    let sink = futures_util::sink::unfold(
        (0_usize, outbound_tx, send_gate.clone(), Some(blocked_tx)),
        |(send_count, outbound_tx, send_gate, mut blocked_tx), message: Message| async move {
            if send_count > 0 {
                if send_count == 1 {
                    blocked_tx.take().unwrap().send(()).unwrap();
                }
                let _permit = send_gate.acquire().await.unwrap();
            }
            outbound_tx.send(message).unwrap();
            Ok::<_, Infallible>((send_count + 1, outbound_tx, send_gate, blocked_tx))
        },
    );
    let stream = futures_util::stream::pending::<Result<Message, Infallible>>();
    let handler = tokio::spawn(crate::api_websocket::handle_socket_io(
        state.clone(),
        room_id.clone(),
        observer.id.clone(),
        Box::pin(sink),
        stream,
    ));
    outbound_rx.recv().await.unwrap();

    assert_eq!(
        state
            .peers
            .forward(SignalMessage::error(
                room_id.clone(),
                UserId::from_external("source"),
                "block websocket writer",
            ))
            .delivered,
        1
    );
    blocked_rx.await.unwrap();
    for index in 0..SIGNAL_CHANNEL_CAPACITY {
        assert_eq!(
            state
                .peers
                .forward(SignalMessage::error(
                    room_id.clone(),
                    UserId::from_external(format!("source-{index}")),
                    "fill bounded queue",
                ))
                .delivered,
            1
        );
    }
    let joined = state
        .registry
        .join(room_id.clone(), JoinRoomRequest::default())
        .user;
    assert_eq!(
        state.peers.user_joined(&room_id, joined.clone()).delivered,
        0
    );

    send_gate.add_permits(1);
    let mut received_resync = false;
    for _ in 0..(SIGNAL_CHANNEL_CAPACITY + 3) {
        let message = tokio::time::timeout(Duration::from_secs(1), outbound_rx.recv())
            .await
            .unwrap()
            .unwrap();
        let Message::Text(json) = message else {
            continue;
        };
        let message: SignalMessage = serde_json::from_str(&json).unwrap();
        if let SignalPayload::RoomSnapshot { room } = message.payload {
            if room.users.iter().any(|user| user.id == joined.id) {
                received_resync = true;
                break;
            }
        }
    }

    handler.abort();
    let _ = handler.await;
    assert!(received_resync);
}
