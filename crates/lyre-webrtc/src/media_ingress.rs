use crate::{ServerMediaDecodeFailure, ServerMediaPcmFrame};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ServerMediaRemoteTrack {
    pub track_id: String,
    pub kind: ServerMediaTrackKind,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ServerMediaTrackKind {
    Audio,
    Video,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ServerMediaRtpPacket {
    pub track_id: String,
    pub sequence_number: u16,
    pub timestamp: u32,
    pub marker: bool,
    pub payload_type: u8,
    pub payload: Vec<u8>,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct MediaIngressRecorder {
    inner: Arc<Mutex<MediaIngressState>>,
}

#[derive(Debug, Default)]
struct MediaIngressState {
    remote_tracks: Vec<ServerMediaRemoteTrack>,
    received_rtp_packets: Vec<ServerMediaRtpPacket>,
    pcm_frames: Vec<ServerMediaPcmFrame>,
    decode_failures: Vec<ServerMediaDecodeFailure>,
}

impl MediaIngressRecorder {
    pub(crate) fn record_remote_track(&self, track: ServerMediaRemoteTrack) {
        self.inner
            .lock()
            .expect("media ingress recorder lock must not be poisoned")
            .remote_tracks
            .push(track);
    }

    pub(crate) fn record_rtp_packet(&self, packet: ServerMediaRtpPacket) {
        self.inner
            .lock()
            .expect("media ingress recorder lock must not be poisoned")
            .received_rtp_packets
            .push(packet);
    }

    pub(crate) fn record_pcm_frame(&self, frame: ServerMediaPcmFrame) {
        self.inner
            .lock()
            .expect("media ingress recorder lock must not be poisoned")
            .pcm_frames
            .push(frame);
    }

    pub(crate) fn record_decode_failure(&self, failure: ServerMediaDecodeFailure) {
        self.inner
            .lock()
            .expect("media ingress recorder lock must not be poisoned")
            .decode_failures
            .push(failure);
    }

    pub(crate) fn remote_tracks(&self) -> Vec<ServerMediaRemoteTrack> {
        self.inner
            .lock()
            .expect("media ingress recorder lock must not be poisoned")
            .remote_tracks
            .clone()
    }

    pub(crate) fn received_rtp_packets(&self) -> Vec<ServerMediaRtpPacket> {
        self.inner
            .lock()
            .expect("media ingress recorder lock must not be poisoned")
            .received_rtp_packets
            .clone()
    }

    pub(crate) fn drain_pcm_frames(&self) -> Vec<ServerMediaPcmFrame> {
        std::mem::take(
            &mut self
                .inner
                .lock()
                .expect("media ingress recorder lock must not be poisoned")
                .pcm_frames,
        )
    }

    pub(crate) fn drain_decode_failures(&self) -> Vec<ServerMediaDecodeFailure> {
        std::mem::take(
            &mut self
                .inner
                .lock()
                .expect("media ingress recorder lock must not be poisoned")
                .decode_failures,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorder_returns_remote_track_and_rtp_snapshots() {
        let recorder = MediaIngressRecorder::default();

        recorder.record_remote_track(ServerMediaRemoteTrack {
            track_id: "audio-1".to_owned(),
            kind: ServerMediaTrackKind::Audio,
            mime_type: Some("audio/opus".to_owned()),
        });
        recorder.record_rtp_packet(ServerMediaRtpPacket {
            track_id: "audio-1".to_owned(),
            sequence_number: 7,
            timestamp: 48_000,
            marker: true,
            payload_type: 111,
            payload: vec![1, 2, 3],
        });
        recorder.record_pcm_frame(ServerMediaPcmFrame {
            track_id: "audio-1".to_owned(),
            sequence_number: 7,
            rtp_timestamp: 48_000,
            sample_rate_hz: 48_000,
            channels: 1,
            samples: vec![0.1, -0.1],
        });
        recorder.record_decode_failure(ServerMediaDecodeFailure {
            track_id: "audio-1".to_owned(),
            sequence_number: 8,
            rtp_timestamp: 48_960,
            error: "Input packet empty".to_owned(),
        });

        assert_eq!(
            recorder.remote_tracks(),
            vec![ServerMediaRemoteTrack {
                track_id: "audio-1".to_owned(),
                kind: ServerMediaTrackKind::Audio,
                mime_type: Some("audio/opus".to_owned()),
            }]
        );
        assert_eq!(
            recorder.received_rtp_packets(),
            vec![ServerMediaRtpPacket {
                track_id: "audio-1".to_owned(),
                sequence_number: 7,
                timestamp: 48_000,
                marker: true,
                payload_type: 111,
                payload: vec![1, 2, 3],
            }]
        );
        assert_eq!(
            recorder.drain_pcm_frames(),
            vec![ServerMediaPcmFrame {
                track_id: "audio-1".to_owned(),
                sequence_number: 7,
                rtp_timestamp: 48_000,
                sample_rate_hz: 48_000,
                channels: 1,
                samples: vec![0.1, -0.1],
            }]
        );
        assert!(recorder.drain_pcm_frames().is_empty());
        assert_eq!(
            recorder.drain_decode_failures(),
            vec![ServerMediaDecodeFailure {
                track_id: "audio-1".to_owned(),
                sequence_number: 8,
                rtp_timestamp: 48_960,
                error: "Input packet empty".to_owned(),
            }]
        );
        assert!(recorder.drain_decode_failures().is_empty());
    }
}
