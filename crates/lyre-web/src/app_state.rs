use crate::{
    error::ApiError,
    media_egress::{ProcessedAudioEgressFanout, ProcessedAudioEgressFrame},
    media_runtime::WebMediaRuntime,
    metrics::MetricsState,
    processed_audio_webrtc_egress_pump::ProcessedAudioWebRtcEgressPump,
    raw_opus_webrtc_egress_pump::RawOpusWebRtcEgressPump,
    server_media_runtime_pump::ServerMediaRuntimePump,
    signalling::PeerHub,
    state_persistence::RoomStatePersistence,
};
use anyhow::Context;
use lyre_core::{
    default_ice_servers, AudioFrame, IceServerConfig, JoinRoomRequest, MediaRelayError,
    MediaRelayRegistry, ProcessedAudioFrame, RoomId, RoomRegistry,
};
use lyre_noise_cancelling::{DeepFilterNetRuntimeConfig, NoiseModelRuntimeConfig};
use lyre_webrtc::{
    ServerMediaNegotiator, ServerMediaPortRange, ServerMediaSessionRegistry, WebRtcStack,
};
use std::{net::IpAddr, sync::Arc};
use tokio::sync::{broadcast, Mutex};

#[derive(Debug, Clone)]
pub struct AppState {
    pub registry: Arc<RoomRegistry>,
    pub media_relays: Arc<MediaRelayRegistry>,
    pub media_runtime: Arc<WebMediaRuntime>,
    pub media_egress: Arc<ProcessedAudioEgressFanout>,
    pub processed_audio_webrtc_egress_pump: Arc<ProcessedAudioWebRtcEgressPump>,
    pub raw_opus_webrtc_egress_pump: Arc<RawOpusWebRtcEgressPump>,
    pub server_media_sessions: Arc<ServerMediaSessionRegistry>,
    pub server_media_negotiator: Arc<ServerMediaNegotiator>,
    pub server_media_runtime_pump: Arc<ServerMediaRuntimePump>,
    pub peers: Arc<PeerHub>,
    pub metrics: Arc<MetricsState>,
    pub ice_servers: Arc<Vec<IceServerConfig>>,
    pub turn_rest_credentials: Option<lyre_core::TurnRestCredentialsConfig>,
    room_state_persistence: Arc<Mutex<Option<RoomStatePersistence>>>,
    pub room_state_persistence_lock: Arc<Mutex<()>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new(default_ice_servers(), None)
    }
}

impl AppState {
    pub fn new(
        ice_servers: Vec<IceServerConfig>,
        turn_rest_credentials: Option<lyre_core::TurnRestCredentialsConfig>,
    ) -> Self {
        Self::with_room_state_persistence(
            ice_servers,
            turn_rest_credentials,
            None,
            DeepFilterNetRuntimeConfig::default(),
        )
        .expect("in-memory AppState construction must not fail")
    }

    pub fn with_room_state_persistence(
        ice_servers: Vec<IceServerConfig>,
        turn_rest_credentials: Option<lyre_core::TurnRestCredentialsConfig>,
        room_state_persistence: Option<RoomStatePersistence>,
        deepfilternet_runtime: DeepFilterNetRuntimeConfig,
    ) -> anyhow::Result<Self> {
        Self::with_room_state_persistence_and_server_media_public_ip(
            ice_servers,
            turn_rest_credentials,
            room_state_persistence,
            deepfilternet_runtime,
            None,
            None,
        )
    }

    pub fn with_room_state_persistence_and_server_media_public_ip(
        ice_servers: Vec<IceServerConfig>,
        turn_rest_credentials: Option<lyre_core::TurnRestCredentialsConfig>,
        room_state_persistence: Option<RoomStatePersistence>,
        deepfilternet_runtime: DeepFilterNetRuntimeConfig,
        server_media_public_ip: Option<IpAddr>,
        server_media_port_range: Option<ServerMediaPortRange>,
    ) -> anyhow::Result<Self> {
        Self::with_room_state_persistence_server_media_and_noise_model_runtime(
            ice_servers,
            turn_rest_credentials,
            room_state_persistence,
            NoiseModelRuntimeConfig {
                deepfilternet: deepfilternet_runtime,
                ..NoiseModelRuntimeConfig::default()
            },
            server_media_public_ip,
            server_media_port_range,
        )
    }

    pub fn with_room_state_persistence_server_media_and_noise_model_runtime(
        ice_servers: Vec<IceServerConfig>,
        turn_rest_credentials: Option<lyre_core::TurnRestCredentialsConfig>,
        room_state_persistence: Option<RoomStatePersistence>,
        model_runtime: NoiseModelRuntimeConfig,
        server_media_public_ip: Option<IpAddr>,
        server_media_port_range: Option<ServerMediaPortRange>,
    ) -> anyhow::Result<Self> {
        let deepfilternet_runtime = model_runtime
            .deepfilternet
            .validate()
            .map_err(anyhow::Error::from)
            .context("invalid DeepFilterNet runtime config")?;
        let model_runtime = NoiseModelRuntimeConfig {
            deepfilternet: deepfilternet_runtime,
            ..model_runtime
        };
        let registry = match &room_state_persistence {
            Some(persistence) => persistence.load_registry()?,
            None => RoomRegistry::new(),
        };
        let media_relays = Arc::new(MediaRelayRegistry::new());
        let server_media_sessions = Arc::new(ServerMediaSessionRegistry::new());
        let server_media_negotiator = Arc::new(ServerMediaNegotiator::new(
            WebRtcStack::with_server_media_config(server_media_public_ip, server_media_port_range),
            Arc::clone(&server_media_sessions),
        ));
        let media_runtime = Arc::new(WebMediaRuntime::with_noise_model_runtime(
            Arc::clone(&media_relays),
            model_runtime,
        ));
        let server_media_runtime_pump = Arc::new(ServerMediaRuntimePump::new(
            Arc::clone(&media_runtime),
            Arc::clone(&server_media_negotiator),
        ));
        let media_egress = Arc::new(ProcessedAudioEgressFanout::new(Arc::clone(&media_relays)));
        let processed_audio_webrtc_egress_pump = Arc::new(ProcessedAudioWebRtcEgressPump::new(
            Arc::clone(&media_runtime),
            Arc::clone(&media_egress),
            Arc::clone(&server_media_negotiator),
        ));
        let raw_opus_webrtc_egress_pump = Arc::new(RawOpusWebRtcEgressPump::new(
            Arc::clone(&media_relays),
            Arc::clone(&server_media_negotiator),
        ));
        Ok(Self {
            registry: Arc::new(registry),
            media_runtime,
            media_egress,
            processed_audio_webrtc_egress_pump,
            raw_opus_webrtc_egress_pump,
            server_media_sessions,
            server_media_negotiator,
            server_media_runtime_pump,
            media_relays,
            peers: Arc::new(PeerHub::new()),
            metrics: Arc::new(MetricsState::default()),
            ice_servers: Arc::new(ice_servers),
            turn_rest_credentials,
            room_state_persistence: Arc::new(Mutex::new(room_state_persistence)),
            room_state_persistence_lock: Arc::new(Mutex::new(())),
        })
    }

    #[cfg(test)]
    pub async fn set_room_state_persistence_for_tests(
        &self,
        persistence: Option<RoomStatePersistence>,
    ) {
        *self.room_state_persistence.lock().await = persistence;
    }

    pub fn process_media_frame(&self, frame: AudioFrame) -> Result<(), MediaRelayError> {
        self.media_runtime.process_frame(frame)
    }

    pub fn processed_media_frames(&self, room_id: &RoomId) -> Vec<ProcessedAudioFrame> {
        self.media_runtime.frames_for_room(room_id)
    }

    pub fn processed_audio_egress_frames(
        &self,
        frame: &ProcessedAudioFrame,
    ) -> Result<Vec<ProcessedAudioEgressFrame>, MediaRelayError> {
        self.media_egress.fanout(frame)
    }

    pub fn subscribe_processed_media_frames(
        &self,
        room_id: &RoomId,
    ) -> broadcast::Receiver<ProcessedAudioFrame> {
        self.media_runtime.subscribe(room_id)
    }

    pub fn clear_processed_media_room(&self, room_id: &RoomId) {
        self.media_runtime.clear_room(room_id);
    }

    pub fn stop_media_relay(
        &self,
        room_id: RoomId,
        request: lyre_core::StopMediaRelayRequest,
    ) -> lyre_core::MediaRelayRoomStatus {
        self.processed_audio_webrtc_egress_pump.stop(&room_id);
        self.raw_opus_webrtc_egress_pump.stop(&room_id);
        let status = self.media_relays.stop(room_id.clone(), request);
        self.clear_processed_media_room(&room_id);
        self.close_server_media_sessions_for_room(&room_id);
        status
    }

    pub fn start_media_relay(
        &self,
        room_id: RoomId,
        request: lyre_core::StartMediaRelayRequest,
    ) -> lyre_core::MediaRelayRoomStatus {
        let status = self.media_relays.start(room_id.clone(), request);
        self.apply_media_relay_noise(&room_id, &status.noise);
        status
    }

    pub fn update_media_relay_settings(
        &self,
        room_id: RoomId,
        request: lyre_core::UpdateMediaRelaySettingsRequest,
    ) -> Result<lyre_core::MediaRelayRoomStatus, MediaRelayError> {
        let status = self
            .media_relays
            .update_settings(room_id.clone(), request)?;
        self.apply_media_relay_noise(&room_id, &status.noise);
        Ok(status)
    }

    fn apply_media_relay_noise(
        &self,
        room_id: &RoomId,
        noise: &lyre_core::NoiseCancellationConfig,
    ) {
        if noise.provider == lyre_core::NoiseProvider::Off {
            self.processed_audio_webrtc_egress_pump.stop(room_id);
            self.raw_opus_webrtc_egress_pump.start(room_id.clone());
        } else {
            self.raw_opus_webrtc_egress_pump.stop(room_id);
            self.processed_audio_webrtc_egress_pump
                .start(room_id.clone());
        }
    }

    pub async fn join_room_persisted(
        &self,
        room_id: RoomId,
        request: JoinRoomRequest,
    ) -> Result<lyre_core::JoinRoomResponse, ApiError> {
        let _guard = self.room_state_persistence_lock.lock().await;
        let persistence = self.room_state_persistence.lock().await.clone();
        let Some(persistence) = persistence else {
            let response = self.registry.join(room_id, request);
            self.metrics.record_join();
            return Ok(response);
        };
        let rollback = self.registry.to_persisted();
        let response = self.registry.join(room_id, request);
        if let Err(error) = persistence.save_registry(&self.registry) {
            self.registry.replace_with_persisted(rollback);
            self.metrics.record_persistence_failure();
            return Err(ApiError::from(error));
        }
        self.metrics.record_join();
        Ok(response)
    }

    pub async fn leave_room_persisted(
        &self,
        room_id: &RoomId,
        user_id: &lyre_core::UserId,
    ) -> Result<lyre_core::LeaveRoomResponse, ApiError> {
        let _guard = self.room_state_persistence_lock.lock().await;
        let persistence = self.room_state_persistence.lock().await.clone();
        let Some(persistence) = persistence else {
            let response = self.registry.leave(room_id, user_id);
            if response.removed {
                self.close_departed_user_server_media_state(room_id, user_id);
                self.metrics.record_leave();
            }
            return Ok(response);
        };
        let rollback = self.registry.to_persisted();
        let response = self.registry.leave(room_id, user_id);
        if let Err(error) = persistence.save_registry(&self.registry) {
            self.registry.replace_with_persisted(rollback);
            self.metrics.record_persistence_failure();
            return Err(ApiError::from(error));
        }
        if response.removed {
            self.close_departed_user_server_media_state(room_id, user_id);
            self.metrics.record_leave();
        }
        Ok(response)
    }

    pub async fn disconnect_room_socket(&self, room_id: &RoomId, user_id: &lyre_core::UserId) {
        self.peers.remove_peer(room_id, user_id);
        match self.leave_room_persisted(room_id, user_id).await {
            Ok(response) if response.removed => {
                self.peers.user_left(room_id, user_id);
            }
            Ok(_) => {}
            Err(ApiError::Persistence(error)) => {
                tracing::warn!(
                    error = format_args!("{error:#}"),
                    "failed to leave room after websocket disconnect"
                );
            }
            Err(error) => {
                tracing::warn!(
                    error = ?error,
                    "failed to leave room after websocket disconnect"
                );
            }
        }
    }
}
