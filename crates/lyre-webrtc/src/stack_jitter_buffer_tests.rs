use crate::{
    stack::WebRtcStack,
    stack_audio_ingress::CONCEALMENT_UNAVAILABLE_ERROR,
    test_support::{
        encoded_opus_payload_for_test, opus_rtp_packet_for_test,
        server_media_offer_with_valid_opus_sender,
    },
    ServerMediaAnswer, ServerMediaIceCandidate, WebRtcPeerConnectionHandle,
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
async fn answer_remote_offer_decodes_out_of_order_rtp_in_sequence_order() {
    let (server, track) = connected_opus_server().await;
    let payload = encoded_opus_payload_for_test();
    let mut decoded = Vec::new();

    write_opus_packets(
        &track,
        [
            (10, 9_600, payload.clone()),
            (12, 11_520, payload.clone()),
            (11, 10_560, payload),
        ],
    )
    .await;

    for _ in 0..100 {
        decoded.extend(server.drain_pcm_frames());
        let sequences = decoded
            .iter()
            .map(|frame| frame.sequence_number)
            .collect::<Vec<_>>();
        if sequences == vec![10, 11, 12] {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("server did not decode out-of-order RTP in sequence order");
}

#[tokio::test]
async fn answer_remote_offer_drops_duplicate_rtp_packets() {
    let (server, track) = connected_opus_server().await;
    let payload = encoded_opus_payload_for_test();
    let mut decoded = Vec::new();

    write_opus_packets(
        &track,
        [
            (20, 19_200, payload.clone()),
            (20, 19_200, payload.clone()),
            (21, 20_160, payload),
        ],
    )
    .await;

    for _ in 0..100 {
        decoded.extend(server.drain_pcm_frames());
        let sequences = decoded
            .iter()
            .map(|frame| frame.sequence_number)
            .collect::<Vec<_>>();
        if sequences == vec![20, 21] {
            tokio::time::sleep(Duration::from_millis(80)).await;
            decoded.extend(server.drain_pcm_frames());
            assert_eq!(
                decoded
                    .iter()
                    .map(|frame| frame.sequence_number)
                    .collect::<Vec<_>>(),
                vec![20, 21]
            );
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("server did not drop duplicate RTP packet");
}

#[tokio::test]
async fn answer_remote_offer_records_loss_when_gap_exceeds_jitter_depth() {
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

    for _ in 0..100 {
        let failures = server.drain_decode_failures();
        if failures.iter().any(|failure| {
            failure.sequence_number == 31
                && failure.rtp_timestamp == 29_760
                && failure.error == CONCEALMENT_UNAVAILABLE_ERROR
        }) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("server did not record missing RTP packet as concealment-unavailable failure");
}

#[tokio::test]
async fn answer_remote_offer_records_multiple_loss_failures_with_incrementing_timestamps() {
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

    let mut failures = Vec::new();
    for _ in 0..100 {
        failures.extend(server.drain_decode_failures());
        let loss_events = failures
            .iter()
            .filter(|failure| failure.error == CONCEALMENT_UNAVAILABLE_ERROR)
            .map(|failure| (failure.sequence_number, failure.rtp_timestamp))
            .collect::<Vec<_>>();
        if loss_events == vec![(41, 40_960), (42, 41_920)] {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("server did not record multiple missing RTP packets with deterministic timestamps");
}

#[tokio::test]
async fn answer_remote_offer_records_wrapped_loss_timestamp() {
    let (server, track) = connected_opus_server().await;
    let payload = encoded_opus_payload_for_test();

    write_opus_packets(
        &track,
        [
            (100, u32::MAX - 479),
            (102, 0),
            (103, 0),
            (104, 0),
            (105, 960),
        ]
        .map(|(sequence, timestamp)| (sequence, timestamp, payload.clone())),
    )
    .await;

    for _ in 0..100 {
        let failures = server.drain_decode_failures();
        if failures.iter().any(|failure| {
            failure.sequence_number == 101
                && failure.rtp_timestamp == 480
                && failure.error == CONCEALMENT_UNAVAILABLE_ERROR
        }) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("server did not record wrapped missing RTP timestamp");
}
