use crate::{
    api::AppState, api_server_media_state::CloseServerMediaSessionResponse, error::ApiError,
};
use lyre_core::{
    media::{MediaRelaySubscriptions, UpdateMediaRelaySubscriptionsRequest},
    MediaRelayRoomStatus, RegisterMediaTrackRequest, RoomAccessToken, RoomId,
    StartMediaRelayRequest, StopMediaRelayRequest, UpdateMediaRelaySettingsRequest, UserId,
};
use lyre_webrtc::ServerMediaIceCandidate;

impl AppState {
    pub(crate) async fn start_authorized_media_relay(
        &self,
        room_id: RoomId,
        request: StartMediaRelayRequest,
        access_token: &RoomAccessToken,
    ) -> Result<MediaRelayRoomStatus, ApiError> {
        let _guard = self.server_media_lifecycle.lock(&room_id).await;
        self.registry
            .validate_any_access_token(&room_id, access_token)
            .map_err(|_| ApiError::Unauthorized)?;
        Ok(self.start_media_relay(room_id, request))
    }

    pub(crate) async fn stop_authorized_media_relay(
        &self,
        room_id: RoomId,
        request: StopMediaRelayRequest,
        access_token: &RoomAccessToken,
    ) -> Result<MediaRelayRoomStatus, ApiError> {
        let _guard = self.server_media_lifecycle.lock(&room_id).await;
        self.registry
            .validate_access_token(&room_id, &request.user_id, access_token)
            .map_err(|_| ApiError::Unauthorized)?;
        Ok(self.stop_media_relay_locked(room_id, request).await)
    }

    pub(crate) async fn register_authorized_media_track(
        &self,
        room_id: RoomId,
        request: RegisterMediaTrackRequest,
        access_token: &RoomAccessToken,
    ) -> Result<MediaRelayRoomStatus, ApiError> {
        let _guard = self.server_media_lifecycle.lock(&room_id).await;
        self.registry
            .validate_access_token(&room_id, &request.user_id, access_token)
            .map_err(|_| ApiError::Unauthorized)?;
        Ok(self.media_relays.register_track(room_id, request)?)
    }

    pub(crate) async fn register_authorized_media_participant(
        &self,
        room_id: RoomId,
        user_id: UserId,
        access_token: &RoomAccessToken,
    ) -> Result<MediaRelayRoomStatus, ApiError> {
        let _guard = self.server_media_lifecycle.lock(&room_id).await;
        self.registry
            .validate_access_token(&room_id, &user_id, access_token)
            .map_err(|_| ApiError::Unauthorized)?;
        Ok(self.media_relays.register_participant(room_id, user_id)?)
    }

    pub(crate) async fn update_authorized_media_relay_subscriptions(
        &self,
        room_id: RoomId,
        request: UpdateMediaRelaySubscriptionsRequest,
        access_token: &RoomAccessToken,
    ) -> Result<MediaRelaySubscriptions, ApiError> {
        let _guard = self.server_media_lifecycle.lock(&room_id).await;
        self.registry
            .validate_access_token(&room_id, &request.user_id, access_token)
            .map_err(|_| ApiError::Unauthorized)?;
        Ok(self.media_relays.update_subscriptions(room_id, request)?)
    }

    pub(crate) async fn update_authorized_media_relay_settings(
        &self,
        room_id: RoomId,
        request: UpdateMediaRelaySettingsRequest,
        access_token: &RoomAccessToken,
    ) -> Result<MediaRelayRoomStatus, ApiError> {
        let _guard = self.server_media_lifecycle.lock(&room_id).await;
        self.registry
            .validate_access_token(&room_id, &request.user_id, access_token)
            .map_err(|_| ApiError::Unauthorized)?;
        Ok(self.update_media_relay_settings(room_id, request)?)
    }

    pub(crate) async fn close_authorized_server_media_session_for_user(
        &self,
        room_id: RoomId,
        user_id: UserId,
        access_token: &RoomAccessToken,
    ) -> Result<CloseServerMediaSessionResponse, ApiError> {
        let _guard = self.server_media_lifecycle.lock(&room_id).await;
        self.registry
            .validate_access_token(&room_id, &user_id, access_token)
            .map_err(|_| ApiError::Unauthorized)?;
        Ok(self
            .close_server_media_session_for_user_locked(room_id, user_id)
            .await?)
    }

    pub(crate) async fn add_authorized_server_media_ice_candidate(
        &self,
        candidate: ServerMediaIceCandidate,
        access_token: &RoomAccessToken,
    ) -> Result<(), ApiError> {
        let _guard = self.server_media_lifecycle.lock(&candidate.room_id).await;
        self.registry
            .validate_access_token(&candidate.room_id, &candidate.user_id, access_token)
            .map_err(|_| ApiError::Unauthorized)?;
        self.add_server_media_ice_candidate(candidate).await?;
        Ok(())
    }
}
