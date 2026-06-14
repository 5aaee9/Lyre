use std::{error::Error, sync::Arc};

use thiserror::Error;
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCSessionDescription,
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
        let peer_connection = PeerConnectionBuilder::new()
            .with_handler(Arc::new(NoopPeerConnectionHandler))
            .with_udp_addrs(vec!["127.0.0.1:0".to_owned()])
            .build()
            .await
            .map_err(|source| WebRtcStackError::CreatePeerConnection {
                source: Box::new(source),
            })?;

        Ok(WebRtcPeerConnectionHandle {
            _peer_connection: Arc::from(peer_connection),
        })
    }
}

#[derive(Clone)]
struct NoopPeerConnectionHandler;

impl PeerConnectionEventHandler for NoopPeerConnectionHandler {}

#[derive(Clone)]
pub struct WebRtcPeerConnectionHandle {
    _peer_connection: Arc<dyn PeerConnection>,
}

impl std::fmt::Debug for WebRtcPeerConnectionHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WebRtcPeerConnectionHandle")
            .finish_non_exhaustive()
    }
}

impl WebRtcPeerConnectionHandle {
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
}
