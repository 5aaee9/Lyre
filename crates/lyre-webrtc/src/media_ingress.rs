use crate::{ServerMediaDecodeFailure, ServerMediaPcmFrame};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

const MAX_DECODE_FAILURES: usize = 128;
const MAX_MEDIA_BACKLOG: usize = 128;

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
    received_rtp_packets: VecDeque<ServerMediaRtpPacket>,
    pcm_frames: VecDeque<ServerMediaPcmFrame>,
    decode_failures: VecDeque<ServerMediaDecodeFailure>,
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
        let mut state = self
            .inner
            .lock()
            .expect("media ingress recorder lock must not be poisoned");
        if state.received_rtp_packets.len() == MAX_MEDIA_BACKLOG {
            state.received_rtp_packets.pop_front();
        }
        state.received_rtp_packets.push_back(packet);
    }

    pub(crate) fn record_pcm_frame(&self, frame: ServerMediaPcmFrame) {
        let mut state = self
            .inner
            .lock()
            .expect("media ingress recorder lock must not be poisoned");
        if state.pcm_frames.len() == MAX_MEDIA_BACKLOG {
            state.pcm_frames.pop_front();
        }
        state.pcm_frames.push_back(frame);
    }

    pub(crate) fn record_decode_failure(&self, failure: ServerMediaDecodeFailure) {
        let mut state = self
            .inner
            .lock()
            .expect("media ingress recorder lock must not be poisoned");
        if state.decode_failures.len() == MAX_DECODE_FAILURES {
            state.decode_failures.pop_front();
        }
        state.decode_failures.push_back(failure);
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
            .iter()
            .cloned()
            .collect()
    }

    pub(crate) fn drain_rtp_packets(&self) -> Vec<ServerMediaRtpPacket> {
        std::mem::take(
            &mut self
                .inner
                .lock()
                .expect("media ingress recorder lock must not be poisoned")
                .received_rtp_packets,
        )
        .into()
    }

    pub(crate) fn drain_pcm_frames(&self) -> Vec<ServerMediaPcmFrame> {
        std::mem::take(
            &mut self
                .inner
                .lock()
                .expect("media ingress recorder lock must not be poisoned")
                .pcm_frames,
        )
        .into()
    }

    pub(crate) fn drain_decode_failures(&self) -> Vec<ServerMediaDecodeFailure> {
        std::mem::take(
            &mut self
                .inner
                .lock()
                .expect("media ingress recorder lock must not be poisoned")
                .decode_failures,
        )
        .into()
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
            recorder.drain_rtp_packets(),
            vec![ServerMediaRtpPacket {
                track_id: "audio-1".to_owned(),
                sequence_number: 7,
                timestamp: 48_000,
                marker: true,
                payload_type: 111,
                payload: vec![1, 2, 3],
            }]
        );
        assert!(recorder.drain_rtp_packets().is_empty());
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

    #[test]
    fn recorder_bounds_live_rtp_and_pcm_backlogs_by_dropping_oldest_frames() {
        const EXPECTED_BACKLOG_LIMIT: usize = 128;
        let recorder = MediaIngressRecorder::default();

        for sequence_number in 0..=EXPECTED_BACKLOG_LIMIT as u16 {
            recorder.record_rtp_packet(ServerMediaRtpPacket {
                track_id: "audio-1".to_owned(),
                sequence_number,
                timestamp: u32::from(sequence_number) * 960,
                marker: false,
                payload_type: 111,
                payload: vec![sequence_number as u8],
            });
            recorder.record_pcm_frame(ServerMediaPcmFrame {
                track_id: "audio-1".to_owned(),
                sequence_number,
                rtp_timestamp: u32::from(sequence_number) * 960,
                sample_rate_hz: 48_000,
                channels: 1,
                samples: vec![sequence_number as f32],
            });
        }

        let rtp_packets = recorder.drain_rtp_packets();
        assert_eq!(rtp_packets.len(), EXPECTED_BACKLOG_LIMIT);
        assert_eq!(rtp_packets.first().unwrap().sequence_number, 1);
        assert_eq!(
            rtp_packets.last().unwrap().sequence_number,
            EXPECTED_BACKLOG_LIMIT as u16
        );
        let pcm_frames = recorder.drain_pcm_frames();
        assert_eq!(pcm_frames.len(), EXPECTED_BACKLOG_LIMIT);
        assert_eq!(pcm_frames.first().unwrap().sequence_number, 1);
        assert_eq!(
            pcm_frames.last().unwrap().sequence_number,
            EXPECTED_BACKLOG_LIMIT as u16
        );
    }
}
