use crate::{
    stack::WebRtcStack,
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
