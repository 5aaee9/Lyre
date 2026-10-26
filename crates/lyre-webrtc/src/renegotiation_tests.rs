use std::{sync::Arc, time::Duration};

use lyre_core::{RoomId, UserId};

use crate::{
    test_support::{server_media_offer_with_valid_opus_sender, ServerMediaConnectedOffer},
    ServerMediaEgressRtpPacket, ServerMediaIceCandidate, ServerMediaNegotiator, ServerMediaOffer,
    ServerMediaSessionKey, ServerMediaSessionRegistry, WebRtcStack,
};

fn key() -> ServerMediaSessionKey {
    ServerMediaSessionKey {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("listener"),
    }
}

fn offer(sdp: String) -> ServerMediaOffer {
    ServerMediaOffer {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("listener"),
        audio_track_id: "audio-main".to_owned(),
        sdp,
    }
}

async fn wait_for_local_candidates(
    negotiator: &ServerMediaNegotiator,
    key: &ServerMediaSessionKey,
) -> Vec<ServerMediaIceCandidate> {
    for _ in 0..128 {
        let candidates = negotiator.local_ice_candidates(key);
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
    negotiator.local_ice_candidates(key)
}

async fn connected_listener(
    negotiator: &ServerMediaNegotiator,
    sources: Vec<UserId>,
) -> ServerMediaConnectedOffer {
    let test_offer = server_media_offer_with_valid_opus_sender().await;
    let answer = negotiator
        .answer_offer_for_sources(offer(test_offer.offer_sdp.clone()), sources)
        .await
        .unwrap();
    for candidate in test_offer.remote_candidates().await {
        negotiator
            .add_remote_ice_candidate(ServerMediaIceCandidate {
                room_id: RoomId::default_room(),
                user_id: UserId::from_external("listener"),
                candidate: candidate.candidate,
                sdp_mid: candidate.sdp_mid,
                sdp_mline_index: candidate.sdp_mline_index,
                username_fragment: candidate.username_fragment,
            })
            .await
            .unwrap();
    }
    let candidates = wait_for_local_candidates(negotiator, &key()).await;
    test_offer.accept_answer(&answer, candidates).await
}

async fn send_until_received(
    negotiator: &ServerMediaNegotiator,
    listener: &ServerMediaConnectedOffer,
    source: &UserId,
) -> u32 {
    let already_received = listener.received_remote_rtp_packets().len();
    for _ in 0..100 {
        negotiator
            .send_opus_rtp_packet(
                &key(),
                source,
                ServerMediaEgressRtpPacket {
                    sequence_number: 1,
                    timestamp: 960,
                    payload_type: 111,
                    payload: vec![0x01, 0x02, 0x03],
                },
            )
            .await
            .unwrap();
        if let Some(packet) = listener.received_remote_rtp_packets().get(already_received) {
            return packet.header.ssrc;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("listener did not receive audio from source {source}");
}

#[tokio::test]
async fn offer_from_same_peer_adds_and_removes_sources_without_new_peer_connection() {
    let negotiator = ServerMediaNegotiator::new(
        WebRtcStack::new(),
        Arc::new(ServerMediaSessionRegistry::new()),
    );
    let first = UserId::from_external("first");
    let second = UserId::from_external("second");
    let listener = connected_listener(&negotiator, vec![first.clone()]).await;
    let peer_before = negotiator.stored_peer_connection_debug_id(&key()).unwrap();
    let first_ssrc = send_until_received(&negotiator, &listener, &first).await;

    let answer = negotiator
        .answer_offer_for_sources(
            offer(listener.renegotiation_offer(1).await),
            vec![first.clone(), second.clone()],
        )
        .await
        .unwrap();
    listener.apply_answer(answer.sdp.clone()).await;

    assert!(answer.sdp.contains("lyre-user:second:audio"));
    assert_eq!(
        negotiator.stored_peer_connection_debug_id(&key()),
        Some(peer_before)
    );
    assert_eq!(
        send_until_received(&negotiator, &listener, &first).await,
        first_ssrc
    );
    let second_ssrc = send_until_received(&negotiator, &listener, &second).await;
    assert_ne!(second_ssrc, first_ssrc);

    let answer = negotiator
        .answer_offer_for_sources(
            offer(listener.renegotiation_offer(0).await),
            vec![second.clone()],
        )
        .await
        .unwrap();
    listener.apply_answer(answer.sdp.clone()).await;

    assert!(!answer.sdp.contains("lyre-user:first:audio"));
    assert_eq!(
        negotiator.stored_peer_connection_debug_id(&key()),
        Some(peer_before)
    );
    assert!(negotiator
        .send_opus_rtp_packet(
            &key(),
            &first,
            ServerMediaEgressRtpPacket {
                sequence_number: 1,
                timestamp: 960,
                payload_type: 111,
                payload: vec![0x01],
            },
        )
        .await
        .is_err());
    assert_eq!(
        send_until_received(&negotiator, &listener, &second).await,
        second_ssrc
    );
}

#[tokio::test]
async fn offer_from_new_browser_peer_replaces_server_peer() {
    let negotiator = ServerMediaNegotiator::new(
        WebRtcStack::new(),
        Arc::new(ServerMediaSessionRegistry::new()),
    );
    let source = UserId::from_external("source");
    let _listener = connected_listener(&negotiator, vec![source.clone()]).await;
    let peer_before = negotiator.stored_peer_connection_debug_id(&key()).unwrap();

    let fresh_offer = server_media_offer_with_valid_opus_sender().await;
    negotiator
        .answer_offer_for_sources(offer(fresh_offer.offer_sdp.clone()), vec![source])
        .await
        .unwrap();

    assert_ne!(
        negotiator.stored_peer_connection_debug_id(&key()),
        Some(peer_before)
    );
}
