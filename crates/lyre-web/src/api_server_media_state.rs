use crate::{api::AppState, server_media_runtime};
use lyre_core::{MediaRelayError, RoomId};
#[cfg(test)]
use lyre_webrtc::WebRtcPeerConnectionHandle;
use lyre_webrtc::{
    ServerMediaAnswer, ServerMediaDecodeFailure, ServerMediaIceCandidate,
    ServerMediaNegotiationError, ServerMediaOffer, ServerMediaPcmFrame, ServerMediaRemoteTrack,
    ServerMediaRtpPacket, ServerMediaSessionConfig, ServerMediaSessionKey,
    ServerMediaSessionStatus,
};

impl AppState {
    pub fn start_server_media_session(
        &self,
        config: ServerMediaSessionConfig,
    ) -> ServerMediaSessionStatus {
        self.server_media_sessions.start(config)
    }

    pub fn server_media_sessions(&self) -> Vec<ServerMediaSessionStatus> {
        self.server_media_sessions.sessions()
    }

    pub fn active_server_media_sessions(&self) -> Vec<ServerMediaSessionStatus> {
        self.server_media_sessions.active_sessions()
    }

    pub fn close_server_media_sessions_for_room(
        &self,
        room_id: &RoomId,
    ) -> Vec<ServerMediaSessionStatus> {
        self.server_media_negotiator.close_room(room_id);
        self.server_media_sessions.sessions()
    }

    pub async fn answer_server_media_offer(
        &self,
        offer: ServerMediaOffer,
    ) -> Result<ServerMediaAnswer, ServerMediaNegotiationError> {
        self.server_media_negotiator.answer_offer(offer).await
    }

    pub async fn add_server_media_ice_candidate(
        &self,
        candidate: ServerMediaIceCandidate,
    ) -> Result<(), ServerMediaNegotiationError> {
        self.server_media_negotiator
            .add_remote_ice_candidate(candidate)
            .await
    }

    pub fn server_media_ice_candidates(
        &self,
        key: &ServerMediaSessionKey,
    ) -> Vec<ServerMediaIceCandidate> {
        self.server_media_negotiator.local_ice_candidates(key)
    }

    pub fn server_media_remote_tracks(
        &self,
        key: &ServerMediaSessionKey,
    ) -> Vec<ServerMediaRemoteTrack> {
        self.server_media_negotiator.remote_tracks(key)
    }

    pub fn server_media_received_rtp_packets(
        &self,
        key: &ServerMediaSessionKey,
    ) -> Vec<ServerMediaRtpPacket> {
        self.server_media_negotiator.received_rtp_packets(key)
    }

    pub fn drain_server_media_pcm_frames(
        &self,
        key: &ServerMediaSessionKey,
    ) -> Vec<ServerMediaPcmFrame> {
        server_media_runtime::drain_pcm_frames(&self.server_media_negotiator, key)
    }

    pub fn drain_server_media_decode_failures(
        &self,
        key: &ServerMediaSessionKey,
    ) -> Vec<ServerMediaDecodeFailure> {
        server_media_runtime::drain_decode_failures(&self.server_media_negotiator, key)
    }

    pub fn process_server_media_pcm_frame(
        &self,
        key: &ServerMediaSessionKey,
        frame: ServerMediaPcmFrame,
    ) -> Result<(), MediaRelayError> {
        server_media_runtime::process_pcm_frame(&self.media_runtime, key, frame)
    }

    pub fn process_server_media_pcm_frames(
        &self,
        key: &ServerMediaSessionKey,
    ) -> Result<usize, MediaRelayError> {
        server_media_runtime::process_pcm_frames(
            &self.media_runtime,
            &self.server_media_negotiator,
            key,
        )
    }

    #[cfg(test)]
    pub fn server_media_peer_connection_count(&self) -> usize {
        self.server_media_negotiator.stored_peer_connection_count()
    }

    #[cfg(test)]
    pub fn server_media_peer_connection_for_test(
        &self,
        key: &ServerMediaSessionKey,
    ) -> Option<WebRtcPeerConnectionHandle> {
        self.server_media_negotiator.peer_connection_for_test(key)
    }
}
