use crate::{api::AppState, error::ApiError, server_media_runtime};
use lyre_core::{
    MediaRelayError, MediaRelayRoomStatus, RoomAccessToken, RoomId, StopMediaRelayRequest, UserId,
};
#[cfg(test)]
use lyre_webrtc::WebRtcPeerConnectionHandle;
use lyre_webrtc::{
    ServerMediaAnswer, ServerMediaDecodeFailure, ServerMediaIceCandidate,
    ServerMediaNegotiationError, ServerMediaOffer, ServerMediaPcmFrame, ServerMediaRemoteTrack,
    ServerMediaRtpPacket, ServerMediaSessionConfig, ServerMediaSessionKey,
    ServerMediaSessionStatus,
};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CloseServerMediaSessionResponse {
    pub media_relay: MediaRelayRoomStatus,
    pub session: Option<ServerMediaSessionStatus>,
}

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

    pub async fn close_server_media_sessions_for_room(
        &self,
        room_id: &RoomId,
    ) -> Vec<ServerMediaSessionStatus> {
        let _guard = self.server_media_lifecycle.lock(room_id).await;
        self.close_server_media_sessions_for_room_locked(room_id)
            .await
    }

    pub(crate) async fn close_server_media_sessions_for_room_locked(
        &self,
        room_id: &RoomId,
    ) -> Vec<ServerMediaSessionStatus> {
        self.processed_audio_webrtc_egress_pump.stop(room_id);
        self.raw_opus_webrtc_egress_pump.stop(room_id);
        self.server_media_runtime_pump
            .stop_room_and_wait(room_id)
            .await;
        self.server_media_negotiator.close_room(room_id);
        self.media_runtime.clear_room(room_id);
        self.server_media_sessions.sessions()
    }

    pub async fn close_server_media_session_for_user(
        &self,
        room_id: RoomId,
        user_id: UserId,
    ) -> Result<CloseServerMediaSessionResponse, MediaRelayError> {
        let _guard = self.server_media_lifecycle.lock(&room_id).await;
        self.close_server_media_session_for_user_locked(room_id, user_id)
            .await
    }

    pub(crate) async fn close_server_media_session_for_user_locked(
        &self,
        room_id: RoomId,
        user_id: UserId,
    ) -> Result<CloseServerMediaSessionResponse, MediaRelayError> {
        let key = ServerMediaSessionKey {
            room_id: room_id.clone(),
            user_id: user_id.clone(),
        };
        self.server_media_runtime_pump.stop_and_wait(&key).await;
        let session = self.server_media_negotiator.close(&key);
        let media_relay = self.media_relays.remove_participant(room_id, &user_id);
        self.processed_audio_webrtc_egress_pump
            .remove_recipient(&key.room_id, &key.user_id);
        self.media_runtime
            .release_source(&key.room_id, &key.user_id);
        let media_relay = media_relay?;
        Ok(CloseServerMediaSessionResponse {
            media_relay,
            session,
        })
    }

    pub(crate) async fn close_departed_user_server_media_state(
        &self,
        room_id: &RoomId,
        user_id: &UserId,
    ) {
        let _guard = self.server_media_lifecycle.lock(room_id).await;
        self.close_departed_user_server_media_state_locked(room_id, user_id)
            .await;
    }

    async fn close_departed_user_server_media_state_locked(
        &self,
        room_id: &RoomId,
        user_id: &UserId,
    ) {
        let key = ServerMediaSessionKey {
            room_id: room_id.clone(),
            user_id: user_id.clone(),
        };
        self.server_media_runtime_pump.stop_and_wait(&key).await;
        self.server_media_negotiator.close(&key);
        let media_relay = match self
            .media_relays
            .remove_participant(room_id.clone(), user_id)
        {
            Ok(media_relay) => Some(media_relay),
            Err(MediaRelayError::Inactive { .. }) => None,
            Err(error) => {
                tracing::debug!(
                    error = format_args!("{error:#}"),
                    room_id = %room_id,
                    user_id = %user_id,
                    "failed to remove departed user from media relay"
                );
                None
            }
        };
        self.processed_audio_webrtc_egress_pump
            .remove_recipient(room_id, user_id);
        self.media_runtime.release_source(room_id, user_id);
        if media_relay
            .as_ref()
            .is_some_and(|status| status.participants.is_empty())
        {
            self.media_relays.stop(
                room_id.clone(),
                StopMediaRelayRequest {
                    user_id: user_id.clone(),
                },
            );
            self.media_runtime.clear_room(room_id);
            self.close_server_media_sessions_for_room_locked(room_id)
                .await;
        }
    }

    pub async fn answer_server_media_offer(
        &self,
        offer: ServerMediaOffer,
    ) -> Result<ServerMediaAnswer, ServerMediaNegotiationError> {
        self.answer_server_media_offer_for_sources(offer, Vec::new())
            .await
    }

    pub async fn answer_server_media_offer_with_subscriptions(
        &self,
        offer: ServerMediaOffer,
    ) -> Result<ServerMediaAnswer, ApiError> {
        let _guard = self.server_media_lifecycle.lock(&offer.room_id).await;
        let subscribed_source_user_ids = self
            .media_relays
            .subscriptions(&offer.room_id, &offer.user_id)?
            .source_user_ids;
        self.answer_server_media_offer_for_sources_locked(offer, subscribed_source_user_ids)
            .await
            .map_err(Into::into)
    }

    pub async fn answer_authorized_server_media_offer_with_subscriptions(
        &self,
        offer: ServerMediaOffer,
        access_token: &RoomAccessToken,
    ) -> Result<ServerMediaAnswer, ApiError> {
        let _guard = self.server_media_lifecycle.lock(&offer.room_id).await;
        self.registry
            .validate_access_token(&offer.room_id, &offer.user_id, access_token)
            .map_err(|_| ApiError::Unauthorized)?;
        let subscribed_source_user_ids = self
            .media_relays
            .subscriptions(&offer.room_id, &offer.user_id)?
            .source_user_ids;
        self.answer_server_media_offer_for_sources_locked(offer, subscribed_source_user_ids)
            .await
            .map_err(Into::into)
    }

    async fn answer_server_media_offer_for_sources(
        &self,
        offer: ServerMediaOffer,
        subscribed_source_user_ids: Vec<UserId>,
    ) -> Result<ServerMediaAnswer, ServerMediaNegotiationError> {
        let _guard = self.server_media_lifecycle.lock(&offer.room_id).await;
        self.answer_server_media_offer_for_sources_locked(offer, subscribed_source_user_ids)
            .await
    }

    async fn answer_server_media_offer_for_sources_locked(
        &self,
        offer: ServerMediaOffer,
        subscribed_source_user_ids: Vec<UserId>,
    ) -> Result<ServerMediaAnswer, ServerMediaNegotiationError> {
        let key = ServerMediaSessionKey {
            room_id: offer.room_id.clone(),
            user_id: offer.user_id.clone(),
        };
        let replaces_audio_track = self
            .server_media_negotiator
            .session_status(&key)
            .is_some_and(|status| status.audio_track_id != offer.audio_track_id);
        let answer = self
            .server_media_negotiator
            .answer_offer_for_sources(offer, subscribed_source_user_ids)
            .await?;
        self.server_media_runtime_pump.stop_and_wait(&key).await;
        #[cfg(test)]
        self.server_media_lifecycle
            .pause_before_pump_if_requested()
            .await;
        if replaces_audio_track {
            self.media_runtime
                .release_source(&answer.room_id, &answer.user_id);
        }
        self.server_media_runtime_pump.start(key);
        Ok(answer)
    }

    #[cfg(test)]
    pub fn pause_next_server_media_offer_before_pump_for_test(&self) {
        self.server_media_lifecycle.pause_next_offer_before_pump();
    }

    #[cfg(test)]
    pub async fn wait_for_server_media_offer_before_pump_for_test(&self) {
        self.server_media_lifecycle
            .wait_for_offer_before_pump()
            .await;
    }

    #[cfg(test)]
    pub fn resume_server_media_offer_for_test(&self) {
        self.server_media_lifecycle.resume_offer();
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

    pub fn server_media_runtime_pump_count(&self) -> usize {
        self.server_media_runtime_pump.task_count()
    }

    pub fn processed_audio_webrtc_egress_pump_count(&self) -> usize {
        self.processed_audio_webrtc_egress_pump.task_count()
    }

    pub fn raw_opus_webrtc_egress_pump_count(&self) -> usize {
        self.raw_opus_webrtc_egress_pump.task_count()
    }

    #[cfg(test)]
    pub fn server_media_peer_connection_for_test(
        &self,
        key: &ServerMediaSessionKey,
    ) -> Option<WebRtcPeerConnectionHandle> {
        self.server_media_negotiator.peer_connection_for_test(key)
    }
}
