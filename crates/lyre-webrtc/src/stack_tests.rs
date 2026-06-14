use crate::stack::{ServerMediaIceCandidateInit, WebRtcPeerConnectionHandle, WebRtcStack};
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use rtc::{
    peer_connection::configuration::{
        interceptor_registry::register_default_interceptors,
        media_engine::{MediaEngine, MIME_TYPE_OPUS},
    },
    rtp_transceiver::rtp_sender::{
        RTCRtpCodec, RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind,
    },
};
use webrtc::{
    media_stream::{
        track_local::{static_rtp::TrackLocalStaticRTP, TrackLocal},
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
}

#[tokio::test]
async fn create_peer_connection_returns_lyre_handle() {
    let handle = WebRtcStack::new().create_peer_connection().await.unwrap();

    assert_eq!(
        std::any::type_name_of_val(&handle),
        "lyre_webrtc::stack::WebRtcPeerConnectionHandle"
    );
}

async fn offer_sdp() -> String {
    let offerer = WebRtcStack::new().create_peer_connection().await.unwrap();
    offerer.create_local_offer_for_test().await.unwrap()
}

async fn opus_offerer() -> (
    Arc<dyn PeerConnection>,
    Arc<TrackLocalStaticRTP>,
    Arc<Mutex<Vec<RTCIceCandidateInit>>>,
    Receiver<()>,
    Receiver<()>,
) {
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .expect("default codecs should register for tests");
    let registry = register_default_interceptors(Registry::new(), &mut media_engine)
        .expect("default interceptors should register for tests");
    let local_ice_candidates = Arc::new(Mutex::new(Vec::new()));
    let (gather_complete, gather_complete_rx) = channel(1);
    let (connected, connected_rx) = channel(1);
    let offerer = PeerConnectionBuilder::new()
        .with_handler(Arc::new(TestPeerConnectionHandler {
            local_ice_candidates: Arc::clone(&local_ice_candidates),
            gather_complete,
            connected,
        }))
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_udp_addrs(vec!["127.0.0.1:0".to_owned()])
        .build()
        .await
        .expect("test offerer peer connection should build");
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
            codec: RTCRtpCodec {
                mime_type: MIME_TYPE_OPUS.to_owned(),
                clock_rate: 48_000,
                channels: 2,
                sdp_fmtp_line: String::new(),
                rtcp_feedback: vec![],
            },
            ..Default::default()
        }],
    )));
    offerer
        .add_track(track.clone())
        .await
        .expect("test offerer should accept audio track");
    (
        Arc::from(offerer),
        track,
        local_ice_candidates,
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

fn host_candidate() -> ServerMediaIceCandidateInit {
    ServerMediaIceCandidateInit {
        candidate: "candidate:1 1 UDP 2130706431 192.168.1.100 54321 typ host".to_owned(),
        sdp_mid: Some("0".to_owned()),
        sdp_mline_index: Some(0),
        username_fragment: None,
    }
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

#[tokio::test]
async fn answer_remote_offer_returns_answer_sdp() {
    let answerer = WebRtcStack::new().create_peer_connection().await.unwrap();

    let answer = answerer
        .answer_remote_offer(offer_sdp().await)
        .await
        .unwrap();

    assert!(answer.starts_with("v=0"));
}

#[tokio::test]
async fn opus_offerer_helper_creates_media_offer() {
    let (offerer, _track, _candidates, mut gather_complete_rx, _connected_rx) =
        opus_offerer().await;

    let offer = offerer.create_offer(None).await.unwrap();
    offerer.set_local_description(offer).await.unwrap();

    let offer_sdp = local_description_sdp_after_gathering(&offerer, &mut gather_complete_rx).await;
    assert!(offer_sdp.contains("m=audio"));
    assert!(offer_sdp.contains("opus"));
    assert!(offer_sdp.contains("a=ice-ufrag:"));
}

#[tokio::test]
async fn answer_remote_offer_records_audio_track_and_rtp_packet() {
    use std::time::Duration;

    let server = WebRtcStack::new().create_peer_connection().await.unwrap();
    let (offerer, track, offerer_candidates, mut gather_complete_rx, mut connected_rx) =
        opus_offerer().await;

    let offer = offerer.create_offer(None).await.unwrap();
    offerer.set_local_description(offer).await.unwrap();
    let offer_sdp = local_description_sdp_after_gathering(&offerer, &mut gather_complete_rx).await;

    let answer_sdp = server.answer_remote_offer(offer_sdp).await.unwrap();
    let answer = webrtc::peer_connection::RTCSessionDescription::answer(answer_sdp).unwrap();
    offerer.set_remote_description(answer).await.unwrap();

    let remote_candidates = wait_for_test_candidates(&offerer_candidates).await;
    for candidate in remote_candidates {
        server
            .add_remote_ice_candidate(to_server_candidate(candidate))
            .await
            .unwrap();
    }
    let server_candidates = wait_for_local_candidates(&server).await;
    for candidate in server_candidates {
        if candidate.candidate.is_empty() {
            continue;
        }
        offerer
            .add_ice_candidate(to_webrtc_candidate(candidate))
            .await
            .unwrap();
    }

    assert!(wait_for_connected(&mut connected_rx).await);

    for _ in 0..100 {
        let _ = track
            .write_rtp(test_rtp_packet(vec![0x11, 0x22, 0x33, 0x44]))
            .await;
        let packets = server.received_rtp_packets();
        if packets.iter().any(|packet| {
            packet.sequence_number == 42
                && packet.timestamp == 1234
                && packet.marker
                && packet.payload_type == 111
                && packet.payload == vec![0x11, 0x22, 0x33, 0x44]
        }) {
            assert!(server
                .remote_tracks()
                .iter()
                .any(|track| { track.kind == crate::ServerMediaTrackKind::Audio }));
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("server did not record the sent RTP packet");
}

fn test_rtp_packet(payload: Vec<u8>) -> rtc::rtp::Packet {
    rtc::rtp::Packet {
        header: rtc::rtp::Header {
            version: 2,
            sequence_number: 42,
            timestamp: 1234,
            marker: true,
            payload_type: 111,
            ssrc: 1234,
            ..Default::default()
        },
        payload: Bytes::from(payload),
    }
}

#[tokio::test]
async fn invalid_remote_offer_preserves_source_error() {
    let answerer = WebRtcStack::new().create_peer_connection().await.unwrap();

    let error = answerer
        .answer_remote_offer("not sdp".to_owned())
        .await
        .unwrap_err();

    assert!(std::error::Error::source(&error).is_some());
}

#[tokio::test]
async fn add_remote_ice_candidate_accepts_candidate_after_answer() {
    let answerer = WebRtcStack::new().create_peer_connection().await.unwrap();
    answerer
        .answer_remote_offer(offer_sdp().await)
        .await
        .unwrap();

    answerer
        .add_remote_ice_candidate(host_candidate())
        .await
        .unwrap();
}

#[tokio::test]
async fn invalid_remote_ice_candidate_preserves_source_error() {
    let answerer = WebRtcStack::new().create_peer_connection().await.unwrap();
    answerer
        .answer_remote_offer(offer_sdp().await)
        .await
        .unwrap();
    let mut candidate = host_candidate();
    candidate.candidate = "not a candidate".to_owned();

    let error = answerer
        .add_remote_ice_candidate(candidate)
        .await
        .unwrap_err();

    assert!(std::error::Error::source(&error).is_some());
}

#[tokio::test]
async fn local_ice_candidates_are_lyre_owned_values() {
    let answerer = WebRtcStack::new().create_peer_connection().await.unwrap();
    answerer
        .answer_remote_offer(offer_sdp().await)
        .await
        .unwrap();

    let candidates = wait_for_local_candidates(&answerer).await;

    assert!(candidates
        .iter()
        .any(|candidate| candidate.candidate.starts_with("candidate:")));
    assert!(candidates
        .iter()
        .any(|candidate| candidate.candidate.is_empty()));
}
