use crate::SERVER_MEDIA_OPUS_FRAME_SIZE;
use bytes::Bytes;
use opus_rs::{Application, OpusEncoder};
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind,
};
use std::{
    error::Error,
    sync::{Arc, Mutex},
};
use thiserror::Error;
use webrtc::media_stream::{
    track_local::{static_rtp::TrackLocalStaticRTP, TrackLocal},
    MediaStreamTrack,
};

pub const SERVER_MEDIA_EGRESS_PAYLOAD_TYPE: u8 = 111;
pub const SERVER_MEDIA_EGRESS_SAMPLE_RATE_HZ: u32 = 48_000;
pub const SERVER_MEDIA_EGRESS_CHANNELS: u16 = 1;
const SERVER_MEDIA_EGRESS_APPLICATION: Application = Application::Audio;

#[derive(Debug, Clone, PartialEq)]
pub struct ServerMediaProcessedAudioFrame {
    pub sequence: u64,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerMediaEgressRtpPacket {
    pub sequence_number: u16,
    pub timestamp: u32,
    pub payload_type: u8,
    pub payload: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum ServerMediaEgressError {
    #[error("server media egress requires 48 kHz audio, got {sample_rate_hz} Hz")]
    InvalidSampleRate { sample_rate_hz: u32 },
    #[error("server media egress requires mono audio, got {channels} channels")]
    InvalidChannels { channels: u16 },
    #[error(
        "server media egress requires non-empty 20 ms Opus frame chunks, got {samples} samples"
    )]
    InvalidFrameSize { samples: usize },
    #[error("failed to initialize server media Opus egress encoder")]
    EncoderInit { message: String },
    #[error("failed to encode server media Opus egress frame")]
    Encode { message: String },
    #[error("server media egress requires audio/opus RTP payload type 111, got {payload_type}")]
    InvalidPayloadType { payload_type: u8 },
    #[error("failed to write server media egress RTP packet")]
    WriteRtp {
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    #[error("server media egress peer is missing for room `{room_id}` user `{user_id}`")]
    PeerMissing {
        room_id: lyre_core::RoomId,
        user_id: lyre_core::UserId,
    },
}

pub(crate) struct ServerMediaOpusEgress {
    encoder: OpusEncoder,
    sequence_number: u16,
    timestamp: u32,
}

#[derive(Clone)]
pub(crate) struct ServerMediaEgress {
    track: Arc<TrackLocalStaticRTP>,
    encoder: Arc<Mutex<ServerMediaOpusEgress>>,
    sent_packets: Arc<Mutex<Vec<ServerMediaEgressRtpPacket>>>,
}

impl ServerMediaEgress {
    pub(crate) fn new() -> Result<Self, ServerMediaEgressError> {
        Ok(Self {
            track: Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
                "lyre-server-audio".to_owned(),
                "audio".to_owned(),
                "audio".to_owned(),
                RtpCodecKind::Audio,
                vec![RTCRtpEncodingParameters {
                    rtp_coding_parameters: RTCRtpCodingParameters {
                        ssrc: Some(5678),
                        ..Default::default()
                    },
                    codec: RTCRtpCodec {
                        mime_type: "audio/opus".to_owned(),
                        clock_rate: SERVER_MEDIA_EGRESS_SAMPLE_RATE_HZ,
                        channels: SERVER_MEDIA_EGRESS_CHANNELS,
                        sdp_fmtp_line: String::new(),
                        rtcp_feedback: vec![],
                    },
                    ..Default::default()
                }],
            ))),
            encoder: Arc::new(Mutex::new(ServerMediaOpusEgress::new()?)),
            sent_packets: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub(crate) fn track(&self) -> Arc<TrackLocalStaticRTP> {
        Arc::clone(&self.track)
    }

    pub(crate) async fn send_processed_audio_frame(
        &self,
        frame: ServerMediaProcessedAudioFrame,
    ) -> Result<usize, ServerMediaEgressError> {
        let packets = self
            .encoder
            .lock()
            .expect("server media egress lock must not be poisoned")
            .encode(&frame)?;
        for packet in &packets {
            self.track
                .write_rtp(egress_rtp_packet(packet))
                .await
                .map_err(|source| ServerMediaEgressError::WriteRtp {
                    source: Box::new(source),
                })?;
        }
        self.sent_packets
            .lock()
            .expect("server media egress packet snapshots lock must not be poisoned")
            .extend(packets.iter().cloned());
        Ok(packets.len())
    }

    pub(crate) async fn send_opus_rtp_packet(
        &self,
        packet: ServerMediaEgressRtpPacket,
    ) -> Result<usize, ServerMediaEgressError> {
        validate_opus_rtp_packet(&packet)?;
        self.track
            .write_rtp(egress_rtp_packet(&packet))
            .await
            .map_err(|source| ServerMediaEgressError::WriteRtp {
                source: Box::new(source),
            })?;
        self.sent_packets
            .lock()
            .expect("server media egress packet snapshots lock must not be poisoned")
            .push(packet);
        Ok(1)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn sent_packets_for_test(&self) -> Vec<ServerMediaEgressRtpPacket> {
        self.sent_packets
            .lock()
            .expect("server media egress packet snapshots lock must not be poisoned")
            .clone()
    }
}

fn egress_rtp_packet(packet: &ServerMediaEgressRtpPacket) -> rtc::rtp::Packet {
    rtc::rtp::Packet {
        header: rtc::rtp::Header {
            version: 2,
            sequence_number: packet.sequence_number,
            timestamp: packet.timestamp,
            marker: true,
            payload_type: packet.payload_type,
            ssrc: 5678,
            ..Default::default()
        },
        payload: Bytes::from(packet.payload.clone()),
    }
}

impl ServerMediaOpusEgress {
    pub(crate) fn new() -> Result<Self, ServerMediaEgressError> {
        let encoder = OpusEncoder::new(
            SERVER_MEDIA_EGRESS_SAMPLE_RATE_HZ as i32,
            SERVER_MEDIA_EGRESS_CHANNELS as usize,
            SERVER_MEDIA_EGRESS_APPLICATION,
        )
        .map_err(|source| ServerMediaEgressError::EncoderInit {
            message: source.to_owned(),
        })?;
        Ok(Self {
            encoder,
            sequence_number: 0,
            timestamp: 0,
        })
    }

    pub(crate) fn encode(
        &mut self,
        frame: &ServerMediaProcessedAudioFrame,
    ) -> Result<Vec<ServerMediaEgressRtpPacket>, ServerMediaEgressError> {
        validate_frame(frame)?;
        let mut packets = Vec::new();
        for samples in frame.samples.chunks(SERVER_MEDIA_OPUS_FRAME_SIZE) {
            let mut payload = vec![0_u8; 512];
            let payload_len = self
                .encoder
                .encode(samples, SERVER_MEDIA_OPUS_FRAME_SIZE, &mut payload)
                .map_err(|source| ServerMediaEgressError::Encode {
                    message: source.to_owned(),
                })?;
            payload.truncate(payload_len);
            packets.push(ServerMediaEgressRtpPacket {
                sequence_number: self.sequence_number,
                timestamp: self.timestamp,
                payload_type: SERVER_MEDIA_EGRESS_PAYLOAD_TYPE,
                payload,
            });
            self.sequence_number = self.sequence_number.wrapping_add(1);
            self.timestamp = self
                .timestamp
                .wrapping_add(SERVER_MEDIA_OPUS_FRAME_SIZE as u32);
        }
        Ok(packets)
    }
}

fn validate_frame(frame: &ServerMediaProcessedAudioFrame) -> Result<(), ServerMediaEgressError> {
    if frame.sample_rate_hz != SERVER_MEDIA_EGRESS_SAMPLE_RATE_HZ {
        return Err(ServerMediaEgressError::InvalidSampleRate {
            sample_rate_hz: frame.sample_rate_hz,
        });
    }
    if frame.channels != SERVER_MEDIA_EGRESS_CHANNELS {
        return Err(ServerMediaEgressError::InvalidChannels {
            channels: frame.channels,
        });
    }
    if frame.samples.is_empty()
        || !frame
            .samples
            .len()
            .is_multiple_of(SERVER_MEDIA_OPUS_FRAME_SIZE)
    {
        return Err(ServerMediaEgressError::InvalidFrameSize {
            samples: frame.samples.len(),
        });
    }
    Ok(())
}

fn validate_opus_rtp_packet(
    packet: &ServerMediaEgressRtpPacket,
) -> Result<(), ServerMediaEgressError> {
    if packet.payload_type != SERVER_MEDIA_EGRESS_PAYLOAD_TYPE {
        return Err(ServerMediaEgressError::InvalidPayloadType {
            payload_type: packet.payload_type,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use opus_rs::OpusDecoder;
    use webrtc::media_stream::Track;

    fn frame(samples: Vec<f32>) -> ServerMediaProcessedAudioFrame {
        ServerMediaProcessedAudioFrame {
            sequence: 7,
            sample_rate_hz: SERVER_MEDIA_EGRESS_SAMPLE_RATE_HZ,
            channels: SERVER_MEDIA_EGRESS_CHANNELS,
            samples,
        }
    }

    #[test]
    fn encodes_valid_processed_audio_frame_to_opus_rtp_payload() {
        let mut encoder = ServerMediaOpusEgress::new().unwrap();

        let packets = encoder
            .encode(&frame(vec![0.1; SERVER_MEDIA_OPUS_FRAME_SIZE]))
            .unwrap();

        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].sequence_number, 0);
        assert_eq!(packets[0].timestamp, 0);
        assert_eq!(packets[0].payload_type, SERVER_MEDIA_EGRESS_PAYLOAD_TYPE);
        assert!(!packets[0].payload.is_empty());

        let next = encoder
            .encode(&frame(vec![0.1; SERVER_MEDIA_OPUS_FRAME_SIZE]))
            .unwrap();
        assert_eq!(next[0].sequence_number, 1);
        assert_eq!(next[0].timestamp, SERVER_MEDIA_OPUS_FRAME_SIZE as u32);
    }

    #[test]
    fn encoded_processed_audio_decodes_to_audible_pcm() {
        let mut encoder = ServerMediaOpusEgress::new().unwrap();
        let mut decoder = OpusDecoder::new(
            SERVER_MEDIA_EGRESS_SAMPLE_RATE_HZ as i32,
            SERVER_MEDIA_EGRESS_CHANNELS as usize,
        )
        .unwrap();
        let input = (0..SERVER_MEDIA_OPUS_FRAME_SIZE)
            .map(|index| ((index as f32) / 24.0).sin() * 0.1)
            .collect::<Vec<_>>();

        let packets = encoder.encode(&frame(input)).unwrap();
        let mut output = vec![0.0; SERVER_MEDIA_OPUS_FRAME_SIZE];
        decoder
            .decode(
                &packets[0].payload,
                SERVER_MEDIA_OPUS_FRAME_SIZE,
                &mut output,
            )
            .unwrap();

        let peak = output.iter().map(|sample| sample.abs()).fold(0.0, f32::max);
        assert!(peak > 0.05, "peak={peak}");
    }

    #[tokio::test]
    async fn unbound_track_write_preserves_source_error() {
        let egress = ServerMediaEgress::new().unwrap();

        let error = egress
            .send_processed_audio_frame(frame(vec![0.1; SERVER_MEDIA_OPUS_FRAME_SIZE]))
            .await
            .unwrap_err();

        match error {
            ServerMediaEgressError::WriteRtp { source } => {
                assert!(source.to_string().contains("track is not binding yet"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_opus_rtp_payload_type() {
        let packet = ServerMediaEgressRtpPacket {
            sequence_number: 7,
            timestamp: 9_600,
            payload_type: 96,
            payload: vec![1, 2, 3],
        };

        assert!(matches!(
            validate_opus_rtp_packet(&packet),
            Err(ServerMediaEgressError::InvalidPayloadType { payload_type: 96 })
        ));
    }

    #[tokio::test]
    async fn egress_track_advertises_encoded_channel_count() {
        let egress = ServerMediaEgress::new().unwrap();
        let codec = egress.track().codec(5678).await.unwrap();

        assert_eq!(codec.channels, SERVER_MEDIA_EGRESS_CHANNELS);
    }

    #[test]
    fn egress_encoder_uses_audio_application_without_voip_preprocessing() {
        assert_eq!(SERVER_MEDIA_EGRESS_APPLICATION, Application::Audio);
    }

    #[test]
    fn rejects_invalid_processed_audio_shape() {
        let mut encoder = ServerMediaOpusEgress::new().unwrap();
        let mut invalid = frame(vec![0.1; SERVER_MEDIA_OPUS_FRAME_SIZE]);

        invalid.sample_rate_hz = 44_100;
        assert!(matches!(
            encoder.encode(&invalid),
            Err(ServerMediaEgressError::InvalidSampleRate {
                sample_rate_hz: 44_100
            })
        ));

        invalid.sample_rate_hz = SERVER_MEDIA_EGRESS_SAMPLE_RATE_HZ;
        invalid.channels = 2;
        assert!(matches!(
            encoder.encode(&invalid),
            Err(ServerMediaEgressError::InvalidChannels { channels: 2 })
        ));

        invalid.channels = SERVER_MEDIA_EGRESS_CHANNELS;
        invalid.samples.clear();
        assert!(matches!(
            encoder.encode(&invalid),
            Err(ServerMediaEgressError::InvalidFrameSize { samples: 0 })
        ));

        invalid.samples = vec![0.1; SERVER_MEDIA_OPUS_FRAME_SIZE - 1];
        assert!(matches!(
            encoder.encode(&invalid),
            Err(ServerMediaEgressError::InvalidFrameSize { .. })
        ));
    }
}
