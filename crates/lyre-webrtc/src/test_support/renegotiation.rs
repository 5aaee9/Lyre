use super::ServerMediaConnectedOffer;
use rtc::rtp_transceiver::{
    rtp_sender::RtpCodecKind, RTCRtpTransceiverDirection, RTCRtpTransceiverInit,
};
use webrtc::peer_connection::RTCPeerConnectionState;

impl ServerMediaConnectedOffer {
    /// Creates a renegotiation offer on the connected peer, adding receive-only audio
    /// transceivers so the server can attach `extra_receivers` more source tracks.
    pub async fn renegotiation_offer(&self, extra_receivers: usize) -> String {
        for _ in 0..extra_receivers {
            self.offerer
                .add_transceiver_from_kind(
                    RtpCodecKind::Audio,
                    Some(RTCRtpTransceiverInit {
                        direction: RTCRtpTransceiverDirection::Recvonly,
                        ..Default::default()
                    }),
                )
                .await
                .unwrap();
        }
        let offer = self.offerer.create_offer(None).await.unwrap();
        self.offerer.set_local_description(offer).await.unwrap();
        self.offerer.local_description().await.unwrap().sdp
    }

    pub async fn apply_answer(&self, answer_sdp: String) {
        let answer = webrtc::peer_connection::RTCSessionDescription::answer(answer_sdp).unwrap();
        self.offerer.set_remote_description(answer).await.unwrap();
    }
}

/// The browser side can report `Connected` before the server finishes DTLS, and RTP
/// written in that window is dropped. Tests that count every packet wait for both sides.
pub async fn wait_for_server_connected(server: &crate::WebRtcPeerConnectionHandle) {
    for _ in 0..250 {
        if server.connection_state().peer_connection_state == RTCPeerConnectionState::Connected {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("server peer connection did not connect");
}
