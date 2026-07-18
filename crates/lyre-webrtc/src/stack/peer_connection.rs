#[cfg(test)]
#[derive(Debug, Default)]
pub(super) struct PeerConnectionLifecycle {
    created: AtomicUsize,
    closed: AtomicUsize,
}

#[cfg(test)]
impl PeerConnectionLifecycle {
    pub(super) fn record_created(&self) {
        self.created.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn record_closed(&self) {
        self.closed.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn live_count(&self) -> usize {
        self.created.load(Ordering::Relaxed) - self.closed.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ServerMediaIceCandidateInit {
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_mline_index: Option<u16>,
    pub username_fragment: Option<String>,
}

impl ServerMediaIceCandidateInit {
    pub fn with_public_ip(&self, public_ip: Option<IpAddr>) -> Self {
        let Some(public_ip) = public_ip else {
            return self.clone();
        };
        let parts = self.candidate.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 8 || parts[7] != "host" {
            return self.clone();
        }
        Self {
            candidate: self.candidate.replacen(parts[4], &public_ip.to_string(), 1),
            sdp_mid: self.sdp_mid.clone(),
            sdp_mline_index: self.sdp_mline_index,
            username_fragment: self.username_fragment.clone(),
        }
    }
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
    pub(super) _peer_connection: Arc<dyn PeerConnection>,
    pub(super) local_ice_candidates: Arc<Mutex<Vec<ServerMediaIceCandidateInit>>>,
    pub(super) session_key: Arc<Mutex<Option<ServerMediaSessionKey>>>,
    pub(super) server_media_public_ip: Option<IpAddr>,
    pub(super) connection_state: ServerMediaConnectionState,
    pub(super) media_ingress: MediaIngressRecorder,
    pub(super) media_egress: ServerMediaEgress,
    pub(super) payload_dumper: PayloadDumper,
    #[cfg(test)]
    pub(super) peer_connection_lifecycle: Arc<PeerConnectionLifecycle>,
    #[cfg(test)]
    pub(super) peer_connection_closed: Arc<AtomicBool>,
}

impl std::fmt::Debug for WebRtcPeerConnectionHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WebRtcPeerConnectionHandle")
            .finish_non_exhaustive()
    }
}

impl WebRtcPeerConnectionHandle {
    pub fn set_session_key(&self, key: ServerMediaSessionKey) {
        self.payload_dumper.set_session_key(&key);
        *self
            .session_key
            .lock()
            .expect("server media session key lock must not be poisoned") = Some(key);
    }

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
            .iter()
            .map(|candidate| candidate.with_public_ip(self.server_media_public_ip))
            .collect()
    }

    pub fn remote_tracks(&self) -> Vec<ServerMediaRemoteTrack> {
        self.media_ingress.remote_tracks()
    }

    pub fn received_rtp_packets(&self) -> Vec<ServerMediaRtpPacket> {
        self.media_ingress.received_rtp_packets()
    }

    pub fn drain_rtp_packets(&self) -> Vec<ServerMediaRtpPacket> {
        self.media_ingress.drain_rtp_packets()
    }

    pub fn connection_state(&self) -> crate::ServerMediaConnectionStateSnapshot {
        self.connection_state.snapshot()
    }

    pub fn drain_pcm_frames(&self) -> Vec<ServerMediaPcmFrame> {
        self.media_ingress.drain_pcm_frames()
    }

    pub fn drain_decode_failures(&self) -> Vec<ServerMediaDecodeFailure> {
        self.media_ingress.drain_decode_failures()
    }

    pub async fn send_processed_audio_frame(
        &self,
        source_user_id: &lyre_core::UserId,
        frame: ServerMediaProcessedAudioFrame,
    ) -> Result<usize, ServerMediaEgressError> {
        self.media_egress
            .send_processed_audio_frame(source_user_id, frame)
            .await
    }

    pub async fn send_opus_rtp_packet(
        &self,
        source_user_id: &lyre_core::UserId,
        packet: crate::ServerMediaEgressRtpPacket,
    ) -> Result<usize, ServerMediaEgressError> {
        self.media_egress
            .send_opus_rtp_packet(source_user_id, packet)
            .await
    }

    pub async fn close(&self) -> Result<(), WebRtcStackError> {
        self._peer_connection.close().await.map_err(|source| {
            WebRtcStackError::ClosePeerConnection {
                source: Box::new(source),
            }
        })?;
        #[cfg(test)]
        if !self.peer_connection_closed.swap(true, Ordering::Relaxed) {
            self.peer_connection_lifecycle.record_closed();
        }
        self.connection_state
            .set_peer_connection_state(RTCPeerConnectionState::Closed);
        Ok(())
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn sent_egress_rtp_packets_for_test(&self) -> Vec<crate::ServerMediaEgressRtpPacket> {
        self.media_egress.sent_packets_for_test()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn egress_track_ids_for_test(&self) -> Vec<String> {
        self.media_egress.track_ids_for_test()
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
    #[error("failed to close WebRTC peer connection")]
    ClosePeerConnection {
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
