use crate::{payload_dump::PayloadDumper, SERVER_MEDIA_OPUS_FRAME_SIZE};
use bytes::Bytes;
use lyre_core::UserId;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind,
};
use std::{
    collections::BTreeMap,
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
const SERVER_MEDIA_EGRESS_GAP_FADE_SAMPLES: usize = 240;

#[derive(Debug, Clone, PartialEq)]
pub struct ServerMediaProcessedAudioFrame {
    pub sequence: u64,
    pub rtp_timestamp: Option<u32>,
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
    #[error("server media egress source `{source_user_id}` was not negotiated")]
    SourceNotNegotiated { source_user_id: UserId },
}

pub(crate) struct ServerMediaOpusEgress {
    encoder: crate::libopus::SendOnly<crate::libopus::Encoder>,
    sequence_number: u16,
    timestamp: u32,
    next_source_rtp_timestamp: Option<u32>,
}

#[derive(Clone)]
pub(crate) struct ServerMediaEgress {
    sources: Arc<BTreeMap<UserId, ServerMediaEgressSource>>,
    sent_packets: Arc<Mutex<Vec<ServerMediaEgressRtpPacket>>>,
    payload_dumper: PayloadDumper,
}

#[derive(Clone)]
pub(crate) struct ServerMediaEgressSource {
    track_id: String,
    track: Arc<TrackLocalStaticRTP>,
    encoder: Arc<Mutex<ServerMediaOpusEgress>>,
}

impl ServerMediaEgress {
    pub(crate) fn new(
        source_user_ids: &[UserId],
        payload_dumper: PayloadDumper,
    ) -> Result<Self, ServerMediaEgressError> {
        let mut sources = BTreeMap::new();
        for source_user_id in source_user_ids {
            let track_id = server_media_source_track_id(source_user_id);
            sources.insert(
                source_user_id.clone(),
                ServerMediaEgressSource {
                    track_id: track_id.clone(),
                    track: Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
                        track_id,
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
                },
            );
        }
        Ok(Self {
            sources: Arc::new(sources),
            sent_packets: Arc::new(Mutex::new(Vec::new())),
            payload_dumper,
        })
    }

    pub(crate) fn tracks(&self) -> Vec<Arc<TrackLocalStaticRTP>> {
        self.sources
            .values()
            .map(|source| Arc::clone(&source.track))
            .collect()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn track_ids_for_test(&self) -> Vec<String> {
        self.sources
            .values()
            .map(|source| source.track_id.clone())
            .collect()
    }

    pub(crate) async fn send_processed_audio_frame(
        &self,
        source_user_id: &UserId,
        frame: ServerMediaProcessedAudioFrame,
    ) -> Result<usize, ServerMediaEgressError> {
        let source = self.source(source_user_id)?;
        let packets = source
            .encoder
            .lock()
            .expect("server media egress lock must not be poisoned")
            .encode(&frame)?;
        for packet in &packets {
            source
                .track
                .write_rtp(egress_rtp_packet(packet))
                .await
                .map_err(|source| ServerMediaEgressError::WriteRtp {
                    source: Box::new(source),
                })?;
            self.payload_dumper.dump_outbound(packet);
        }
        self.sent_packets
            .lock()
            .expect("server media egress packet snapshots lock must not be poisoned")
            .extend(packets.iter().cloned());
        Ok(packets.len())
    }

    pub(crate) async fn send_opus_rtp_packet(
        &self,
        source_user_id: &UserId,
        packet: ServerMediaEgressRtpPacket,
    ) -> Result<usize, ServerMediaEgressError> {
        let source = self.source(source_user_id)?;
        validate_opus_rtp_packet(&packet)?;
        source
            .track
            .write_rtp(egress_rtp_packet(&packet))
            .await
            .map_err(|source| ServerMediaEgressError::WriteRtp {
                source: Box::new(source),
            })?;
        self.payload_dumper.dump_outbound(&packet);
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

    fn source(
        &self,
        source_user_id: &UserId,
    ) -> Result<ServerMediaEgressSource, ServerMediaEgressError> {
        self.sources.get(source_user_id).cloned().ok_or_else(|| {
            ServerMediaEgressError::SourceNotNegotiated {
                source_user_id: source_user_id.clone(),
            }
        })
    }
}

pub fn server_media_source_track_id(source_user_id: &UserId) -> String {
    format!("lyre-user:{}:audio", percent_encode_user_id(source_user_id))
}

fn percent_encode_user_id(source_user_id: &UserId) -> String {
    let mut encoded = String::new();
    for byte in source_user_id.as_str().as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(*byte as char);
            }
            byte => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
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
        Ok(Self {
            encoder: new_opus_encoder()?,
            sequence_number: 0,
            timestamp: 0,
            next_source_rtp_timestamp: None,
        })
    }

    pub(crate) fn encode(
        &mut self,
        frame: &ServerMediaProcessedAudioFrame,
    ) -> Result<Vec<ServerMediaEgressRtpPacket>, ServerMediaEgressError> {
        validate_frame(frame)?;
        let source_gap = frame.rtp_timestamp.is_some_and(|rtp_timestamp| {
            self.next_source_rtp_timestamp
                .is_some_and(|next| next != rtp_timestamp)
        });
        let samples = if source_gap {
            fade_in_after_gap(&frame.samples)
        } else {
            frame.samples.clone()
        };
        let mut packets = Vec::new();
        for (chunk_index, chunk) in samples.chunks(SERVER_MEDIA_OPUS_FRAME_SIZE).enumerate() {
            let mut payload = vec![0_u8; 512];
            let payload_len = self
                .encoder
                .get_mut()
                .encode_float(chunk, SERVER_MEDIA_OPUS_FRAME_SIZE, &mut payload)
                .map_err(|message| ServerMediaEgressError::Encode { message })?;
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
            if let Some(rtp_timestamp) = frame.rtp_timestamp {
                self.next_source_rtp_timestamp = Some(
                    rtp_timestamp
                        .wrapping_add(((chunk_index + 1) * SERVER_MEDIA_OPUS_FRAME_SIZE) as u32),
                );
            }
        }
        Ok(packets)
    }
}

fn fade_in_after_gap(samples: &[f32]) -> Vec<f32> {
    let fade_len = samples.len().min(SERVER_MEDIA_EGRESS_GAP_FADE_SAMPLES);
    samples
        .iter()
        .enumerate()
        .map(|(index, sample)| {
            if index < fade_len {
                sample * index as f32 / fade_len as f32
            } else {
                *sample
            }
        })
        .collect()
}

fn new_opus_encoder(
) -> Result<crate::libopus::SendOnly<crate::libopus::Encoder>, ServerMediaEgressError> {
    crate::libopus::Encoder::new(
        SERVER_MEDIA_EGRESS_SAMPLE_RATE_HZ,
        SERVER_MEDIA_EGRESS_CHANNELS,
        crate::libopus::APPLICATION_AUDIO,
    )
    .map(crate::libopus::SendOnly::new)
    .map_err(|message| ServerMediaEgressError::EncoderInit { message })
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
    use crate::payload_dump::PayloadDumper;
    use webrtc::media_stream::Track;

    fn source_user_id() -> UserId {
        UserId::from_external("source")
    }

    fn frame(samples: Vec<f32>) -> ServerMediaProcessedAudioFrame {
        ServerMediaProcessedAudioFrame {
            sequence: 7,
            rtp_timestamp: None,
            sample_rate_hz: SERVER_MEDIA_EGRESS_SAMPLE_RATE_HZ,
            channels: SERVER_MEDIA_EGRESS_CHANNELS,
            samples,
        }
    }

    fn frame_with_rtp_timestamp(
        rtp_timestamp: u32,
        samples: Vec<f32>,
    ) -> ServerMediaProcessedAudioFrame {
        ServerMediaProcessedAudioFrame {
            rtp_timestamp: Some(rtp_timestamp),
            ..frame(samples)
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
        let input = (0..SERVER_MEDIA_OPUS_FRAME_SIZE)
            .map(|index| ((index as f32) / 24.0).sin() * 0.1)
            .collect::<Vec<_>>();

        let packets = encoder.encode(&frame(input)).unwrap();
        let output = decode_with_libopus(&packets[0].payload);

        let peak = output.iter().map(|sample| sample.abs()).fold(0.0, f32::max);
        assert!(peak > 0.05, "peak={peak}");
    }

    #[test]
    fn gap_fade_starts_at_zero_and_reaches_original_pcm() {
        let input = vec![0.5; SERVER_MEDIA_OPUS_FRAME_SIZE];

        let faded = fade_in_after_gap(&input);

        assert_eq!(faded[0], 0.0);
        assert!(faded[1] > 0.0);
        assert!(faded[SERVER_MEDIA_EGRESS_GAP_FADE_SAMPLES - 1] < 0.5);
        assert_eq!(faded[SERVER_MEDIA_EGRESS_GAP_FADE_SAMPLES], 0.5);
        assert_eq!(faded[SERVER_MEDIA_OPUS_FRAME_SIZE - 1], 0.5);
    }

    #[test]
    fn egress_encoder_fades_pcm_after_source_rtp_timestamp_discontinuity() {
        let mut discontinuous = ServerMediaOpusEgress::new().unwrap();
        discontinuous
            .encode(&frame_with_rtp_timestamp(
                9_600,
                vec![0.5; SERVER_MEDIA_OPUS_FRAME_SIZE],
            ))
            .unwrap();

        let after_gap = discontinuous
            .encode(&frame_with_rtp_timestamp(
                96_000,
                vec![0.5; SERVER_MEDIA_OPUS_FRAME_SIZE],
            ))
            .unwrap();
        let continuous = ServerMediaOpusEgress::new()
            .unwrap()
            .encode(&frame_with_rtp_timestamp(
                9_600,
                vec![0.5; SERVER_MEDIA_OPUS_FRAME_SIZE],
            ))
            .unwrap();

        assert_ne!(after_gap[0].payload, continuous[0].payload);
        assert_eq!(after_gap[0].sequence_number, 1);
        assert_eq!(after_gap[0].timestamp, SERVER_MEDIA_OPUS_FRAME_SIZE as u32);
    }

    #[tokio::test]
    async fn unbound_track_write_preserves_source_error() {
        let source_user_id = source_user_id();
        let egress = ServerMediaEgress::new(
            std::slice::from_ref(&source_user_id),
            PayloadDumper::disabled_for_test(),
        )
        .unwrap();

        let error = egress
            .send_processed_audio_frame(
                &source_user_id,
                frame(vec![0.1; SERVER_MEDIA_OPUS_FRAME_SIZE]),
            )
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
        let source_user_id = source_user_id();
        let egress = ServerMediaEgress::new(
            std::slice::from_ref(&source_user_id),
            PayloadDumper::disabled_for_test(),
        )
        .unwrap();
        let codec = egress.tracks()[0].codec(5678).await.unwrap();

        assert_eq!(codec.channels, SERVER_MEDIA_EGRESS_CHANNELS);
    }

    #[test]
    fn egress_encoder_uses_libopus_audio_application_without_voip_preprocessing() {
        assert_eq!(crate::libopus::APPLICATION_AUDIO, 2049);
    }

    #[test]
    fn egress_encoder_output_is_libopus_decodable_after_near_silence() {
        let mut encoder = ServerMediaOpusEgress::new().unwrap();
        let mut peak = 0.0_f32;

        for index in 0..640 {
            let amplitude = if index % 97 == 0 { 0.0002 } else { 0.0 };
            let input = (0..SERVER_MEDIA_OPUS_FRAME_SIZE)
                .map(|sample| ((sample as f32) / 24.0).sin() * amplitude)
                .collect::<Vec<_>>();
            let packets = encoder.encode(&frame(input)).unwrap();
            let decoded = decode_with_libopus(&packets[0].payload);
            peak = peak.max(
                decoded
                    .iter()
                    .map(|sample| sample.abs())
                    .fold(0.0, f32::max),
            );
        }

        assert!(peak <= 0.01, "peak={peak}");
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

    fn decode_with_libopus(payload: &[u8]) -> Vec<f32> {
        let mut decoder = crate::libopus::Decoder::new(
            SERVER_MEDIA_EGRESS_SAMPLE_RATE_HZ,
            SERVER_MEDIA_EGRESS_CHANNELS,
        )
        .unwrap();
        let mut output = vec![0.0; SERVER_MEDIA_OPUS_FRAME_SIZE];
        let decoded = decoder
            .decode_float(payload, SERVER_MEDIA_OPUS_FRAME_SIZE, &mut output)
            .unwrap();
        assert_eq!(decoded, SERVER_MEDIA_OPUS_FRAME_SIZE);
        output
    }
}
