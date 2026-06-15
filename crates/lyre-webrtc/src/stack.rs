use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket},
    sync::{Arc, Mutex},
};

use crate::{
    connection_state::ServerMediaConnectionState, egress::ServerMediaEgress,
    media_ingress::MediaIngressRecorder, payload_dump::PayloadDumper,
    stack_audio_ingress::handle_audio_rtp_packet, ServerMediaDecodeFailure, ServerMediaEgressError,
    ServerMediaJitterBuffer, ServerMediaOpusDecoder, ServerMediaPcmConcealer, ServerMediaPcmFrame,
    ServerMediaProcessedAudioFrame, ServerMediaRemoteTrack, ServerMediaRtpPacket,
    ServerMediaSessionKey, ServerMediaTrackKind,
};
use rtc::{
    peer_connection::configuration::{
        interceptor_registry::register_default_interceptors,
        media_engine::{MediaEngine, MIME_TYPE_OPUS},
    },
    rtp_transceiver::rtp_sender::{RTCRtpCodec, RTCRtpCodecParameters, RtpCodecKind},
};
use thiserror::Error;
use tracing::{info, warn};
use webrtc::{
    media_stream::track_local::TrackLocal,
    media_stream::track_remote::{TrackRemote, TrackRemoteEvent},
    peer_connection::{
        PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceCandidateInit,
        RTCIceGatheringState, RTCPeerConnectionIceEvent, RTCSessionDescription, Registry,
    },
};

#[derive(Debug, Default, Clone)]
pub struct WebRtcStack {
    server_media_public_ip: Option<IpAddr>,
    server_media_port_range: Option<ServerMediaPortRange>,
}

impl WebRtcStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_server_media_public_ip(server_media_public_ip: Option<IpAddr>) -> Self {
        Self {
            server_media_public_ip,
            server_media_port_range: None,
        }
    }

    pub fn with_server_media_config(
        server_media_public_ip: Option<IpAddr>,
        server_media_port_range: Option<ServerMediaPortRange>,
    ) -> Self {
        Self {
            server_media_public_ip,
            server_media_port_range,
        }
    }

    pub async fn create_peer_connection(
        &self,
    ) -> Result<WebRtcPeerConnectionHandle, WebRtcStackError> {
        let local_ice_candidates = Arc::new(Mutex::new(Vec::new()));
        let connection_state = ServerMediaConnectionState::default();
        let session_key = Arc::new(Mutex::new(None));
        let media_ingress = MediaIngressRecorder::default();
        let payload_dumper = PayloadDumper::from_env();
        let media_egress = ServerMediaEgress::new(payload_dumper.clone()).map_err(|source| {
            WebRtcStackError::CreatePeerConnection {
                source: Box::new(source),
            }
        })?;
        let handler = Arc::new(PeerConnectionHandler {
            local_ice_candidates: Arc::clone(&local_ice_candidates),
            session_key: Arc::clone(&session_key),
            connection_state: connection_state.clone(),
            media_ingress: media_ingress.clone(),
            payload_dumper: payload_dumper.clone(),
        });
        let mut media_engine = MediaEngine::default();
        media_engine
            .register_codec(
                RTCRtpCodecParameters {
                    rtp_codec: RTCRtpCodec {
                        mime_type: MIME_TYPE_OPUS.to_owned(),
                        clock_rate: 48_000,
                        channels: 2,
                        sdp_fmtp_line: String::new(),
                        rtcp_feedback: vec![],
                    },
                    payload_type: 111,
                },
                RtpCodecKind::Audio,
            )
            .map_err(|source| WebRtcStackError::CreatePeerConnection {
                source: Box::new(source),
            })?;
        let registry = register_default_interceptors(Registry::new(), &mut media_engine).map_err(
            |source| WebRtcStackError::CreatePeerConnection {
                source: Box::new(source),
            },
        )?;
        let peer_connection = PeerConnectionBuilder::new()
            .with_handler(handler)
            .with_media_engine(media_engine)
            .with_interceptor_registry(registry)
            .with_udp_addrs(server_media_udp_addrs(self.server_media_port_range))
            .build()
            .await
            .map_err(|source| WebRtcStackError::CreatePeerConnection {
                source: Box::new(source),
            })?;
        peer_connection
            .add_track(media_egress.track() as Arc<dyn TrackLocal>)
            .await
            .map_err(|source| WebRtcStackError::CreatePeerConnection {
                source: Box::new(source),
            })?;

        Ok(WebRtcPeerConnectionHandle {
            _peer_connection: Arc::from(peer_connection),
            local_ice_candidates,
            session_key,
            server_media_public_ip: self.server_media_public_ip,
            connection_state,
            media_ingress,
            media_egress,
            payload_dumper,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerMediaPortRange {
    pub start: u16,
    pub end: u16,
}

fn server_media_udp_addrs(port_range: Option<ServerMediaPortRange>) -> Vec<String> {
    let host = server_media_udp_host();
    vec![server_media_udp_addr(&host, port_range)]
}

fn server_media_udp_addr(host: &str, port_range: Option<ServerMediaPortRange>) -> String {
    let Some(range) = port_range else {
        return format!("{host}:0");
    };
    for port in range.start..=range.end {
        let addr = format!("{host}:{port}");
        if UdpSocket::bind(&addr).is_ok() {
            return addr;
        }
    }
    format!("{host}:{}", range.start)
}

fn server_media_udp_host() -> String {
    let socket = match UdpSocket::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0))) {
        Ok(socket) => socket,
        Err(_) => return "127.0.0.1".to_owned(),
    };
    if socket.connect("8.8.8.8:80").is_ok() {
        if let Ok(SocketAddr::V4(addr)) = socket.local_addr() {
            if !addr.ip().is_unspecified() {
                return addr.ip().to_string();
            }
        }
    }
    if socket.connect("2001:4860:4860::8888:80").is_ok() {
        if let Ok(SocketAddr::V6(addr)) = socket.local_addr() {
            if !addr.ip().is_unspecified() {
                return format!("[{}]", addr.ip());
            }
        }
    }
    IpAddr::V4(Ipv4Addr::LOCALHOST).to_string()
}

#[derive(Clone)]
struct PeerConnectionHandler {
    local_ice_candidates: Arc<Mutex<Vec<ServerMediaIceCandidateInit>>>,
    session_key: Arc<Mutex<Option<ServerMediaSessionKey>>>,
    connection_state: ServerMediaConnectionState,
    media_ingress: MediaIngressRecorder,
    payload_dumper: PayloadDumper,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for PeerConnectionHandler {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        let candidate = match event.candidate.to_json() {
            Ok(candidate) => candidate,
            Err(_) => return,
        };
        self.local_ice_candidates
            .lock()
            .expect("local ICE candidate collection lock must not be poisoned")
            .push(ServerMediaIceCandidateInit::from(candidate));
    }

    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            self.local_ice_candidates
                .lock()
                .expect("local ICE candidate collection lock must not be poisoned")
                .push(ServerMediaIceCandidateInit {
                    candidate: String::new(),
                    sdp_mid: None,
                    sdp_mline_index: None,
                    username_fragment: None,
                });
        }
    }

    async fn on_ice_connection_state_change(
        &self,
        state: webrtc::peer_connection::RTCIceConnectionState,
    ) {
        self.connection_state.set_ice_connection_state(state);
        if let Some(key) = self
            .session_key
            .lock()
            .expect("server media session key lock must not be poisoned")
            .clone()
        {
            info!(
                room_id = %key.room_id,
                user_id = %key.user_id,
                ice_connection_state = ?state,
                "server media ICE connection state changed"
            );
        } else {
            info!(
                ice_connection_state = ?state,
                "server media ICE connection state changed"
            );
        }
    }

    async fn on_connection_state_change(
        &self,
        state: webrtc::peer_connection::RTCPeerConnectionState,
    ) {
        self.connection_state.set_peer_connection_state(state);
        if let Some(key) = self
            .session_key
            .lock()
            .expect("server media session key lock must not be poisoned")
            .clone()
        {
            info!(
                room_id = %key.room_id,
                user_id = %key.user_id,
                peer_connection_state = ?state,
                "server media peer connection state changed"
            );
        } else {
            info!(
                peer_connection_state = ?state,
                "server media peer connection state changed"
            );
        }
    }

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        let track_id = track.track_id().await.to_string();
        let kind = match track.kind().await {
            RtpCodecKind::Audio => ServerMediaTrackKind::Audio,
            RtpCodecKind::Video => ServerMediaTrackKind::Video,
            _ => ServerMediaTrackKind::Unknown,
        };
        let mime_type = first_codec_mime_type(&track).await;
        self.media_ingress
            .record_remote_track(ServerMediaRemoteTrack {
                track_id: track_id.clone(),
                kind: kind.clone(),
                mime_type,
            });

        if kind != ServerMediaTrackKind::Audio {
            return;
        }

        let media_ingress = self.media_ingress.clone();
        let payload_dumper = self.payload_dumper.clone();
        tokio::spawn(async move {
            let mut decoder = match ServerMediaOpusDecoder::new() {
                Ok(decoder) => decoder,
                Err(error) => {
                    warn!(error = %error, "failed to initialize server media Opus decoder");
                    return;
                }
            };
            let mut jitter_buffer = ServerMediaJitterBuffer::default();
            let mut concealer = ServerMediaPcmConcealer::default();
            while let Some(event) = track.poll().await {
                if let TrackRemoteEvent::OnRtpPacket(packet) = event {
                    let packet = ServerMediaRtpPacket {
                        track_id: track_id.clone(),
                        sequence_number: packet.header.sequence_number,
                        timestamp: packet.header.timestamp,
                        marker: packet.header.marker,
                        payload_type: packet.header.payload_type,
                        payload: packet.payload.to_vec(),
                    };
                    payload_dumper.dump_inbound(&packet);
                    handle_audio_rtp_packet(
                        &media_ingress,
                        &mut decoder,
                        &mut jitter_buffer,
                        &mut concealer,
                        packet,
                    );
                }
            }
        });
    }
}

async fn first_codec_mime_type(track: &Arc<dyn TrackRemote>) -> Option<String> {
    for ssrc in track.ssrcs().await {
        if let Some(codec) = track.codec(ssrc).await {
            return Some(codec.mime_type);
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ServerMediaIceCandidateInit {
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_mline_index: Option<u16>,
    pub username_fragment: Option<String>,
}

impl ServerMediaIceCandidateInit {
    pub fn with_public_ip(&self, public_ip: Option<IpAddr>) -> Self {
        let Some(public_ip) = public_ip else {
            return self.clone();
        };
        let parts = self.candidate.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 8 || parts[7] != "host" {
            return self.clone();
        }
        Self {
            candidate: self.candidate.replacen(parts[4], &public_ip.to_string(), 1),
            sdp_mid: self.sdp_mid.clone(),
            sdp_mline_index: self.sdp_mline_index,
            username_fragment: self.username_fragment.clone(),
        }
    }
}

impl From<RTCIceCandidateInit> for ServerMediaIceCandidateInit {
    fn from(candidate: RTCIceCandidateInit) -> Self {
        Self {
            candidate: candidate.candidate,
            sdp_mid: candidate.sdp_mid,
            sdp_mline_index: candidate.sdp_mline_index,
            username_fragment: candidate.username_fragment,
        }
    }
}

impl From<ServerMediaIceCandidateInit> for RTCIceCandidateInit {
    fn from(candidate: ServerMediaIceCandidateInit) -> Self {
        Self {
            candidate: candidate.candidate,
            sdp_mid: candidate.sdp_mid,
            sdp_mline_index: candidate.sdp_mline_index,
            username_fragment: candidate.username_fragment,
            url: None,
        }
    }
}

#[derive(Clone)]
pub struct WebRtcPeerConnectionHandle {
    _peer_connection: Arc<dyn PeerConnection>,
    local_ice_candidates: Arc<Mutex<Vec<ServerMediaIceCandidateInit>>>,
    session_key: Arc<Mutex<Option<ServerMediaSessionKey>>>,
    server_media_public_ip: Option<IpAddr>,
    connection_state: ServerMediaConnectionState,
    media_ingress: MediaIngressRecorder,
    media_egress: ServerMediaEgress,
    payload_dumper: PayloadDumper,
}

impl std::fmt::Debug for WebRtcPeerConnectionHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WebRtcPeerConnectionHandle")
            .finish_non_exhaustive()
    }
}

impl WebRtcPeerConnectionHandle {
    pub fn set_session_key(&self, key: ServerMediaSessionKey) {
        self.payload_dumper.set_session_key(&key);
        *self
            .session_key
            .lock()
            .expect("server media session key lock must not be poisoned") = Some(key);
    }

    pub async fn add_remote_ice_candidate(
        &self,
        candidate: ServerMediaIceCandidateInit,
    ) -> Result<(), WebRtcStackError> {
        self._peer_connection
            .add_ice_candidate(candidate.into())
            .await
            .map_err(|source| WebRtcStackError::AddIceCandidate {
                source: Box::new(source),
            })
    }

    pub fn local_ice_candidates(&self) -> Vec<ServerMediaIceCandidateInit> {
        self.local_ice_candidates
            .lock()
            .expect("local ICE candidate collection lock must not be poisoned")
            .iter()
            .map(|candidate| candidate.with_public_ip(self.server_media_public_ip))
            .collect()
    }

    pub fn remote_tracks(&self) -> Vec<ServerMediaRemoteTrack> {
        self.media_ingress.remote_tracks()
    }

    pub fn received_rtp_packets(&self) -> Vec<ServerMediaRtpPacket> {
        self.media_ingress.received_rtp_packets()
    }

    pub fn connection_state(&self) -> crate::ServerMediaConnectionStateSnapshot {
        self.connection_state.snapshot()
    }

    pub fn drain_pcm_frames(&self) -> Vec<ServerMediaPcmFrame> {
        self.media_ingress.drain_pcm_frames()
    }

    pub fn drain_decode_failures(&self) -> Vec<ServerMediaDecodeFailure> {
        self.media_ingress.drain_decode_failures()
    }

    pub async fn send_processed_audio_frame(
        &self,
        frame: ServerMediaProcessedAudioFrame,
    ) -> Result<usize, ServerMediaEgressError> {
        self.media_egress.send_processed_audio_frame(frame).await
    }

    pub async fn send_opus_rtp_packet(
        &self,
        packet: crate::ServerMediaEgressRtpPacket,
    ) -> Result<usize, ServerMediaEgressError> {
        self.media_egress.send_opus_rtp_packet(packet).await
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn sent_egress_rtp_packets_for_test(&self) -> Vec<crate::ServerMediaEgressRtpPacket> {
        self.media_egress.sent_packets_for_test()
    }

    pub async fn answer_remote_offer(&self, offer_sdp: String) -> Result<String, WebRtcStackError> {
        let offer = RTCSessionDescription::offer(offer_sdp).map_err(|source| {
            WebRtcStackError::CreateAnswer {
                source: Box::new(source),
            }
        })?;
        self._peer_connection
            .set_remote_description(offer)
            .await
            .map_err(|source| WebRtcStackError::CreateAnswer {
                source: Box::new(source),
            })?;
        let answer = self
            ._peer_connection
            .create_answer(None)
            .await
            .map_err(|source| WebRtcStackError::CreateAnswer {
                source: Box::new(source),
            })?;
        self._peer_connection
            .set_local_description(answer)
            .await
            .map_err(|source| WebRtcStackError::CreateAnswer {
                source: Box::new(source),
            })?;
        let local_description = self.local_description_with_ice().await?;
        Ok(local_description.sdp)
    }

    pub async fn create_local_offer_for_test(&self) -> Result<String, WebRtcStackError> {
        self._peer_connection
            .create_data_channel("lyre-test", None)
            .await
            .map_err(|source| WebRtcStackError::CreateOffer {
                source: Box::new(source),
            })?;
        let offer = self
            ._peer_connection
            .create_offer(None)
            .await
            .map_err(|source| WebRtcStackError::CreateOffer {
                source: Box::new(source),
            })?;
        self._peer_connection
            .set_local_description(offer)
            .await
            .map_err(|source| WebRtcStackError::CreateOffer {
                source: Box::new(source),
            })?;
        let local_description = self.local_description_with_ice().await?;
        Ok(local_description.sdp)
    }

    async fn local_description_with_ice(&self) -> Result<RTCSessionDescription, WebRtcStackError> {
        for _ in 0..64 {
            if let Some(local_description) = self._peer_connection.local_description().await {
                if local_description.sdp.contains("a=ice-ufrag:") {
                    return Ok(local_description);
                }
            }
            tokio::task::yield_now().await;
        }
        self._peer_connection
            .local_description()
            .await
            .ok_or(WebRtcStackError::MissingLocalDescription)
    }

    #[cfg(test)]
    pub(crate) fn debug_id(&self) -> usize {
        Arc::as_ptr(&self._peer_connection) as *const () as usize
    }
}

#[derive(Debug, Error)]
pub enum WebRtcStackError {
    #[error("failed to create WebRTC peer connection")]
    CreatePeerConnection {
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    #[error("failed to create WebRTC offer")]
    CreateOffer {
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    #[error("failed to create WebRTC answer")]
    CreateAnswer {
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    #[error("failed to add WebRTC ICE candidate")]
    AddIceCandidate {
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    #[error("WebRTC peer connection did not produce a local description")]
    MissingLocalDescription,
}
