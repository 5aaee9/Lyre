use crate::{
    stack::ServerMediaIceCandidateInit, ServerMediaAnswer, ServerMediaIceCandidate,
    WebRtcPeerConnectionHandle, SERVER_MEDIA_OPUS_FRAME_SIZE,
};
use bytes::Bytes;
use opus_rs::{Application, OpusEncoder};
use rtc::{
    peer_connection::configuration::{
        interceptor_registry::register_default_interceptors,
        media_engine::{MediaEngine, MIME_TYPE_OPUS},
    },
    rtp_transceiver::rtp_sender::{
        RTCRtpCodec, RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind,
    },
};
use std::sync::{Arc, Mutex};
use webrtc::{
    media_stream::{
        track_local::{static_rtp::TrackLocalStaticRTP, TrackLocal},
        track_remote::{TrackRemote, TrackRemoteEvent},
        MediaStreamTrack,
    },
    peer_connection::{
        PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceCandidateInit,
        RTCIceGatheringState, RTCPeerConnectionIceEvent, RTCPeerConnectionState, Registry,
    },
    runtime::{channel, timeout, Receiver, Sender},
};

#[derive(Clone)]
struct TestPeerConnectionHandler {
    local_ice_candidates: Arc<Mutex<Vec<RTCIceCandidateInit>>>,
    gather_complete: Sender<()>,
    connected: Sender<()>,
    remote_rtp_packets: Arc<Mutex<Vec<rtc::rtp::Packet>>>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for TestPeerConnectionHandler {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        let candidate = match event.candidate.to_json() {
            Ok(candidate) => candidate,
            Err(_) => return,
        };
        self.local_ice_candidates
            .lock()
            .expect("test ICE candidate collection lock must not be poisoned")
            .push(candidate);
    }

    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        if state == RTCPeerConnectionState::Connected {
            let _ = self.connected.try_send(());
        }
    }

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        let remote_rtp_packets = Arc::clone(&self.remote_rtp_packets);
        tokio::spawn(async move {
            while let Some(event) = track.poll().await {
                if let TrackRemoteEvent::OnRtpPacket(packet) = event {
                    remote_rtp_packets
                        .lock()
                        .expect("test remote RTP packet collection lock must not be poisoned")
                        .push(packet);
                }
            }
        });
    }
}

pub async fn server_media_offer_with_valid_opus_sender() -> ServerMediaTestOffer {
    let (
        offerer,
        track,
        offerer_candidates,
        remote_rtp_packets,
        mut gather_complete_rx,
        connected_rx,
    ) = opus_offerer().await;

    let offer = offerer.create_offer(None).await.unwrap();
    offerer.set_local_description(offer).await.unwrap();
    let offer_sdp = local_description_sdp_after_gathering(&offerer, &mut gather_complete_rx).await;

    ServerMediaTestOffer {
        offer_sdp,
        offerer,
        track,
        offerer_candidates,
        remote_rtp_packets,
        connected_rx,
    }
}

pub struct ServerMediaTestOffer {
    pub offer_sdp: String,
    offerer: Arc<dyn PeerConnection>,
    track: Arc<TrackLocalStaticRTP>,
    offerer_candidates: Arc<Mutex<Vec<RTCIceCandidateInit>>>,
    remote_rtp_packets: Arc<Mutex<Vec<rtc::rtp::Packet>>>,
    connected_rx: Receiver<()>,
}

impl ServerMediaTestOffer {
    pub async fn accept_answer_and_send_valid_opus(
        self,
        answer: &ServerMediaAnswer,
        server_candidates: Vec<ServerMediaIceCandidate>,
    ) {
        let connected = self.accept_answer(answer, server_candidates).await;
        connected.send_valid_opus_packets(100).await;
    }

    pub async fn accept_answer(
        mut self,
        answer: &ServerMediaAnswer,
        server_candidates: Vec<ServerMediaIceCandidate>,
    ) -> ServerMediaConnectedOffer {
        let answer =
            webrtc::peer_connection::RTCSessionDescription::answer(answer.sdp.clone()).unwrap();
        self.offerer.set_remote_description(answer).await.unwrap();

        for candidate in server_candidates {
            if candidate.candidate.is_empty() {
                continue;
            }
            self.offerer
                .add_ice_candidate(to_webrtc_candidate(ServerMediaIceCandidateInit {
                    candidate: candidate.candidate,
                    sdp_mid: candidate.sdp_mid,
                    sdp_mline_index: candidate.sdp_mline_index,
                    username_fragment: candidate.username_fragment,
                }))
                .await
                .unwrap();
        }

        let _ = wait_for_connected(&mut self.connected_rx).await;
        ServerMediaConnectedOffer {
            _offerer: self.offerer,
            track: self.track,
            remote_rtp_packets: self.remote_rtp_packets,
        }
    }

    pub async fn remote_candidates(&self) -> Vec<ServerMediaIceCandidateInit> {
        wait_for_test_candidates(&self.offerer_candidates)
            .await
            .into_iter()
            .map(to_server_candidate)
            .collect()
    }
}

pub struct ServerMediaConnectedOffer {
    _offerer: Arc<dyn PeerConnection>,
    track: Arc<TrackLocalStaticRTP>,
    remote_rtp_packets: Arc<Mutex<Vec<rtc::rtp::Packet>>>,
}

impl ServerMediaConnectedOffer {
    pub fn track(&self) -> Arc<TrackLocalStaticRTP> {
        Arc::clone(&self.track)
    }

    pub async fn send_valid_opus_packets(&self, count: usize) {
        let payload = encoded_opus_payload();
        for _ in 0..count {
            let _ = self.track.write_rtp(test_rtp_packet(payload.clone())).await;
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }

    pub fn received_remote_rtp_packets(&self) -> Vec<rtc::rtp::Packet> {
        self.remote_rtp_packets
            .lock()
            .expect("test remote RTP packet collection lock must not be poisoned")
            .clone()
    }
}

pub async fn send_valid_opus_packet_to_server(server: &crate::WebRtcPeerConnectionHandle) {
    let offer = server_media_offer_with_valid_opus_sender().await;
    let answer_sdp = server
        .answer_remote_offer(offer.offer_sdp.clone())
        .await
        .unwrap();
    let answer = ServerMediaAnswer {
        room_id: lyre_core::RoomId::default_room(),
        user_id: lyre_core::UserId::from_external("user_01"),
        audio_track_id: "audio-main".to_owned(),
        sdp: answer_sdp,
        state: crate::ServerMediaSessionState::Negotiating,
    };
    for candidate in offer.remote_candidates().await {
        server.add_remote_ice_candidate(candidate).await.unwrap();
    }
    offer
        .accept_answer_and_send_valid_opus(
            &answer,
            wait_for_local_candidates(server)
                .await
                .into_iter()
                .map(|candidate| ServerMediaIceCandidate {
                    room_id: answer.room_id.clone(),
                    user_id: answer.user_id.clone(),
                    candidate: candidate.candidate,
                    sdp_mid: candidate.sdp_mid,
                    sdp_mline_index: candidate.sdp_mline_index,
                    username_fragment: candidate.username_fragment,
                })
                .collect(),
        )
        .await;
}

type OpusOffererParts = (
    Arc<dyn PeerConnection>,
    Arc<TrackLocalStaticRTP>,
    Arc<Mutex<Vec<RTCIceCandidateInit>>>,
    Arc<Mutex<Vec<rtc::rtp::Packet>>>,
    Receiver<()>,
    Receiver<()>,
);

async fn opus_offerer() -> OpusOffererParts {
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .expect("default codecs should register for tests");
    let registry = register_default_interceptors(Registry::new(), &mut media_engine).unwrap();
    let local_ice_candidates = Arc::new(Mutex::new(Vec::new()));
    let remote_rtp_packets = Arc::new(Mutex::new(Vec::new()));
    let (gather_complete, gather_complete_rx) = channel(1);
    let (connected, connected_rx) = channel(1);
    let offerer = PeerConnectionBuilder::new()
        .with_handler(Arc::new(TestPeerConnectionHandler {
            local_ice_candidates: Arc::clone(&local_ice_candidates),
            gather_complete,
            connected,
            remote_rtp_packets: Arc::clone(&remote_rtp_packets),
        }))
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_udp_addrs(vec!["127.0.0.1:0".to_owned()])
        .build()
        .await
        .expect("test offerer peer connection should build");
    let codec = RTCRtpCodec {
        mime_type: MIME_TYPE_OPUS.to_owned(),
        clock_rate: 48_000,
        channels: 2,
        sdp_fmtp_line: String::new(),
        rtcp_feedback: vec![],
    };
    let track = Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
        "lyre-test".to_owned(),
        "audio".to_owned(),
        "audio".to_owned(),
        RtpCodecKind::Audio,
        vec![RTCRtpEncodingParameters {
            rtp_coding_parameters: RTCRtpCodingParameters {
                ssrc: Some(1234),
                ..Default::default()
            },
            codec,
            ..Default::default()
        }],
    )));
    offerer.add_track(track.clone()).await.unwrap();
    (
        Arc::from(offerer),
        track,
        local_ice_candidates,
        remote_rtp_packets,
        gather_complete_rx,
        connected_rx,
    )
}

async fn local_description_sdp_after_gathering(
    peer_connection: &Arc<dyn PeerConnection>,
    gather_complete_rx: &mut Receiver<()>,
) -> String {
    let _ = timeout(std::time::Duration::from_secs(5), gather_complete_rx.recv()).await;
    peer_connection.local_description().await.unwrap().sdp
}

async fn wait_for_connected(connected_rx: &mut Receiver<()>) -> bool {
    timeout(std::time::Duration::from_secs(5), connected_rx.recv())
        .await
        .ok()
        .flatten()
        .is_some()
}

async fn wait_for_test_candidates(
    candidates: &Arc<Mutex<Vec<RTCIceCandidateInit>>>,
) -> Vec<RTCIceCandidateInit> {
    for _ in 0..128 {
        let snapshot = candidates
            .lock()
            .expect("test ICE candidate collection lock must not be poisoned")
            .clone();
        if snapshot
            .iter()
            .any(|candidate| candidate.candidate.starts_with("candidate:"))
        {
            return snapshot;
        }
        tokio::task::yield_now().await;
    }
    candidates
        .lock()
        .expect("test ICE candidate collection lock must not be poisoned")
        .clone()
}

async fn wait_for_local_candidates(
    handle: &WebRtcPeerConnectionHandle,
) -> Vec<ServerMediaIceCandidateInit> {
    for _ in 0..128 {
        let candidates = handle.local_ice_candidates();
        if candidates
            .iter()
            .any(|candidate| candidate.candidate.starts_with("candidate:"))
            && candidates
                .iter()
                .any(|candidate| candidate.candidate.is_empty())
        {
            return candidates;
        }
        tokio::task::yield_now().await;
    }
    handle.local_ice_candidates()
}

fn to_server_candidate(candidate: RTCIceCandidateInit) -> ServerMediaIceCandidateInit {
    ServerMediaIceCandidateInit {
        candidate: candidate.candidate,
        sdp_mid: candidate.sdp_mid,
        sdp_mline_index: candidate.sdp_mline_index,
        username_fragment: candidate.username_fragment,
    }
}

fn to_webrtc_candidate(candidate: ServerMediaIceCandidateInit) -> RTCIceCandidateInit {
    RTCIceCandidateInit {
        candidate: candidate.candidate,
        sdp_mid: candidate.sdp_mid,
        sdp_mline_index: candidate.sdp_mline_index,
        username_fragment: candidate.username_fragment,
        url: None,
    }
}

pub fn encoded_opus_payload_for_test() -> Vec<u8> {
    let mut encoder = OpusEncoder::new(48_000, 1, Application::Voip).unwrap();
    let samples = vec![0.1; SERVER_MEDIA_OPUS_FRAME_SIZE];
    let mut payload = vec![0_u8; 512];
    let payload_len = encoder
        .encode(&samples, SERVER_MEDIA_OPUS_FRAME_SIZE, &mut payload)
        .unwrap();
    payload.truncate(payload_len);
    payload
}

fn encoded_opus_payload() -> Vec<u8> {
    encoded_opus_payload_for_test()
}

pub fn opus_rtp_packet_for_test(
    sequence_number: u16,
    timestamp: u32,
    payload: Vec<u8>,
) -> rtc::rtp::Packet {
    rtc::rtp::Packet {
        header: rtc::rtp::Header {
            version: 2,
            sequence_number,
            timestamp,
            marker: true,
            payload_type: 111,
            ssrc: 1234,
            ..Default::default()
        },
        payload: Bytes::from(payload),
    }
}

fn test_rtp_packet(payload: Vec<u8>) -> rtc::rtp::Packet {
    opus_rtp_packet_for_test(42, 1234, payload)
}
