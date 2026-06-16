use dashmap::DashMap;
use lyre_core::{RoomId, RoomRegistry, RoomSnapshot, UserId, UserProfile};
use lyre_webrtc::ServerMediaIceCandidate;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SignalKind {
    Offer,
    Answer,
    IceCandidate,
    ServerMediaIceCandidate,
    ServerMediaIceCandidatesRequest,
    ServerMediaIceCandidates,
    UserJoined,
    UserLeft,
    RoomSnapshot,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SignalPayload {
    Offer {
        sdp: String,
    },
    Answer {
        sdp: String,
    },
    IceCandidate {
        candidate: String,
        sdp_mid: Option<String>,
        sdp_m_line_index: Option<u16>,
    },
    ServerMediaIceCandidate {
        candidate: String,
        sdp_mid: Option<String>,
        sdp_mline_index: Option<u16>,
        username_fragment: Option<String>,
    },
    ServerMediaIceCandidatesRequest,
    ServerMediaIceCandidates {
        candidates: Vec<ServerMediaIceCandidate>,
    },
    UserJoined {
        user: UserProfile,
    },
    UserLeft {
        user_id: UserId,
    },
    RoomSnapshot {
        room: RoomSnapshot,
    },
    Error {
        message: String,
    },
}

impl SignalPayload {
    fn kind(&self) -> SignalKind {
        match self {
            Self::Offer { .. } => SignalKind::Offer,
            Self::Answer { .. } => SignalKind::Answer,
            Self::IceCandidate { .. } => SignalKind::IceCandidate,
            Self::ServerMediaIceCandidate { .. } => SignalKind::ServerMediaIceCandidate,
            Self::ServerMediaIceCandidatesRequest => SignalKind::ServerMediaIceCandidatesRequest,
            Self::ServerMediaIceCandidates { .. } => SignalKind::ServerMediaIceCandidates,
            Self::UserJoined { .. } => SignalKind::UserJoined,
            Self::UserLeft { .. } => SignalKind::UserLeft,
            Self::RoomSnapshot { .. } => SignalKind::RoomSnapshot,
            Self::Error { .. } => SignalKind::Error,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignalMessage {
    #[serde(rename = "type")]
    pub kind: SignalKind,
    pub room_id: RoomId,
    pub sender_id: UserId,
    pub recipient_id: Option<UserId>,
    pub payload: SignalPayload,
}

impl SignalMessage {
    pub fn new(
        room_id: RoomId,
        sender_id: UserId,
        recipient_id: Option<UserId>,
        payload: SignalPayload,
    ) -> Self {
        Self {
            kind: payload.kind(),
            room_id,
            sender_id,
            recipient_id,
            payload,
        }
    }

    pub fn to_self(room_id: RoomId, user_id: UserId, payload: SignalPayload) -> Self {
        Self::new(room_id, user_id.clone(), Some(user_id), payload)
    }

    pub fn error(room_id: RoomId, sender_id: UserId, message: impl Into<String>) -> Self {
        Self::new(
            room_id,
            sender_id,
            None,
            SignalPayload::Error {
                message: message.into(),
            },
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalRoute {
    Target(UserId),
    BroadcastExceptSender,
}

pub fn route_signal_message(
    path_room: &RoomId,
    query_user: &UserId,
    message: &SignalMessage,
) -> Result<SignalRoute, Box<SignalMessage>> {
    if &message.room_id != path_room {
        return Err(Box::new(SignalMessage::error(
            path_room.clone(),
            query_user.clone(),
            "message room_id does not match websocket room",
        )));
    }
    if &message.sender_id != query_user {
        return Err(Box::new(SignalMessage::error(
            path_room.clone(),
            query_user.clone(),
            "message sender_id does not match websocket user",
        )));
    }

    Ok(message
        .recipient_id
        .clone()
        .map(SignalRoute::Target)
        .unwrap_or(SignalRoute::BroadcastExceptSender))
}

pub type PeerSender = mpsc::UnboundedSender<SignalMessage>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalDelivery {
    pub delivered: usize,
}

#[derive(Debug, Default)]
pub struct PeerHub {
    peers: DashMap<RoomId, DashMap<UserId, PeerSender>>,
}

impl PeerHub {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn connect(
        &self,
        registry: &RoomRegistry,
        room_id: RoomId,
        user_id: UserId,
        tx: PeerSender,
    ) -> RoomSnapshot {
        let snapshot = registry.snapshot(room_id.clone());
        self.peers.entry(room_id).or_default().insert(user_id, tx);
        snapshot
    }

    pub fn disconnect(&self, room_id: &RoomId, user_id: &UserId) -> SignalDelivery {
        self.remove_peer(room_id, user_id);
        self.user_left(room_id, user_id)
    }

    pub fn remove_peer(&self, room_id: &RoomId, user_id: &UserId) {
        if let Some(room) = self.peers.get(room_id) {
            room.remove(user_id);
        }
    }

    pub fn user_joined(&self, room_id: &RoomId, user: UserProfile) -> SignalDelivery {
        let sender_id = user.id.clone();
        let message = SignalMessage::new(
            room_id.clone(),
            sender_id.clone(),
            None,
            SignalPayload::UserJoined { user },
        );
        self.broadcast_except(room_id, &sender_id, message)
    }

    pub fn user_left(&self, room_id: &RoomId, user_id: &UserId) -> SignalDelivery {
        let message = SignalMessage::new(
            room_id.clone(),
            user_id.clone(),
            None,
            SignalPayload::UserLeft {
                user_id: user_id.clone(),
            },
        );
        self.broadcast_except(room_id, user_id, message)
    }

    pub fn forward(&self, message: SignalMessage) -> SignalDelivery {
        if let Some(recipient_id) = message.recipient_id.clone() {
            let room_id = message.room_id.clone();
            return self.send_to(&room_id, &recipient_id, message);
        }
        let room_id = message.room_id.clone();
        let sender_id = message.sender_id.clone();
        self.broadcast_except(&room_id, &sender_id, message)
    }

    fn send_to(
        &self,
        room_id: &RoomId,
        user_id: &UserId,
        message: SignalMessage,
    ) -> SignalDelivery {
        let delivered = self
            .peers
            .get(room_id)
            .and_then(|room| room.get(user_id).map(|sender| sender.send(message).is_ok()))
            .unwrap_or(false) as usize;
        SignalDelivery { delivered }
    }

    fn broadcast_except(
        &self,
        room_id: &RoomId,
        sender_id: &UserId,
        message: SignalMessage,
    ) -> SignalDelivery {
        let delivered = self
            .peers
            .get(room_id)
            .map(|room| {
                room.iter()
                    .filter(|entry| entry.key() != sender_id)
                    .filter(|entry| entry.value().send(message.clone()).is_ok())
                    .count()
            })
            .unwrap_or(0);
        SignalDelivery { delivered }
    }
}
