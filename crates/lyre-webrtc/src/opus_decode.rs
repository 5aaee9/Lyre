use crate::ServerMediaRtpPacket;
use opus_rs::OpusDecoder;
use thiserror::Error;

pub const SERVER_MEDIA_OPUS_SAMPLE_RATE_HZ: u32 = 48_000;
pub const SERVER_MEDIA_OPUS_CHANNELS: u16 = 1;
pub const SERVER_MEDIA_OPUS_FRAME_SIZE: usize = 960;
const DECODED_PCM_ARTIFACT_MIN_I16_ABS: i16 = 14_746;
const DECODED_PCM_ARTIFACT_MIN_REPEATED_SAMPLES: usize = SERVER_MEDIA_OPUS_FRAME_SIZE / 4;
const PCM_F32_TO_I16_SCALE: f32 = 32768.0;

#[derive(Debug, Clone, PartialEq)]
pub struct ServerMediaPcmFrame {
    pub track_id: String,
    pub sequence_number: u16,
    pub rtp_timestamp: u32,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerMediaDecodeFailure {
    pub track_id: String,
    pub sequence_number: u16,
    pub rtp_timestamp: u32,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ServerMediaDecodeError {
    #[error("failed to configure server media Opus decoder: {message}")]
    InvalidDecoderConfig { message: String },
    #[error("failed to decode server media Opus packet: {message}")]
    Decode { message: String },
}

pub struct ServerMediaOpusDecoder {
    decoder: OpusDecoder,
}

impl ServerMediaOpusDecoder {
    pub fn new() -> Result<Self, ServerMediaDecodeError> {
        Ok(Self {
            decoder: new_opus_decoder()?,
        })
    }

    pub fn decode_packet(
        &mut self,
        packet: &ServerMediaRtpPacket,
    ) -> Result<ServerMediaPcmFrame, ServerMediaDecodeError> {
        let mut samples =
            vec![0.0_f32; SERVER_MEDIA_OPUS_FRAME_SIZE * SERVER_MEDIA_OPUS_CHANNELS as usize];
        self.decoder
            .decode(&packet.payload, SERVER_MEDIA_OPUS_FRAME_SIZE, &mut samples)
            .map_err(|source| ServerMediaDecodeError::Decode {
                message: source.to_owned(),
            })?;
        suppress_repeated_high_amplitude_artifact(&mut samples);
        Ok(ServerMediaPcmFrame {
            track_id: packet.track_id.clone(),
            sequence_number: packet.sequence_number,
            rtp_timestamp: packet.timestamp,
            sample_rate_hz: SERVER_MEDIA_OPUS_SAMPLE_RATE_HZ,
            channels: SERVER_MEDIA_OPUS_CHANNELS,
            samples,
        })
    }
}

fn new_opus_decoder() -> Result<OpusDecoder, ServerMediaDecodeError> {
    let decoder = OpusDecoder::new(
        SERVER_MEDIA_OPUS_SAMPLE_RATE_HZ as i32,
        SERVER_MEDIA_OPUS_CHANNELS as usize,
    )
    .map_err(|source| ServerMediaDecodeError::InvalidDecoderConfig {
        message: source.to_owned(),
    })?;
    Ok(decoder)
}

fn suppress_repeated_high_amplitude_artifact(samples: &mut [f32]) {
    if has_repeated_high_amplitude_artifact(samples) {
        samples.fill(0.0);
    }
}

fn has_repeated_high_amplitude_artifact(samples: &[f32]) -> bool {
    let mut most_repeated_count = 0;

    for sample in samples {
        let quantized = quantized_i16_abs(*sample);
        if quantized < DECODED_PCM_ARTIFACT_MIN_I16_ABS {
            continue;
        }
        let count = samples
            .iter()
            .filter(|candidate| quantized_i16_abs(**candidate) == quantized)
            .count();
        if count > most_repeated_count {
            most_repeated_count = count;
        }
    }

    most_repeated_count >= DECODED_PCM_ARTIFACT_MIN_REPEATED_SAMPLES
}

fn quantized_i16_abs(sample: f32) -> i16 {
    (sample * PCM_F32_TO_I16_SCALE)
        .round()
        .clamp(i16::MIN as f32, i16::MAX as f32)
        .abs() as i16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ServerMediaRtpPacket;
    use opus_rs::{Application, OpusEncoder};

    fn valid_packet() -> ServerMediaRtpPacket {
        let mut encoder = OpusEncoder::new(
            SERVER_MEDIA_OPUS_SAMPLE_RATE_HZ as i32,
            SERVER_MEDIA_OPUS_CHANNELS as usize,
            Application::Voip,
        )
        .unwrap();
        let samples = (0..SERVER_MEDIA_OPUS_FRAME_SIZE)
            .map(|index| ((index as f32) / 24.0).sin() * 0.1)
            .collect::<Vec<_>>();
        let mut payload = vec![0_u8; 512];
        let payload_len = encoder
            .encode(&samples, SERVER_MEDIA_OPUS_FRAME_SIZE, &mut payload)
            .unwrap();
        payload.truncate(payload_len);

        ServerMediaRtpPacket {
            track_id: "audio-main".to_owned(),
            sequence_number: 42,
            timestamp: 9_600,
            marker: true,
            payload_type: 111,
            payload,
        }
    }

    fn packet_with_timestamp(timestamp: u32) -> ServerMediaRtpPacket {
        ServerMediaRtpPacket {
            timestamp,
            ..valid_packet()
        }
    }

    fn artifact_sample() -> f32 {
        16_710.0 / PCM_F32_TO_I16_SCALE
    }

    #[test]
    fn decoder_decodes_valid_opus_payload_to_pcm_frame() {
        let mut decoder = ServerMediaOpusDecoder::new().unwrap();

        let frame = decoder.decode_packet(&valid_packet()).unwrap();

        assert_eq!(frame.track_id, "audio-main");
        assert_eq!(frame.sequence_number, 42);
        assert_eq!(frame.rtp_timestamp, 9_600);
        assert_eq!(frame.sample_rate_hz, 48_000);
        assert_eq!(frame.channels, 1);
        assert_eq!(frame.samples.len(), SERVER_MEDIA_OPUS_FRAME_SIZE);
        assert!(frame.samples.iter().any(|sample| sample.abs() > 0.0));
    }

    #[test]
    fn decoder_keeps_state_after_rtp_timestamp_discontinuity() {
        let mut discontinuous = ServerMediaOpusDecoder::new().unwrap();
        discontinuous
            .decode_packet(&packet_with_timestamp(9_600))
            .unwrap();

        let after_gap = discontinuous
            .decode_packet(&packet_with_timestamp(96_000))
            .unwrap();
        let fresh = ServerMediaOpusDecoder::new()
            .unwrap()
            .decode_packet(&packet_with_timestamp(96_000))
            .unwrap();

        assert_ne!(after_gap.samples, fresh.samples);
    }

    #[test]
    fn suppresses_repeated_high_amplitude_decoded_pcm_artifact() {
        let mut samples = vec![0.0; SERVER_MEDIA_OPUS_FRAME_SIZE];
        samples[..DECODED_PCM_ARTIFACT_MIN_REPEATED_SAMPLES].fill(artifact_sample());
        samples[DECODED_PCM_ARTIFACT_MIN_REPEATED_SAMPLES] = 0.25;

        suppress_repeated_high_amplitude_artifact(&mut samples);

        assert!(samples.iter().all(|sample| *sample == 0.0));
    }

    #[test]
    fn keeps_sparse_high_amplitude_pcm_samples() {
        let mut samples = vec![0.0; SERVER_MEDIA_OPUS_FRAME_SIZE];
        samples[..DECODED_PCM_ARTIFACT_MIN_REPEATED_SAMPLES - 1].fill(artifact_sample());

        suppress_repeated_high_amplitude_artifact(&mut samples);

        assert!(samples[..DECODED_PCM_ARTIFACT_MIN_REPEATED_SAMPLES - 1]
            .iter()
            .all(|sample| *sample == artifact_sample()));
    }

    #[test]
    fn decoder_rejects_empty_payload_with_decoder_message() {
        let mut decoder = ServerMediaOpusDecoder::new().unwrap();
        let mut packet = valid_packet();
        packet.payload.clear();

        assert_eq!(
            decoder.decode_packet(&packet),
            Err(ServerMediaDecodeError::Decode {
                message: "Input packet empty".to_owned(),
            })
        );
    }

    #[test]
    fn decoder_rejects_malformed_payload_with_decoder_message() {
        let mut decoder = ServerMediaOpusDecoder::new().unwrap();
        let mut packet = valid_packet();
        packet.payload = vec![0xff, 0xff, 0xff, 0xff];

        let error = decoder.decode_packet(&packet).unwrap_err();

        match error {
            ServerMediaDecodeError::Decode { message } => {
                assert!(!message.is_empty());
            }
            ServerMediaDecodeError::InvalidDecoderConfig { .. } => {
                panic!("malformed packet must fail at decode time")
            }
        }
    }
}
