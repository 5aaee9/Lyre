use crate::{
    stack::{ServerMediaIceCandidateInit, WebRtcPeerConnectionHandle, WebRtcStack},
    SERVER_MEDIA_OPUS_FRAME_SIZE,
};
use std::sync::{Arc, Mutex};

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

fn encoded_opus_payload() -> Vec<u8> {
    let mut encoder = OpusEncoder::new(48_000, 1, Application::Voip).unwrap();
    let samples = (0..SERVER_MEDIA_OPUS_FRAME_SIZE)
        .map(|index| ((index as f32) / 24.0).sin() * 0.1)
        .collect::<Vec<_>>();
    let mut payload = vec![0_u8; 512];
    let payload_len = encoder
        .encode(&samples, SERVER_MEDIA_OPUS_FRAME_SIZE, &mut payload)
        .unwrap();
    payload.truncate(payload_len);
    payload
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
async fn answer_remote_offer_records_audio_track_rtp_packet_and_pcm_frame() {
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

    for candidate in wait_for_test_candidates(&offerer_candidates).await {
        server
            .add_remote_ice_candidate(to_server_candidate(candidate))
            .await
            .unwrap();
    }
    for candidate in wait_for_local_candidates(&server).await {
        if candidate.candidate.is_empty() {
            continue;
        }
        offerer
            .add_ice_candidate(to_webrtc_candidate(candidate))
            .await
            .unwrap();
    }

    assert!(wait_for_connected(&mut connected_rx).await);

    let payload = encoded_opus_payload();
    for _ in 0..100 {
        let _ = track.write_rtp(test_rtp_packet(payload.clone())).await;
        let packets = server.received_rtp_packets();
        let frames = server.drain_pcm_frames();
        if packets.iter().any(|packet| {
            packet.sequence_number == 42
                && packet.timestamp == 1234
                && packet.marker
                && packet.payload_type == 111
                && packet.payload == payload
        }) && frames.iter().any(|frame| {
            frame.track_id == "audio"
                && frame.sequence_number == 42
                && frame.rtp_timestamp == 1234
                && frame.sample_rate_hz == 48_000
                && frame.channels == 1
                && frame.samples.len() == SERVER_MEDIA_OPUS_FRAME_SIZE
        }) {
            assert!(server
                .remote_tracks()
                .iter()
                .any(|track| { track.kind == crate::ServerMediaTrackKind::Audio }));
            assert!(server.drain_pcm_frames().is_empty());
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("server did not record the sent RTP packet and decoded PCM frame");
}

#[tokio::test]
async fn answer_remote_offer_records_decode_failure_for_malformed_audio_rtp() {
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

    for candidate in wait_for_test_candidates(&offerer_candidates).await {
        server
            .add_remote_ice_candidate(to_server_candidate(candidate))
            .await
            .unwrap();
    }
    for candidate in wait_for_local_candidates(&server).await {
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
        let _ = track.write_rtp(test_rtp_packet(Vec::new())).await;
        let failures = server.drain_decode_failures();
        if failures.iter().any(|failure| {
            failure.track_id == "audio"
                && failure.sequence_number == 42
                && failure.rtp_timestamp == 1234
                && failure.error == "Input packet empty"
        }) {
            assert!(server.drain_pcm_frames().is_empty());
            assert!(server.drain_decode_failures().is_empty());
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("server did not record the malformed Opus RTP decode failure");
}
