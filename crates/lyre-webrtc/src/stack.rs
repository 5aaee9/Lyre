use std::{
    error::Error,
    sync::{Arc, Mutex},
};

use crate::{
    media_ingress::MediaIngressRecorder, ServerMediaDecodeError, ServerMediaDecodeFailure,
    ServerMediaOpusDecoder, ServerMediaPcmFrame, ServerMediaRemoteTrack, ServerMediaRtpPacket,
    ServerMediaTrackKind,
};
use rtc::{
    peer_connection::configuration::{
        interceptor_registry::register_default_interceptors,
        media_engine::{MediaEngine, MIME_TYPE_OPUS},
    },
    rtp_transceiver::rtp_sender::{RTCRtpCodec, RTCRtpCodecParameters, RtpCodecKind},
};
use thiserror::Error;
use tracing::warn;
use webrtc::{
    media_stream::track_remote::{TrackRemote, TrackRemoteEvent},
    peer_connection::{
        PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceCandidateInit,
        RTCIceGatheringState, RTCPeerConnectionIceEvent, RTCSessionDescription, Registry,
    },
    rtp_transceiver::{RTCRtpTransceiverDirection, RTCRtpTransceiverInit},
};

#[derive(Debug, Default, Clone)]
pub struct WebRtcStack;

impl WebRtcStack {
    pub fn new() -> Self {
        Self
    }

    pub async fn create_peer_connection(
        &self,
    ) -> Result<WebRtcPeerConnectionHandle, WebRtcStackError> {
        let local_ice_candidates = Arc::new(Mutex::new(Vec::new()));
        let media_ingress = MediaIngressRecorder::default();
        let handler = Arc::new(PeerConnectionHandler {
            local_ice_candidates: Arc::clone(&local_ice_candidates),
            media_ingress: media_ingress.clone(),
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
            .with_udp_addrs(vec!["127.0.0.1:0".to_owned()])
            .build()
            .await
            .map_err(|source| WebRtcStackError::CreatePeerConnection {
                source: Box::new(source),
            })?;
        peer_connection
            .add_transceiver_from_kind(
                RtpCodecKind::Audio,
                Some(RTCRtpTransceiverInit {
                    direction: RTCRtpTransceiverDirection::Recvonly,
                    ..Default::default()
                }),
            )
            .await
            .map_err(|source| WebRtcStackError::CreatePeerConnection {
                source: Box::new(source),
            })?;

        Ok(WebRtcPeerConnectionHandle {
            _peer_connection: Arc::from(peer_connection),
            local_ice_candidates,
            media_ingress,
        })
    }
}

#[derive(Clone)]
struct PeerConnectionHandler {
    local_ice_candidates: Arc<Mutex<Vec<ServerMediaIceCandidateInit>>>,
    media_ingress: MediaIngressRecorder,
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
        tokio::spawn(async move {
            let mut decoder = match ServerMediaOpusDecoder::new() {
                Ok(decoder) => decoder,
                Err(error) => {
                    warn!(error = %error, "failed to initialize server media Opus decoder");
                    return;
                }
            };
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
                    media_ingress.record_rtp_packet(packet.clone());
                    match decoder.decode_packet(&packet) {
                        Ok(frame) => media_ingress.record_pcm_frame(frame),
                        Err(error) => {
                            let message = match &error {
                                ServerMediaDecodeError::InvalidDecoderConfig { message }
                                | ServerMediaDecodeError::Decode { message } => message.clone(),
                            };
                            warn!(error = %error, "failed to decode server media Opus RTP packet");
                            media_ingress.record_decode_failure(ServerMediaDecodeFailure {
                                track_id: packet.track_id,
                                sequence_number: packet.sequence_number,
                                rtp_timestamp: packet.timestamp,
                                error: message,
                            });
                        }
                    }
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
    media_ingress: MediaIngressRecorder,
}

impl std::fmt::Debug for WebRtcPeerConnectionHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WebRtcPeerConnectionHandle")
            .finish_non_exhaustive()
    }
}

impl WebRtcPeerConnectionHandle {
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
            .clone()
    }

    pub fn remote_tracks(&self) -> Vec<ServerMediaRemoteTrack> {
        self.media_ingress.remote_tracks()
    }

    pub fn received_rtp_packets(&self) -> Vec<ServerMediaRtpPacket> {
        self.media_ingress.received_rtp_packets()
    }

    pub fn drain_pcm_frames(&self) -> Vec<ServerMediaPcmFrame> {
        self.media_ingress.drain_pcm_frames()
    }

    pub fn drain_decode_failures(&self) -> Vec<ServerMediaDecodeFailure> {
        self.media_ingress.drain_decode_failures()
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
