use std::{
    error::Error,
    sync::{Arc, Mutex},
};

use thiserror::Error;
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceCandidateInit,
    RTCIceGatheringState, RTCPeerConnectionIceEvent, RTCSessionDescription,
};

#[derive(Debug, Default, Clone)]
pub struct WebRtcStack;

impl WebRtcStack {
    pub fn new() -> Self {
        Self
    }

    pub async fn create_peer_connection(
        &self,
    ) -> Result<WebRtcPeerConnectionHandle, WebRtcStackError> {
        let local_ice_candidates = Arc::new(Mutex::new(Vec::new()));
        let handler = Arc::new(PeerConnectionHandler {
            local_ice_candidates: Arc::clone(&local_ice_candidates),
        });
        let peer_connection = PeerConnectionBuilder::new()
            .with_handler(handler)
            .with_udp_addrs(vec!["127.0.0.1:0".to_owned()])
            .build()
            .await
            .map_err(|source| WebRtcStackError::CreatePeerConnection {
                source: Box::new(source),
            })?;

        Ok(WebRtcPeerConnectionHandle {
            _peer_connection: Arc::from(peer_connection),
            local_ice_candidates,
        })
    }
}

#[derive(Clone)]
struct PeerConnectionHandler {
    local_ice_candidates: Arc<Mutex<Vec<ServerMediaIceCandidateInit>>>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for PeerConnectionHandler {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        let candidate = match event.candidate.to_json() {
            Ok(candidate) => candidate,
            Err(_) => return,
        };
        self.local_ice_candidates
            .lock()
            .expect("local ICE candidate collection lock must not be poisoned")
            .push(ServerMediaIceCandidateInit::from(candidate));
    }

    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            self.local_ice_candidates
                .lock()
                .expect("local ICE candidate collection lock must not be poisoned")
                .push(ServerMediaIceCandidateInit {
                    candidate: String::new(),
                    sdp_mid: None,
                    sdp_mline_index: None,
                    username_fragment: None,
                });
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ServerMediaIceCandidateInit {
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_mline_index: Option<u16>,
    pub username_fragment: Option<String>,
}

impl From<RTCIceCandidateInit> for ServerMediaIceCandidateInit {
    fn from(candidate: RTCIceCandidateInit) -> Self {
        Self {
            candidate: candidate.candidate,
            sdp_mid: candidate.sdp_mid,
            sdp_mline_index: candidate.sdp_mline_index,
            username_fragment: candidate.username_fragment,
        }
    }
}

impl From<ServerMediaIceCandidateInit> for RTCIceCandidateInit {
    fn from(candidate: ServerMediaIceCandidateInit) -> Self {
        Self {
            candidate: candidate.candidate,
            sdp_mid: candidate.sdp_mid,
            sdp_mline_index: candidate.sdp_mline_index,
            username_fragment: candidate.username_fragment,
            url: None,
        }
    }
}

#[derive(Clone)]
pub struct WebRtcPeerConnectionHandle {
    _peer_connection: Arc<dyn PeerConnection>,
    local_ice_candidates: Arc<Mutex<Vec<ServerMediaIceCandidateInit>>>,
}

impl std::fmt::Debug for WebRtcPeerConnectionHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WebRtcPeerConnectionHandle")
            .finish_non_exhaustive()
    }
}

impl WebRtcPeerConnectionHandle {
    pub async fn add_remote_ice_candidate(
        &self,
        candidate: ServerMediaIceCandidateInit,
    ) -> Result<(), WebRtcStackError> {
        self._peer_connection
            .add_ice_candidate(candidate.into())
            .await
            .map_err(|source| WebRtcStackError::AddIceCandidate {
                source: Box::new(source),
            })
    }

    pub fn local_ice_candidates(&self) -> Vec<ServerMediaIceCandidateInit> {
        self.local_ice_candidates
            .lock()
            .expect("local ICE candidate collection lock must not be poisoned")
            .clone()
    }

    pub async fn answer_remote_offer(&self, offer_sdp: String) -> Result<String, WebRtcStackError> {
        let offer = RTCSessionDescription::offer(offer_sdp).map_err(|source| {
            WebRtcStackError::CreateAnswer {
                source: Box::new(source),
            }
        })?;
        self._peer_connection
            .set_remote_description(offer)
            .await
            .map_err(|source| WebRtcStackError::CreateAnswer {
                source: Box::new(source),
            })?;
        let answer = self
            ._peer_connection
            .create_answer(None)
            .await
            .map_err(|source| WebRtcStackError::CreateAnswer {
                source: Box::new(source),
            })?;
        self._peer_connection
            .set_local_description(answer)
            .await
            .map_err(|source| WebRtcStackError::CreateAnswer {
                source: Box::new(source),
            })?;
        let local_description = self.local_description_with_ice().await?;
        Ok(local_description.sdp)
    }

    pub async fn create_local_offer_for_test(&self) -> Result<String, WebRtcStackError> {
        self._peer_connection
            .create_data_channel("lyre-test", None)
            .await
            .map_err(|source| WebRtcStackError::CreateOffer {
                source: Box::new(source),
            })?;
        let offer = self
            ._peer_connection
            .create_offer(None)
            .await
            .map_err(|source| WebRtcStackError::CreateOffer {
                source: Box::new(source),
            })?;
        self._peer_connection
            .set_local_description(offer)
            .await
            .map_err(|source| WebRtcStackError::CreateOffer {
                source: Box::new(source),
            })?;
        let local_description = self.local_description_with_ice().await?;
        Ok(local_description.sdp)
    }

    async fn local_description_with_ice(&self) -> Result<RTCSessionDescription, WebRtcStackError> {
        for _ in 0..64 {
            if let Some(local_description) = self._peer_connection.local_description().await {
                if local_description.sdp.contains("a=ice-ufrag:") {
                    return Ok(local_description);
                }
            }
            tokio::task::yield_now().await;
        }
        self._peer_connection
            .local_description()
            .await
            .ok_or(WebRtcStackError::MissingLocalDescription)
    }

    #[cfg(test)]
    pub(crate) fn debug_id(&self) -> usize {
        Arc::as_ptr(&self._peer_connection) as *const () as usize
    }
}

#[derive(Debug, Error)]
pub enum WebRtcStackError {
    #[error("failed to create WebRTC peer connection")]
    CreatePeerConnection {
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    #[error("failed to create WebRTC offer")]
    CreateOffer {
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    #[error("failed to create WebRTC answer")]
    CreateAnswer {
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    #[error("failed to add WebRTC ICE candidate")]
    AddIceCandidate {
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    #[error("WebRTC peer connection did not produce a local description")]
    MissingLocalDescription,
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn create_peer_connection_returns_lyre_handle() {
        let handle = super::WebRtcStack::new()
            .create_peer_connection()
            .await
            .unwrap();

        assert_eq!(
            std::any::type_name_of_val(&handle),
            "lyre_webrtc::stack::WebRtcPeerConnectionHandle"
        );
    }

    async fn offer_sdp() -> String {
        let offerer = super::WebRtcStack::new()
            .create_peer_connection()
            .await
            .unwrap();
        offerer.create_local_offer_for_test().await.unwrap()
    }

    fn host_candidate() -> super::ServerMediaIceCandidateInit {
        super::ServerMediaIceCandidateInit {
            candidate: "candidate:1 1 UDP 2130706431 192.168.1.100 54321 typ host".to_owned(),
            sdp_mid: Some("0".to_owned()),
            sdp_mline_index: Some(0),
            username_fragment: None,
        }
    }

    async fn wait_for_local_candidates(
        handle: &super::WebRtcPeerConnectionHandle,
    ) -> Vec<super::ServerMediaIceCandidateInit> {
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
        let answerer = super::WebRtcStack::new()
            .create_peer_connection()
            .await
            .unwrap();

        let answer = answerer
            .answer_remote_offer(offer_sdp().await)
            .await
            .unwrap();

        assert!(answer.starts_with("v=0"));
    }

    #[tokio::test]
    async fn invalid_remote_offer_preserves_source_error() {
        let answerer = super::WebRtcStack::new()
            .create_peer_connection()
            .await
            .unwrap();

        let error = answerer
            .answer_remote_offer("not sdp".to_owned())
            .await
            .unwrap_err();

        assert!(std::error::Error::source(&error).is_some());
    }

    #[tokio::test]
    async fn add_remote_ice_candidate_accepts_candidate_after_answer() {
        let answerer = super::WebRtcStack::new()
            .create_peer_connection()
            .await
            .unwrap();
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
        let answerer = super::WebRtcStack::new()
            .create_peer_connection()
            .await
            .unwrap();
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
        let answerer = super::WebRtcStack::new()
            .create_peer_connection()
            .await
            .unwrap();
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
}
