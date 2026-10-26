use crate::{
    ServerMediaAnswer, ServerMediaEgressError, ServerMediaEgressRtpPacket, ServerMediaIceCandidate,
    ServerMediaIceCandidateInit, ServerMediaProcessedAudioFrame, ServerMediaSessionState,
    WebRtcPeerConnectionHandle, WebRtcStack, SERVER_MEDIA_OPUS_FRAME_SIZE,
};
use lyre_core::{RoomId, UserId};

async fn wait_for_local_candidates(
    server: &WebRtcPeerConnectionHandle,
) -> Vec<ServerMediaIceCandidateInit> {
    for _ in 0..128 {
        let candidates = server.local_ice_candidates();
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
    server.local_ice_candidates()
}

#[tokio::test]
async fn two_negotiated_sources_reach_remote_peer_with_distinct_ssrcs() {
    let first_source = UserId::from_external("source-a");
    let second_source = UserId::from_external("source-b");
    let server = WebRtcStack::new()
        .create_peer_connection_for_sources(&[first_source.clone(), second_source.clone()])
        .await
        .unwrap();
    let offer =
        crate::test_support::server_media_offer_with_valid_opus_sender_and_remote_source_count(2)
            .await;
    let answer_sdp = server
        .answer_remote_offer(offer.offer_sdp.clone())
        .await
        .unwrap();
    assert_eq!(answer_sdp.matches("\nm=audio ").count(), 2);
    assert!(answer_sdp.contains("a=msid:lyre-user:source-a:audio lyre-user:source-a:audio"));
    assert!(answer_sdp.contains("a=msid:lyre-user:source-b:audio lyre-user:source-b:audio"));
    let answer = ServerMediaAnswer {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("listener"),
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
            wait_for_local_candidates(&server)
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
    crate::test_support::wait_for_server_connected(&server).await;

    for (source, sequence_number, timestamp) in
        [(&first_source, 10, 9_600), (&second_source, 11, 10_560)]
    {
        server
            .send_opus_rtp_packet(
                source,
                ServerMediaEgressRtpPacket {
                    sequence_number,
                    timestamp,
                    payload_type: 111,
                    payload: vec![0x01, 0x02, 0x03],
                },
            )
            .await
            .unwrap();
    }

    for _ in 0..100 {
        let packets = connected.received_remote_rtp_packets();
        if packets.len() == 2 {
            let sequence_numbers = packets
                .iter()
                .map(|packet| packet.header.sequence_number)
                .collect::<std::collections::BTreeSet<_>>();
            let ssrcs = packets
                .iter()
                .map(|packet| packet.header.ssrc)
                .collect::<std::collections::BTreeSet<_>>();
            // Each source egress stream owns its sequence space, starting at 0.
            assert_eq!(sequence_numbers, [0].into_iter().collect());
            assert_eq!(ssrcs.len(), 2);
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("remote peer did not receive both negotiated sources");
}

#[tokio::test]
async fn processed_audio_frame_reaches_remote_peer_without_retaining_packet_history() {
    let source_user_id = UserId::from_external("source");
    let server = WebRtcStack::new()
        .create_peer_connection_for_sources(std::slice::from_ref(&source_user_id))
        .await
        .unwrap();
    let offer = crate::test_support::server_media_offer_with_valid_opus_sender().await;
    let answer_sdp = server
        .answer_remote_offer(offer.offer_sdp.clone())
        .await
        .unwrap();
    assert_eq!(answer_sdp.matches("\nm=audio ").count(), 1);
    let answer = ServerMediaAnswer {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
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
            wait_for_local_candidates(&server)
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
    crate::test_support::wait_for_server_connected(&server).await;

    let sent = server
        .send_processed_audio_frame(
            &source_user_id,
            ServerMediaProcessedAudioFrame {
                sequence: 7,
                rtp_timestamp: None,
                sample_rate_hz: 48_000,
                channels: 1,
                samples: vec![0.1; SERVER_MEDIA_OPUS_FRAME_SIZE],
            },
        )
        .await
        .unwrap();

    assert_eq!(sent, 1);
    for _ in 0..100 {
        let packets = connected.received_remote_rtp_packets();
        if let Some(packet) = packets.first() {
            assert_eq!(packet.header.sequence_number, 0);
            assert_eq!(packet.header.timestamp, 0);
            assert_eq!(packet.header.payload_type, 111);
            assert!(!packet.payload.is_empty());
            assert!(server.sent_egress_rtp_packets_for_test().is_empty());
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("remote peer did not receive processed audio RTP");
}

#[tokio::test]
async fn processed_audio_egress_rejects_invalid_pcm_shape() {
    let source_user_id = UserId::from_external("source");
    let server = WebRtcStack::new()
        .create_peer_connection_for_sources(std::slice::from_ref(&source_user_id))
        .await
        .unwrap();

    let error = server
        .send_processed_audio_frame(
            &source_user_id,
            ServerMediaProcessedAudioFrame {
                sequence: 7,
                rtp_timestamp: None,
                sample_rate_hz: 44_100,
                channels: 1,
                samples: vec![0.1; SERVER_MEDIA_OPUS_FRAME_SIZE],
            },
        )
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        ServerMediaEgressError::InvalidSampleRate {
            sample_rate_hz: 44_100
        }
    ));
    assert!(server.sent_egress_rtp_packets_for_test().is_empty());
}

#[tokio::test]
async fn processed_audio_egress_source_not_negotiated_returns_typed_error() {
    let negotiated_source = UserId::from_external("negotiated");
    let unnegotiated_source = UserId::from_external("unnegotiated");
    let server = WebRtcStack::new()
        .create_peer_connection_for_sources(std::slice::from_ref(&negotiated_source))
        .await
        .unwrap();

    let error = server
        .send_processed_audio_frame(
            &unnegotiated_source,
            ServerMediaProcessedAudioFrame {
                sequence: 7,
                rtp_timestamp: None,
                sample_rate_hz: 48_000,
                channels: 1,
                samples: vec![0.1; SERVER_MEDIA_OPUS_FRAME_SIZE],
            },
        )
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        ServerMediaEgressError::SourceNotNegotiated { source_user_id }
            if source_user_id == unnegotiated_source
    ));
    assert!(server.sent_egress_rtp_packets_for_test().is_empty());
}
