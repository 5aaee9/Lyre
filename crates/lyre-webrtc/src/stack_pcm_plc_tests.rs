use crate::{
    stack::WebRtcStack,
    stack_audio_ingress::CONCEALMENT_UNAVAILABLE_ERROR,
    test_support::{
        encoded_opus_payload_for_test, opus_rtp_packet_for_test,
        server_media_offer_with_valid_opus_sender,
    },
    ServerMediaAnswer, ServerMediaIceCandidate, WebRtcPeerConnectionHandle,
    SERVER_MEDIA_OPUS_FRAME_SIZE,
};
use std::{sync::Arc, time::Duration};
use webrtc::media_stream::track_local::{static_rtp::TrackLocalStaticRTP, TrackLocal};

async fn connected_opus_server() -> (WebRtcPeerConnectionHandle, Arc<TrackLocalStaticRTP>) {
    let server = WebRtcStack::new().create_peer_connection().await.unwrap();
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
    let connected = offer
        .accept_answer(
            &answer,
            server
                .local_ice_candidates()
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

    (server, connected.track())
}

async fn write_opus_packets(
    track: &Arc<TrackLocalStaticRTP>,
    packets: impl IntoIterator<Item = (u16, u32, Vec<u8>)>,
) {
    for (sequence, timestamp, payload) in packets {
        let _ = track
            .write_rtp(opus_rtp_packet_for_test(sequence, timestamp, payload))
            .await;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn missing_packet_after_decoded_baseline_produces_synthetic_pcm_frame() {
    let (server, track) = connected_opus_server().await;
    let payload = encoded_opus_payload_for_test();

    write_opus_packets(
        &track,
        [
            (30, 28_800),
            (32, 30_720),
            (33, 31_680),
            (34, 32_640),
            (35, 33_600),
        ]
        .map(|(sequence, timestamp)| (sequence, timestamp, payload.clone())),
    )
    .await;

    let mut frames = Vec::new();
    for _ in 0..100 {
        frames.extend(server.drain_pcm_frames());
        if let Some(frame) = frames.iter().find(|frame| frame.sequence_number == 31) {
            assert_eq!(frame.rtp_timestamp, 29_760);
            assert_eq!(frame.sample_rate_hz, 48_000);
            assert_eq!(frame.channels, 1);
            assert_eq!(frame.samples.len(), SERVER_MEDIA_OPUS_FRAME_SIZE);
            assert!(frame.samples.iter().any(|sample| sample.abs() > 0.0));
            assert!(server
                .drain_decode_failures()
                .iter()
                .all(|failure| failure.sequence_number != 31));
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("server did not synthesize PCM for missing RTP packet");
}

#[tokio::test]
async fn multiple_missing_packets_produce_multiple_synthetic_pcm_frames() {
    let (server, track) = connected_opus_server().await;
    let payload = encoded_opus_payload_for_test();

    write_opus_packets(
        &track,
        [
            (40, 40_000),
            (43, 42_880),
            (44, 43_840),
            (45, 44_800),
            (46, 45_760),
        ]
        .map(|(sequence, timestamp)| (sequence, timestamp, payload.clone())),
    )
    .await;

    let mut frames = Vec::new();
    for _ in 0..100 {
        frames.extend(server.drain_pcm_frames());
        let loss_frames = frames
            .iter()
            .filter(|frame| matches!(frame.sequence_number, 41 | 42))
            .map(|frame| {
                (
                    frame.sequence_number,
                    frame.rtp_timestamp,
                    frame.samples.len(),
                )
            })
            .collect::<Vec<_>>();
        if loss_frames
            == vec![
                (41, 40_960, SERVER_MEDIA_OPUS_FRAME_SIZE),
                (42, 41_920, SERVER_MEDIA_OPUS_FRAME_SIZE),
            ]
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("server did not synthesize PCM for multiple missing RTP packets");
}

#[tokio::test]
async fn missing_packet_without_decoded_baseline_records_decode_failure() {
    let (server, track) = connected_opus_server().await;
    let payload = encoded_opus_payload_for_test();

    write_opus_packets(
        &track,
        [
            (30, 28_800, Vec::new()),
            (32, 30_720, payload.clone()),
            (33, 31_680, payload.clone()),
            (34, 32_640, payload.clone()),
            (35, 33_600, payload),
        ],
    )
    .await;

    let mut failures = Vec::new();
    for _ in 0..100 {
        failures.extend(server.drain_decode_failures());
        let saw_malformed_baseline = failures
            .iter()
            .any(|failure| failure.sequence_number == 30 && failure.error == "Input packet empty");
        let saw_loss_without_baseline = failures.iter().any(|failure| {
            failure.sequence_number == 31
                && failure.rtp_timestamp == 29_760
                && failure.error == CONCEALMENT_UNAVAILABLE_ERROR
        });
        if saw_malformed_baseline && saw_loss_without_baseline {
            assert!(server
                .drain_pcm_frames()
                .iter()
                .all(|frame| frame.sequence_number != 31));
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("server did not keep decode failure when no PLC baseline exists");
}

#[tokio::test]
async fn malformed_real_packet_does_not_seed_plc_state() {
    let (server, track) = connected_opus_server().await;
    let payload = encoded_opus_payload_for_test();

    write_opus_packets(
        &track,
        [
            (50, 48_000, Vec::new()),
            (52, 49_920, payload.clone()),
            (53, 50_880, payload.clone()),
            (54, 51_840, payload.clone()),
            (55, 52_800, payload),
        ],
    )
    .await;

    let mut failures = Vec::new();
    let mut frames = Vec::new();
    for _ in 0..100 {
        failures.extend(server.drain_decode_failures());
        frames.extend(server.drain_pcm_frames());
        let saw_malformed = failures
            .iter()
            .any(|failure| failure.sequence_number == 50 && failure.error == "Input packet empty");
        let saw_loss_without_baseline = failures.iter().any(|failure| {
            failure.sequence_number == 51
                && failure.rtp_timestamp == 48_960
                && failure.error == CONCEALMENT_UNAVAILABLE_ERROR
        });
        let decoded_after_loss = frames.iter().any(|frame| frame.sequence_number == 52);
        let no_synthetic_loss = frames.iter().all(|frame| frame.sequence_number != 51);
        if saw_malformed && saw_loss_without_baseline && decoded_after_loss && no_synthetic_loss {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("malformed real packet incorrectly seeded PLC state");
}
