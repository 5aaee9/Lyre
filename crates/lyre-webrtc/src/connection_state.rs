use std::sync::{Arc, Mutex};

use webrtc::peer_connection::{RTCIceConnectionState, RTCPeerConnectionState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerMediaConnectionStateSnapshot {
    pub peer_connection_state: RTCPeerConnectionState,
    pub ice_connection_state: RTCIceConnectionState,
}

impl Default for ServerMediaConnectionStateSnapshot {
    fn default() -> Self {
        Self {
            peer_connection_state: RTCPeerConnectionState::New,
            ice_connection_state: RTCIceConnectionState::New,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ServerMediaConnectionState {
    snapshot: Arc<Mutex<ServerMediaConnectionStateSnapshot>>,
}

impl ServerMediaConnectionState {
    pub(crate) fn snapshot(&self) -> ServerMediaConnectionStateSnapshot {
        self.snapshot
            .lock()
            .expect("server media connection state lock must not be poisoned")
            .clone()
    }

    pub(crate) fn set_peer_connection_state(&self, state: RTCPeerConnectionState) {
        self.snapshot
            .lock()
            .expect("server media connection state lock must not be poisoned")
            .peer_connection_state = state;
    }

    pub(crate) fn set_ice_connection_state(&self, state: RTCIceConnectionState) {
        self.snapshot
            .lock()
            .expect("server media connection state lock must not be poisoned")
            .ice_connection_state = state;
    }
}
