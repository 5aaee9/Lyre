use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket},
    sync::{Arc, Mutex},
};

#[cfg(test)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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
        RTCIceGatheringState, RTCPeerConnectionIceEvent, RTCPeerConnectionState,
        RTCSessionDescription, Registry,
    },
};

include!("stack/peer_connection.rs");

#[derive(Debug, Default, Clone)]
pub struct WebRtcStack {
    server_media_public_ip: Option<IpAddr>,
    server_media_port_range: Option<ServerMediaPortRange>,
    #[cfg(test)]
    peer_connection_lifecycle: Arc<PeerConnectionLifecycle>,
}

impl WebRtcStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_server_media_public_ip(server_media_public_ip: Option<IpAddr>) -> Self {
        Self {
            server_media_public_ip,
            server_media_port_range: None,
            #[cfg(test)]
            peer_connection_lifecycle: Arc::default(),
        }
    }

    pub fn with_server_media_config(
        server_media_public_ip: Option<IpAddr>,
        server_media_port_range: Option<ServerMediaPortRange>,
    ) -> Self {
        Self {
            server_media_public_ip,
            server_media_port_range,
            #[cfg(test)]
            peer_connection_lifecycle: Arc::default(),
        }
    }

    pub async fn create_peer_connection(
        &self,
    ) -> Result<WebRtcPeerConnectionHandle, WebRtcStackError> {
        self.create_peer_connection_for_sources(&[]).await
    }

    pub async fn create_peer_connection_for_sources(
        &self,
        source_user_ids: &[lyre_core::UserId],
    ) -> Result<WebRtcPeerConnectionHandle, WebRtcStackError> {
        let local_ice_candidates = Arc::new(Mutex::new(Vec::new()));
        let connection_state = ServerMediaConnectionState::default();
        let session_key = Arc::new(Mutex::new(None));
        let media_ingress = MediaIngressRecorder::default();
        let payload_dumper = PayloadDumper::from_env();
        let media_egress = ServerMediaEgress::new(source_user_ids, payload_dumper.clone())
            .map_err(|source| WebRtcStackError::CreatePeerConnection {
                source: Box::new(source),
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
        #[cfg(test)]
        self.peer_connection_lifecycle.record_created();
        #[cfg(test)]
        let peer_connection_closed = Arc::new(AtomicBool::new(false));
        let mut egress_senders = Vec::new();
        for track in media_egress.tracks() {
            match peer_connection
                .add_track(track as Arc<dyn TrackLocal>)
                .await
            {
                Ok(sender) => egress_senders.push(sender),
                Err(source) => {
                    let close_result = peer_connection.close().await;
                    #[cfg(test)]
                    if close_result.is_ok() && !peer_connection_closed.swap(true, Ordering::Relaxed)
                    {
                        self.peer_connection_lifecycle.record_closed();
                    }
                    if let Err(close_error) = close_result {
                        warn!(
                        error = format_args!("{close_error:#}"),
                        "failed to close peer connection after egress track registration failed"
                    );
                    }
                    return Err(WebRtcStackError::CreatePeerConnection {
                        source: Box::new(source),
                    });
                }
            }
        }
        media_egress.attach_senders(egress_senders);

        Ok(WebRtcPeerConnectionHandle {
            _peer_connection: Arc::from(peer_connection),
            local_ice_candidates,
            session_key,
            server_media_public_ip: self.server_media_public_ip,
            connection_state,
            media_ingress,
            media_egress,
            payload_dumper,
            #[cfg(test)]
            peer_connection_lifecycle: Arc::clone(&self.peer_connection_lifecycle),
            #[cfg(test)]
            peer_connection_closed,
        })
    }

    #[cfg(test)]
    pub(crate) fn live_peer_connection_count_for_test(&self) -> usize {
        self.peer_connection_lifecycle.live_count()
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
                    let negotiated_codec_is_opus = track
                        .codec(packet.header.ssrc)
                        .await
                        .is_some_and(|codec| codec.mime_type.eq_ignore_ascii_case(MIME_TYPE_OPUS));
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
                        negotiated_codec_is_opus,
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
