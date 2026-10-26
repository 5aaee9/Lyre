use std::time::Duration;

use lyre_core::{RoomId, UserId};

use crate::{
    test_support::server_media_offer_with_opus_payload_type, ServerMediaAnswer,
    ServerMediaEgressRtpPacket, ServerMediaIceCandidate, ServerMediaSessionState,
    WebRtcPeerConnectionHandle, WebRtcStack,
};

async fn wait_for_local_candidates(
    server: &WebRtcPeerConnectionHandle,
    room_id: &RoomId,
    user_id: &UserId,
) -> Vec<ServerMediaIceCandidate> {
    for _ in 0..128 {
        let candidates = server.local_ice_candidates();
        if candidates
            .iter()
            .any(|candidate| candidate.candidate.starts_with("candidate:"))
            && candidates
                .iter()
                .any(|candidate| candidate.candidate.is_empty())
        {
            return candidates
                .into_iter()
                .map(|candidate| ServerMediaIceCandidate {
                    room_id: room_id.clone(),
                    user_id: user_id.clone(),
                    candidate: candidate.candidate,
                    sdp_mid: candidate.sdp_mid,
                    sdp_mline_index: candidate.sdp_mline_index,
                    username_fragment: candidate.username_fragment,
                })
                .collect();
        }
        tokio::task::yield_now().await;
    }

    panic!("server did not gather local ICE candidates");
}

#[tokio::test]
async fn negotiated_dynamic_opus_payload_type_is_decoded() {
    let server = WebRtcStack::new().create_peer_connection().await.unwrap();
    let offer = server_media_offer_with_opus_payload_type(109).await;
    assert!(offer.offer_sdp.contains("a=rtpmap:109 opus/48000/2"));

    let answer_sdp = server
        .answer_remote_offer(offer.offer_sdp.clone())
        .await
        .unwrap();
    assert!(answer_sdp.contains("a=rtpmap:109 opus/48000/2"));

    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("dynamic-opus-user");
    let answer = ServerMediaAnswer {
        room_id: room_id.clone(),
        user_id: user_id.clone(),
        audio_track_id: "audio-main".to_owned(),
        sdp: answer_sdp,
        state: ServerMediaSessionState::Negotiating,
    };

    for candidate in offer.remote_candidates().await {
        server.add_remote_ice_candidate(candidate).await.unwrap();
    }
    let server_candidates = wait_for_local_candidates(&server, &room_id, &user_id).await;
    let connected = offer.accept_answer(&answer, server_candidates).await;
    connected.send_valid_opus_packets(1).await;

    for _ in 0..100 {
        let packets = server.received_rtp_packets();
        let frames = server.drain_pcm_frames();
        if packets
            .iter()
            .any(|packet| packet.payload_type == 109 && packet.sequence_number == 42)
            && frames
                .iter()
                .any(|frame| frame.sequence_number == 42 && !frame.samples.is_empty())
        {
            assert!(server.drain_decode_failures().is_empty());
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!(
        "server did not decode negotiated Opus payload type 109; failures: {:?}",
        server.drain_decode_failures()
    );
}

#[tokio::test]
async fn server_egress_uses_the_negotiated_dynamic_opus_payload_type() {
    let source_user_id = UserId::from_external("source");
    let server = WebRtcStack::new()
        .create_peer_connection_for_sources(std::slice::from_ref(&source_user_id))
        .await
        .unwrap();
    let offer = server_media_offer_with_opus_payload_type(109).await;
    let answer_sdp = server
        .answer_remote_offer(offer.offer_sdp.clone())
        .await
        .unwrap();

    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("listener");
    let answer = ServerMediaAnswer {
        room_id: room_id.clone(),
        user_id: user_id.clone(),
        audio_track_id: "audio-main".to_owned(),
        sdp: answer_sdp,
        state: ServerMediaSessionState::Negotiating,
    };
    for candidate in offer.remote_candidates().await {
        server.add_remote_ice_candidate(candidate).await.unwrap();
    }
    let connected = offer
        .accept_answer(
            &answer,
            wait_for_local_candidates(&server, &room_id, &user_id).await,
        )
        .await;

    for _ in 0..100 {
        server
            .send_opus_rtp_packet(
                &source_user_id,
                ServerMediaEgressRtpPacket {
                    sequence_number: 7,
                    timestamp: 9_600,
                    payload_type: 111,
                    payload: vec![0x01, 0x02, 0x03],
                },
            )
            .await
            .unwrap();
        if let Some(packet) = connected.received_remote_rtp_packets().first() {
            assert_eq!(packet.header.payload_type, 109);
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("remote peer did not receive server RTP with negotiated payload type 109");
}

#[tokio::test]
async fn raw_egress_keeps_recipient_sequence_contiguous_when_source_restarts() {
    let source_user_id = UserId::from_external("source");
    let server = WebRtcStack::new()
        .create_peer_connection_for_sources(std::slice::from_ref(&source_user_id))
        .await
        .unwrap();
    let offer = server_media_offer_with_opus_payload_type(111).await;
    let answer_sdp = server
        .answer_remote_offer(offer.offer_sdp.clone())
        .await
        .unwrap();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("listener");
    let answer = ServerMediaAnswer {
        room_id: room_id.clone(),
        user_id: user_id.clone(),
        audio_track_id: "audio-main".to_owned(),
        sdp: answer_sdp,
        state: ServerMediaSessionState::Negotiating,
    };
    for candidate in offer.remote_candidates().await {
        server.add_remote_ice_candidate(candidate).await.unwrap();
    }
    let connected = offer
        .accept_answer(
            &answer,
            wait_for_local_candidates(&server, &room_id, &user_id).await,
        )
        .await;
    crate::test_support::wait_for_server_connected(&server).await;

    // The source reconnects between the two halves: its sequence numbers and
    // timestamps restart far below the previous ones.
    let source_packets = (0..5)
        .map(|index| (40_000 + index, 3_000_000 + u32::from(index) * 960))
        .chain((0..5).map(|index| (100 + index, 50 + u32::from(index) * 960)));
    for (sequence_number, timestamp) in source_packets {
        server
            .send_opus_rtp_packet(
                &source_user_id,
                ServerMediaEgressRtpPacket {
                    sequence_number,
                    timestamp,
                    payload_type: 111,
                    payload: vec![0x01, 0x02, 0x03],
                },
            )
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    for _ in 0..100 {
        let packets = connected.received_remote_rtp_packets();
        if packets.len() == 10 {
            let sequences = packets
                .iter()
                .map(|packet| packet.header.sequence_number)
                .collect::<Vec<_>>();
            assert_eq!(sequences, (0..10).collect::<Vec<_>>());
            assert!(packets.windows(2).all(|pair| {
                pair[1]
                    .header
                    .timestamp
                    .wrapping_sub(pair[0].header.timestamp)
                    < 48_000
            }));
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!(
        "listener received {} of 10 packets across a source restart",
        connected.received_remote_rtp_packets().len()
    );
}
