use lyre_webrtc::ServerMediaIceCandidate;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ServerMediaIceCandidateSummary {
    pub(crate) end_of_candidates: bool,
    pub(crate) candidate_type: Option<String>,
    pub(crate) address: Option<String>,
    pub(crate) port: Option<u16>,
    pub(crate) sdp_mid: Option<String>,
    pub(crate) sdp_mline_index: Option<u16>,
    pub(crate) has_username_fragment: bool,
}

impl ServerMediaIceCandidateSummary {
    pub(crate) fn from_candidate(candidate: &ServerMediaIceCandidate) -> Self {
        let parts = candidate.candidate.split_whitespace().collect::<Vec<_>>();
        let candidate_type = parts
            .windows(2)
            .find(|window| window[0] == "typ")
            .map(|window| window[1].to_owned());
        Self {
            end_of_candidates: candidate.candidate.is_empty(),
            candidate_type,
            address: parts.get(4).map(|address| (*address).to_owned()),
            port: parts.get(5).and_then(|port| port.parse::<u16>().ok()),
            sdp_mid: candidate.sdp_mid.clone(),
            sdp_mline_index: candidate.sdp_mline_index,
            has_username_fragment: candidate.username_fragment.is_some(),
        }
    }
}

pub(crate) fn summarize_candidates(
    candidates: &[ServerMediaIceCandidate],
) -> Vec<ServerMediaIceCandidateSummary> {
    candidates
        .iter()
        .map(ServerMediaIceCandidateSummary::from_candidate)
        .collect()
}

#[cfg(test)]
mod tests {
    use lyre_core::{RoomId, UserId};

    use super::*;

    fn candidate(candidate: &str) -> ServerMediaIceCandidate {
        ServerMediaIceCandidate {
            room_id: RoomId::default_room(),
            user_id: UserId::from_external("user_01"),
            candidate: candidate.to_owned(),
            sdp_mid: Some("0".to_owned()),
            sdp_mline_index: Some(0),
            username_fragment: Some("ufrag".to_owned()),
        }
    }

    #[test]
    fn summarizes_candidate_type_address_and_port() {
        let summary = ServerMediaIceCandidateSummary::from_candidate(&candidate(
            "candidate:1 1 UDP 2130706431 203.0.113.10 54321 typ host",
        ));

        assert_eq!(summary.candidate_type.as_deref(), Some("host"));
        assert_eq!(summary.address.as_deref(), Some("203.0.113.10"));
        assert_eq!(summary.port, Some(54321));
        assert_eq!(summary.sdp_mid.as_deref(), Some("0"));
        assert_eq!(summary.sdp_mline_index, Some(0));
        assert!(summary.has_username_fragment);
        assert!(!summary.end_of_candidates);
    }

    #[test]
    fn summarizes_end_of_candidates() {
        let summary = ServerMediaIceCandidateSummary::from_candidate(&candidate(""));

        assert!(summary.end_of_candidates);
        assert!(summary.candidate_type.is_none());
        assert!(summary.address.is_none());
        assert!(summary.port.is_none());
    }
}
