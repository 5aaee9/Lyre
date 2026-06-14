use std::sync::Arc;

use dashmap::DashMap;
use lyre_core::{RoomId, UserId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ServerMediaDecodeFailure, ServerMediaIceCandidateInit, ServerMediaPcmFrame,
    ServerMediaRemoteTrack, ServerMediaRtpPacket, ServerMediaSessionConfig, ServerMediaSessionKey,
    ServerMediaSessionRegistry, ServerMediaSessionState, WebRtcPeerConnectionHandle, WebRtcStack,
    WebRtcStackError,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerMediaIceCandidate {
    pub room_id: RoomId,
    pub user_id: UserId,
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_mline_index: Option<u16>,
    pub username_fragment: Option<String>,
}

impl ServerMediaIceCandidate {
    fn key(&self) -> ServerMediaSessionKey {
        ServerMediaSessionKey {
            room_id: self.room_id.clone(),
            user_id: self.user_id.clone(),
        }
    }

    fn init(&self) -> ServerMediaIceCandidateInit {
        ServerMediaIceCandidateInit {
            candidate: self.candidate.clone(),
            sdp_mid: self.sdp_mid.clone(),
            sdp_mline_index: self.sdp_mline_index,
            username_fragment: self.username_fragment.clone(),
        }
    }

    fn from_init(key: &ServerMediaSessionKey, candidate: ServerMediaIceCandidateInit) -> Self {
        Self {
            room_id: key.room_id.clone(),
            user_id: key.user_id.clone(),
            candidate: candidate.candidate,
            sdp_mid: candidate.sdp_mid,
            sdp_mline_index: candidate.sdp_mline_index,
            username_fragment: candidate.username_fragment,
        }
    }
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

    pub async fn add_remote_ice_candidate(
        &self,
        candidate: ServerMediaIceCandidate,
    ) -> Result<(), ServerMediaNegotiationError> {
        let key = candidate.key();
        let peer_connection = self
            .peer_connections
            .get(&key)
            .ok_or(ServerMediaNegotiationError::SessionMissing)?
            .clone();
        peer_connection
            .add_remote_ice_candidate(candidate.init())
            .await
            .map_err(|source| ServerMediaNegotiationError::WebRtc { source })?;
        Ok(())
    }

    pub fn local_ice_candidates(
        &self,
        key: &ServerMediaSessionKey,
    ) -> Vec<ServerMediaIceCandidate> {
        self.peer_connections
            .get(key)
            .map(|peer_connection| {
                peer_connection
                    .local_ice_candidates()
                    .into_iter()
                    .map(|candidate| ServerMediaIceCandidate::from_init(key, candidate))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn remote_tracks(&self, key: &ServerMediaSessionKey) -> Vec<ServerMediaRemoteTrack> {
        self.peer_connections
            .get(key)
            .map(|peer_connection| peer_connection.remote_tracks())
            .unwrap_or_default()
    }

    pub fn received_rtp_packets(&self, key: &ServerMediaSessionKey) -> Vec<ServerMediaRtpPacket> {
        self.peer_connections
            .get(key)
            .map(|peer_connection| peer_connection.received_rtp_packets())
            .unwrap_or_default()
    }

    pub fn drain_pcm_frames(&self, key: &ServerMediaSessionKey) -> Vec<ServerMediaPcmFrame> {
        self.peer_connections
            .get(key)
            .map(|peer_connection| peer_connection.drain_pcm_frames())
            .unwrap_or_default()
    }

    pub fn drain_decode_failures(
        &self,
        key: &ServerMediaSessionKey,
    ) -> Vec<ServerMediaDecodeFailure> {
        self.peer_connections
            .get(key)
            .map(|peer_connection| peer_connection.drain_decode_failures())
            .unwrap_or_default()
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
    pub(crate) fn stored_peer_connection_debug_id(
        &self,
        key: &ServerMediaSessionKey,
    ) -> Option<usize> {
        self.peer_connections
            .get(key)
            .map(|entry| entry.value().debug_id())
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn peer_connection_for_test(
        &self,
        key: &ServerMediaSessionKey,
    ) -> Option<WebRtcPeerConnectionHandle> {
        self.peer_connections
            .get(key)
            .map(|entry| entry.value().clone())
    }
}
