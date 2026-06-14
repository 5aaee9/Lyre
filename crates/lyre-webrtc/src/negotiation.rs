use std::sync::Arc;

use dashmap::DashMap;
use lyre_core::{RoomId, UserId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ServerMediaSessionConfig, ServerMediaSessionKey, ServerMediaSessionRegistry,
    ServerMediaSessionState, WebRtcPeerConnectionHandle, WebRtcStack, WebRtcStackError,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerMediaOffer {
    pub room_id: RoomId,
    pub user_id: UserId,
    pub audio_track_id: String,
    pub sdp: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerMediaAnswer {
    pub room_id: RoomId,
    pub user_id: UserId,
    pub audio_track_id: String,
    pub sdp: String,
    pub state: ServerMediaSessionState,
}

#[derive(Debug)]
pub struct ServerMediaNegotiator {
    stack: WebRtcStack,
    sessions: Arc<ServerMediaSessionRegistry>,
    peer_connections: DashMap<ServerMediaSessionKey, WebRtcPeerConnectionHandle>,
}

#[derive(Debug, Error)]
pub enum ServerMediaNegotiationError {
    #[error("failed to negotiate server media session")]
    WebRtc {
        #[source]
        source: WebRtcStackError,
    },
    #[error("server media session disappeared during negotiation")]
    SessionMissing,
}

impl ServerMediaNegotiator {
    pub fn new(stack: WebRtcStack, sessions: Arc<ServerMediaSessionRegistry>) -> Self {
        Self {
            stack,
            sessions,
            peer_connections: DashMap::new(),
        }
    }

    pub async fn answer_offer(
        &self,
        offer: ServerMediaOffer,
    ) -> Result<ServerMediaAnswer, ServerMediaNegotiationError> {
        let peer_connection = self
            .stack
            .create_peer_connection()
            .await
            .map_err(|source| ServerMediaNegotiationError::WebRtc { source })?;
        let answer_sdp = peer_connection
            .answer_remote_offer(offer.sdp)
            .await
            .map_err(|source| ServerMediaNegotiationError::WebRtc { source })?;

        let key = ServerMediaSessionKey {
            room_id: offer.room_id.clone(),
            user_id: offer.user_id.clone(),
        };
        self.sessions.start(ServerMediaSessionConfig {
            room_id: offer.room_id.clone(),
            user_id: offer.user_id.clone(),
            audio_track_id: offer.audio_track_id.clone(),
        });
        let status = self
            .sessions
            .set_state(&key, ServerMediaSessionState::Negotiating)
            .ok_or(ServerMediaNegotiationError::SessionMissing)?;
        self.peer_connections.insert(key, peer_connection);

        Ok(ServerMediaAnswer {
            room_id: status.room_id,
            user_id: status.user_id,
            audio_track_id: status.audio_track_id,
            sdp: answer_sdp,
            state: status.state,
        })
    }

    pub fn close(&self, key: &ServerMediaSessionKey) {
        self.sessions.close(key);
        self.peer_connections.remove(key);
    }

    pub fn close_room(&self, room_id: &RoomId) {
        self.sessions.close_room(room_id);
        self.peer_connections
            .retain(|key, _| &key.room_id != room_id);
    }

    pub fn stored_peer_connection_count(&self) -> usize {
        self.peer_connections.len()
    }

    #[cfg(test)]
    fn stored_peer_connection_debug_id(&self, key: &ServerMediaSessionKey) -> Option<usize> {
        self.peer_connections
            .get(key)
            .map(|entry| entry.value().debug_id())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn offer_sdp() -> String {
        let offerer = WebRtcStack::new().create_peer_connection().await.unwrap();
        offerer.create_local_offer_for_test().await.unwrap()
    }

    fn offer(track: &str, sdp: String) -> ServerMediaOffer {
        ServerMediaOffer {
            room_id: RoomId::default_room(),
            user_id: UserId::from_external("user_01"),
            audio_track_id: track.to_owned(),
            sdp,
        }
    }

    #[tokio::test]
    async fn answer_offer_marks_session_negotiating_and_stores_handle() {
        let sessions = Arc::new(ServerMediaSessionRegistry::new());
        let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));

        let answer = negotiator
            .answer_offer(offer("audio-main", offer_sdp().await))
            .await
            .unwrap();

        assert!(answer.sdp.starts_with("v=0"));
        assert_eq!(answer.state, ServerMediaSessionState::Negotiating);
        assert_eq!(
            sessions.active_sessions()[0].state,
            ServerMediaSessionState::Negotiating
        );
        assert_eq!(negotiator.stored_peer_connection_count(), 1);
    }

    #[tokio::test]
    async fn failed_offer_does_not_create_session_or_handle() {
        let sessions = Arc::new(ServerMediaSessionRegistry::new());
        let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));

        let result = negotiator
            .answer_offer(offer("audio-main", "not sdp".to_owned()))
            .await;

        assert!(result.is_err());
        assert!(sessions.sessions().is_empty());
        assert_eq!(negotiator.stored_peer_connection_count(), 0);
    }

    #[tokio::test]
    async fn repeated_successful_offer_replaces_track_and_handle() {
        let sessions = Arc::new(ServerMediaSessionRegistry::new());
        let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));
        let key = ServerMediaSessionKey {
            room_id: RoomId::default_room(),
            user_id: UserId::from_external("user_01"),
        };

        negotiator
            .answer_offer(offer("audio-main", offer_sdp().await))
            .await
            .unwrap();
        let first_handle = negotiator.stored_peer_connection_debug_id(&key).unwrap();
        negotiator
            .answer_offer(offer("audio-retry", offer_sdp().await))
            .await
            .unwrap();
        let second_handle = negotiator.stored_peer_connection_debug_id(&key).unwrap();

        assert_eq!(sessions.sessions().len(), 1);
        assert_eq!(sessions.sessions()[0].audio_track_id, "audio-retry");
        assert_eq!(negotiator.stored_peer_connection_count(), 1);
        assert_ne!(first_handle, second_handle);
    }

    #[tokio::test]
    async fn failed_renegotiation_preserves_existing_session_and_handle() {
        let sessions = Arc::new(ServerMediaSessionRegistry::new());
        let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));
        let key = ServerMediaSessionKey {
            room_id: RoomId::default_room(),
            user_id: UserId::from_external("user_01"),
        };
        negotiator
            .answer_offer(offer("audio-main", offer_sdp().await))
            .await
            .unwrap();
        let status_before = sessions.sessions();
        let handle_before = negotiator.stored_peer_connection_debug_id(&key).unwrap();

        let result = negotiator
            .answer_offer(offer("audio-retry", "not sdp".to_owned()))
            .await;

        assert!(result.is_err());
        assert_eq!(sessions.sessions(), status_before);
        assert_eq!(
            negotiator.stored_peer_connection_debug_id(&key),
            Some(handle_before)
        );
        assert_eq!(negotiator.stored_peer_connection_count(), 1);
    }

    #[tokio::test]
    async fn close_and_close_room_remove_stored_handles() {
        let sessions = Arc::new(ServerMediaSessionRegistry::new());
        let negotiator = ServerMediaNegotiator::new(WebRtcStack::new(), Arc::clone(&sessions));
        negotiator
            .answer_offer(offer("audio-main", offer_sdp().await))
            .await
            .unwrap();

        negotiator.close(&ServerMediaSessionKey {
            room_id: RoomId::default_room(),
            user_id: UserId::from_external("user_01"),
        });

        assert_eq!(negotiator.stored_peer_connection_count(), 0);

        negotiator
            .answer_offer(offer("audio-main", offer_sdp().await))
            .await
            .unwrap();
        negotiator.close_room(&RoomId::default_room());

        assert_eq!(negotiator.stored_peer_connection_count(), 0);
    }
}
