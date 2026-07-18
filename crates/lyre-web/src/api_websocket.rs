use crate::{
    app_state::AppState,
    server_media_ice_diagnostics::{summarize_candidates, ServerMediaIceCandidateSummary},
    signalling::{route_signal_message, SignalMessage, SignalPayload, SIGNAL_CHANNEL_CAPACITY},
};
use axum::extract::ws::{Message, WebSocket};
use futures_util::{Sink, SinkExt, Stream, StreamExt};
use lyre_core::{RoomAccessToken, RoomId, UserId};
use lyre_webrtc::{ServerMediaIceCandidate, ServerMediaSessionKey};
use std::time::Duration;
use tokio::sync::mpsc;

const WEBSOCKET_SEND_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) async fn handle_socket(
    state: AppState,
    room_id: RoomId,
    user_id: UserId,
    access_token: RoomAccessToken,
    socket: WebSocket,
) {
    let (ws_tx, ws_rx) = socket.split();
    handle_socket_io_with_admission(
        state,
        room_id,
        user_id,
        ws_tx,
        ws_rx,
        WEBSOCKET_SEND_TIMEOUT,
        Some(access_token),
    )
    .await;
}

#[cfg(test)]
pub(crate) async fn handle_socket_io<S, St, E>(
    state: AppState,
    room_id: RoomId,
    user_id: UserId,
    ws_tx: S,
    ws_rx: St,
) where
    S: Sink<Message> + Unpin,
    S::Error: std::fmt::Display,
    St: Stream<Item = Result<Message, E>> + Unpin,
    E: std::fmt::Display,
{
    handle_socket_io_with_admission(
        state,
        room_id,
        user_id,
        ws_tx,
        ws_rx,
        WEBSOCKET_SEND_TIMEOUT,
        None,
    )
    .await;
}

#[cfg(test)]
pub(crate) async fn handle_socket_io_with_send_timeout<S, St, E>(
    state: AppState,
    room_id: RoomId,
    user_id: UserId,
    ws_tx: S,
    ws_rx: St,
    send_timeout: Duration,
) where
    S: Sink<Message> + Unpin,
    S::Error: std::fmt::Display,
    St: Stream<Item = Result<Message, E>> + Unpin,
    E: std::fmt::Display,
{
    handle_socket_io_with_admission(state, room_id, user_id, ws_tx, ws_rx, send_timeout, None)
        .await;
}

async fn handle_socket_io_with_admission<S, St, E>(
    state: AppState,
    room_id: RoomId,
    user_id: UserId,
    mut ws_tx: S,
    mut ws_rx: St,
    send_timeout: Duration,
    access_token: Option<RoomAccessToken>,
) where
    S: Sink<Message> + Unpin,
    S::Error: std::fmt::Display,
    St: Stream<Item = Result<Message, E>> + Unpin,
    E: std::fmt::Display,
{
    let (peer_tx, mut peer_rx) = mpsc::channel(SIGNAL_CHANNEL_CAPACITY);
    let peer_identity = peer_tx.downgrade();
    let snapshot = match access_token {
        Some(access_token) => match state
            .connect_room_socket(room_id.clone(), user_id.clone(), &access_token, peer_tx)
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => {
                tracing::debug!(
                    error = ?error,
                    room_id = %room_id,
                    user_id = %user_id,
                    "websocket admission was revoked before upgrade completed"
                );
                return;
            }
        },
        None => state
            .peers
            .connect(&state.registry, room_id.clone(), user_id.clone(), peer_tx),
    };
    let snapshot_message = room_snapshot_message(&room_id, &user_id, snapshot);
    if send_socket_message(
        &mut ws_tx,
        Message::Text(serde_json::to_string(&snapshot_message).unwrap().into()),
        send_timeout,
        "room snapshot",
    )
    .await
    {
        loop {
            tokio::select! {
                message = peer_rx.recv() => {
                    let Some(message) = message else {
                        break;
                    };
                    match serde_json::to_string(&message) {
                        Ok(json) => {
                            if !send_socket_message(
                                &mut ws_tx,
                                Message::Text(json.into()),
                                send_timeout,
                                "signal message",
                            )
                            .await
                            {
                                break;
                            }
                            if peer_rx.is_empty() {
                                if let Some(peer_tx) = peer_identity.upgrade() {
                                    if state.peers.take_room_snapshot_pending(
                                        &room_id,
                                        &user_id,
                                        &peer_tx,
                                    ) {
                                        let snapshot = state.registry.snapshot(room_id.clone());
                                        let snapshot_message =
                                            room_snapshot_message(&room_id, &user_id, snapshot);
                                        let json = serde_json::to_string(&snapshot_message).unwrap();
                                        if !send_socket_message(
                                            &mut ws_tx,
                                            Message::Text(json.into()),
                                            send_timeout,
                                            "room snapshot resync",
                                        )
                                        .await
                                        {
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            tracing::debug!(
                                error = format_args!("{error:#}"),
                                "failed to serialize signal message"
                            );
                        }
                    }
                }
                incoming = ws_rx.next() => {
                    let message = match incoming {
                        Some(Ok(message)) => message,
                        Some(Err(error)) => {
                            tracing::debug!(
                                error = format_args!("{error:#}"),
                                "websocket receive failed"
                            );
                            break;
                        }
                        None => break,
                    };
                    match message {
                        Message::Text(text) => {
                            let response = match serde_json::from_str::<SignalMessage>(&text) {
                                Ok(signal) => {
                                    handle_signal_message(&state, &room_id, &user_id, signal).await
                                }
                                Err(_) => Some(SignalMessage::error(
                                    room_id.clone(),
                                    user_id.clone(),
                                    "invalid signal message",
                                )),
                            };
                            if let Some(response) = response {
                                let json = serde_json::to_string(&response).unwrap();
                                if !send_socket_message(
                                    &mut ws_tx,
                                    Message::Text(json.into()),
                                    send_timeout,
                                    "signal response",
                                )
                                .await
                                {
                                    break;
                                }
                            }
                        }
                        Message::Close(_) => break,
                        _ => {}
                    }
                }
            }
        }
    }

    if let Some(peer_tx) = peer_identity.upgrade() {
        state
            .disconnect_room_socket_if_current(&room_id, &user_id, &peer_tx)
            .await;
    }
}

fn room_snapshot_message(
    room_id: &RoomId,
    user_id: &UserId,
    snapshot: lyre_core::RoomSnapshot,
) -> SignalMessage {
    SignalMessage::new(
        room_id.clone(),
        user_id.clone(),
        Some(user_id.clone()),
        SignalPayload::RoomSnapshot { room: snapshot },
    )
}

async fn send_socket_message<S>(
    ws_tx: &mut S,
    message: Message,
    send_timeout: Duration,
    operation: &'static str,
) -> bool
where
    S: Sink<Message> + Unpin,
    S::Error: std::fmt::Display,
{
    match tokio::time::timeout(send_timeout, ws_tx.send(message)).await {
        Ok(Ok(())) => true,
        Ok(Err(error)) => {
            tracing::debug!(
                error = format_args!("{error:#}"),
                operation,
                "failed to send websocket message"
            );
            false
        }
        Err(_) => {
            tracing::debug!(
                operation,
                send_timeout_ms = %send_timeout.as_millis(),
                "websocket send timed out"
            );
            false
        }
    }
}

pub(crate) async fn handle_signal_message(
    state: &AppState,
    room_id: &RoomId,
    user_id: &UserId,
    signal: SignalMessage,
) -> Option<SignalMessage> {
    match route_signal_message(room_id, user_id, &signal) {
        Ok(_) => handle_valid_signal_message(state, room_id, user_id, signal).await,
        Err(error) => Some(*error),
    }
}

async fn handle_valid_signal_message(
    state: &AppState,
    room_id: &RoomId,
    user_id: &UserId,
    signal: SignalMessage,
) -> Option<SignalMessage> {
    match signal.payload {
        SignalPayload::ServerMediaIceCandidate {
            candidate,
            sdp_mid,
            sdp_mline_index,
            username_fragment,
        } => {
            let candidate = ServerMediaIceCandidate {
                room_id: room_id.clone(),
                user_id: user_id.clone(),
                candidate,
                sdp_mid,
                sdp_mline_index,
                username_fragment,
            };
            let summary = ServerMediaIceCandidateSummary::from_candidate(&candidate);
            match state.add_server_media_ice_candidate(candidate).await {
                Ok(()) => {
                    tracing::debug!(
                        room_id = %room_id,
                        user_id = %user_id,
                        candidate = ?summary,
                        "server media remote ICE candidate accepted over websocket"
                    );
                    None
                }
                Err(error) => {
                    tracing::debug!(
                        room_id = %room_id,
                        user_id = %user_id,
                        candidate = ?summary,
                        error = %format_args!("{error:#}"),
                        "failed to add server media ICE candidate over websocket"
                    );
                    Some(SignalMessage::error(
                        room_id.clone(),
                        user_id.clone(),
                        error.to_string(),
                    ))
                }
            }
        }
        SignalPayload::ServerMediaIceCandidatesRequest => {
            let key = ServerMediaSessionKey {
                room_id: room_id.clone(),
                user_id: user_id.clone(),
            };
            let candidates = state.server_media_ice_candidates(&key);
            let candidate_summaries = summarize_candidates(&candidates);
            tracing::debug!(
                room_id = %room_id,
                user_id = %user_id,
                candidate_count = candidates.len(),
                candidates = ?candidate_summaries,
                "server media local ICE candidates returned over websocket"
            );
            Some(SignalMessage::to_self(
                room_id.clone(),
                user_id.clone(),
                SignalPayload::ServerMediaIceCandidates { candidates },
            ))
        }
        SignalPayload::Offer { .. }
        | SignalPayload::Answer { .. }
        | SignalPayload::IceCandidate { .. }
        | SignalPayload::ServerMediaIceCandidates { .. }
        | SignalPayload::UserJoined { .. }
        | SignalPayload::UserLeft { .. }
        | SignalPayload::RoomSnapshot { .. }
        | SignalPayload::Error { .. } => Some(SignalMessage::error(
            room_id.clone(),
            user_id.clone(),
            "unsupported client signal payload",
        )),
    }
}
