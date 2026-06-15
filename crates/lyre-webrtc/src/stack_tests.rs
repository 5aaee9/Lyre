use crate::stack::{ServerMediaIceCandidateInit, WebRtcPeerConnectionHandle, WebRtcStack};

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

#[tokio::test]
async fn local_ice_candidates_are_not_loopback_only() {
    let answerer = WebRtcStack::new().create_peer_connection().await.unwrap();
    answerer
        .answer_remote_offer(offer_sdp().await)
        .await
        .unwrap();

    let candidates = wait_for_local_candidates(&answerer).await;
    let host_candidates = candidates
        .iter()
        .filter(|candidate| candidate.candidate.contains(" typ host"))
        .collect::<Vec<_>>();

    assert!(!host_candidates.is_empty());
    assert!(host_candidates.iter().any(|candidate| {
        !candidate.candidate.contains(" 127.0.0.1 ") && !candidate.candidate.contains(" 0.0.0.0 ")
    }));
}
