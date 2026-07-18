mod pending_websocket_admission;

use crate::{
    media_egress::{ProcessedAudioEgressFanout, ProcessedAudioEgressFrame},
    media_runtime::WebMediaRuntime,
    metrics::MetricsState,
    processed_audio_webrtc_egress_pump::ProcessedAudioWebRtcEgressPump,
    raw_opus_webrtc_egress_pump::RawOpusWebRtcEgressPump,
    server_media_lifecycle::ServerMediaLifecycle,
    server_media_runtime_pump::ServerMediaRuntimePump,
    signalling::PeerHub,
    state_persistence::RoomStatePersistence,
};
use anyhow::Context;
use lyre_core::{
    default_ice_servers, AudioFrame, IceServerConfig, MediaRelayError, MediaRelayRegistry,
    ProcessedAudioFrame, RoomId, RoomRegistry,
};
use lyre_noise_cancelling::{DeepFilterNetRuntimeConfig, NoiseModelRuntimeConfig};
use lyre_webrtc::{
    ServerMediaNegotiator, ServerMediaPortRange, ServerMediaSessionRegistry, WebRtcStack,
};
use std::{net::IpAddr, sync::Arc};
use tokio::sync::{broadcast, Mutex};

use self::pending_websocket_admission::PendingWebSocketAdmissions;

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
    pub(crate) server_media_lifecycle: Arc<ServerMediaLifecycle>,
    pub peers: Arc<PeerHub>,
    pub metrics: Arc<MetricsState>,
    pub ice_servers: Arc<Vec<IceServerConfig>>,
    pub turn_rest_credentials: Option<lyre_core::TurnRestCredentialsConfig>,
    room_state_persistence: Arc<Mutex<Option<RoomStatePersistence>>>,
    pub room_state_persistence_lock: Arc<Mutex<()>>,
    pending_websocket_admissions: Arc<PendingWebSocketAdmissions>,
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
        let expired_pending_websocket_admissions = registry
            .pending_websocket_admissions()
            .into_iter()
            .filter(|(_, _, expires_at)| *expires_at <= chrono::Utc::now())
            .collect::<Vec<_>>();
        for (room_id, user_id, _) in &expired_pending_websocket_admissions {
            registry.leave(room_id, user_id);
        }
        if !expired_pending_websocket_admissions.is_empty() {
            room_state_persistence
                .as_ref()
                .expect("pending websocket admissions only come from persisted state")
                .save_registry(&registry)
                .context("failed to persist expired websocket admission cleanup during startup")?;
        }
        let restored_pending_websocket_admissions = registry.pending_websocket_admissions();
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
        let registry = Arc::new(registry);
        let peers = Arc::new(PeerHub::new());
        let metrics = Arc::new(MetricsState::default());
        let room_state_persistence = Arc::new(Mutex::new(room_state_persistence));
        let room_state_persistence_lock = Arc::new(Mutex::new(()));
        let pending_websocket_admissions = Arc::new(PendingWebSocketAdmissions::default());
        let server_media_lifecycle = Arc::new(ServerMediaLifecycle::default());
        let server_media_runtime_pump = Arc::new(ServerMediaRuntimePump::with_lifecycle(
            Arc::clone(&media_runtime),
            Arc::clone(&server_media_negotiator),
            Arc::clone(&server_media_lifecycle),
        ));
        let state = Self {
            registry,
            media_runtime,
            media_egress,
            processed_audio_webrtc_egress_pump,
            raw_opus_webrtc_egress_pump,
            server_media_sessions,
            server_media_negotiator,
            server_media_runtime_pump,
            server_media_lifecycle,
            media_relays,
            peers,
            metrics,
            ice_servers: Arc::new(ice_servers),
            turn_rest_credentials,
            room_state_persistence,
            room_state_persistence_lock,
            pending_websocket_admissions,
        };
        state.restore_pending_websocket_admissions(restored_pending_websocket_admissions)?;
        Ok(state)
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

    pub async fn stop_media_relay(
        &self,
        room_id: RoomId,
        request: lyre_core::StopMediaRelayRequest,
    ) -> lyre_core::MediaRelayRoomStatus {
        let _guard = self.server_media_lifecycle.lock(&room_id).await;
        self.stop_media_relay_locked(room_id, request).await
    }

    pub(crate) async fn stop_media_relay_locked(
        &self,
        room_id: RoomId,
        request: lyre_core::StopMediaRelayRequest,
    ) -> lyre_core::MediaRelayRoomStatus {
        let status = self.media_relays.stop(room_id.clone(), request);
        self.close_server_media_sessions_for_room_locked(&room_id)
            .await;
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
            self.raw_opus_webrtc_egress_pump
                .discard_room_packets(room_id);
            self.raw_opus_webrtc_egress_pump.start(room_id.clone());
        } else {
            self.raw_opus_webrtc_egress_pump.stop(room_id);
            self.raw_opus_webrtc_egress_pump
                .discard_room_packets(room_id);
            self.processed_audio_webrtc_egress_pump
                .start(room_id.clone());
        }
    }
}
