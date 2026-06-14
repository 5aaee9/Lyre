use std::{error::Error, sync::Arc};

use thiserror::Error;
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};

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

pub struct WebRtcPeerConnectionHandle {
    _peer_connection: Arc<dyn PeerConnection>,
}

#[derive(Debug, Error)]
pub enum WebRtcStackError {
    #[error("failed to create WebRTC peer connection")]
    CreatePeerConnection {
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
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
}
