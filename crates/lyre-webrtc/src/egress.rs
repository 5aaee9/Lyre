use crate::{payload_dump::PayloadDumper, SERVER_MEDIA_OPUS_FRAME_SIZE};
use bytes::Bytes;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind,
};
use std::{
    error::Error,
    ffi::{c_int, c_uchar, c_void},
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
const LIBOPUS_APPLICATION_AUDIO: c_int = 2049;
const LIBOPUS_OK: c_int = 0;

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
}

pub(crate) struct ServerMediaOpusEgress {
    encoder: LibOpusEncoder,
    sequence_number: u16,
    timestamp: u32,
    next_source_rtp_timestamp: Option<u32>,
}

#[derive(Clone)]
pub(crate) struct ServerMediaEgress {
    track: Arc<TrackLocalStaticRTP>,
    encoder: Arc<Mutex<ServerMediaOpusEgress>>,
    sent_packets: Arc<Mutex<Vec<ServerMediaEgressRtpPacket>>>,
    payload_dumper: PayloadDumper,
}

impl ServerMediaEgress {
    pub(crate) fn new(payload_dumper: PayloadDumper) -> Result<Self, ServerMediaEgressError> {
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
            payload_dumper,
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
        packet: ServerMediaEgressRtpPacket,
    ) -> Result<usize, ServerMediaEgressError> {
        validate_opus_rtp_packet(&packet)?;
        self.track
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
            let payload_len =
                self.encoder
                    .encode(chunk, SERVER_MEDIA_OPUS_FRAME_SIZE, &mut payload)?;
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

fn new_opus_encoder() -> Result<LibOpusEncoder, ServerMediaEgressError> {
    LibOpusEncoder::new()
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

struct LibOpusEncoder {
    ptr: *mut c_void,
}

unsafe impl Send for LibOpusEncoder {}

impl LibOpusEncoder {
    fn new() -> Result<Self, ServerMediaEgressError> {
        let mut error = 0;
        let ptr = unsafe {
            opus_encoder_create(
                SERVER_MEDIA_EGRESS_SAMPLE_RATE_HZ as c_int,
                SERVER_MEDIA_EGRESS_CHANNELS as c_int,
                LIBOPUS_APPLICATION_AUDIO,
                &mut error,
            )
        };
        if ptr.is_null() || error != LIBOPUS_OK {
            return Err(ServerMediaEgressError::EncoderInit {
                message: opus_error_message(error),
            });
        }
        Ok(Self { ptr })
    }

    fn encode(
        &mut self,
        input: &[f32],
        frame_size: usize,
        output: &mut [u8],
    ) -> Result<usize, ServerMediaEgressError> {
        let encoded = unsafe {
            opus_encode_float(
                self.ptr,
                input.as_ptr(),
                frame_size as c_int,
                output.as_mut_ptr(),
                output.len() as c_int,
            )
        };
        if encoded < 0 {
            return Err(ServerMediaEgressError::Encode {
                message: opus_error_message(encoded),
            });
        }
        Ok(encoded as usize)
    }
}

impl Drop for LibOpusEncoder {
    fn drop(&mut self) {
        unsafe { opus_encoder_destroy(self.ptr) };
    }
}

fn opus_error_message(code: c_int) -> String {
    let ptr = unsafe { opus_strerror(code) };
    if ptr.is_null() {
        return format!("libopus error {code}");
    }
    unsafe { std::ffi::CStr::from_ptr(ptr.cast()) }
        .to_string_lossy()
        .into_owned()
}

#[link(name = "opus")]
extern "C" {
    fn opus_encoder_create(
        fs: c_int,
        channels: c_int,
        application: c_int,
        error: *mut c_int,
    ) -> *mut c_void;

    fn opus_encode_float(
        st: *mut c_void,
        pcm: *const f32,
        frame_size: c_int,
        data: *mut c_uchar,
        max_data_bytes: c_int,
    ) -> c_int;

    fn opus_encoder_destroy(st: *mut c_void);

    fn opus_strerror(error: c_int) -> *const c_uchar;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload_dump::PayloadDumper;
    use webrtc::media_stream::Track;

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
        let egress = ServerMediaEgress::new(PayloadDumper::disabled_for_test()).unwrap();

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
        let egress = ServerMediaEgress::new(PayloadDumper::disabled_for_test()).unwrap();
        let codec = egress.track().codec(5678).await.unwrap();

        assert_eq!(codec.channels, SERVER_MEDIA_EGRESS_CHANNELS);
    }

    #[test]
    fn egress_encoder_uses_libopus_audio_application_without_voip_preprocessing() {
        assert_eq!(LIBOPUS_APPLICATION_AUDIO, 2049);
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
        let mut error = 0;
        let decoder = unsafe {
            opus_decoder_create(
                SERVER_MEDIA_EGRESS_SAMPLE_RATE_HZ as c_int,
                SERVER_MEDIA_EGRESS_CHANNELS as c_int,
                &mut error,
            )
        };
        assert_eq!(error, LIBOPUS_OK);
        assert!(!decoder.is_null());

        let mut output = vec![0.0; SERVER_MEDIA_OPUS_FRAME_SIZE];
        let decoded = unsafe {
            opus_decode_float(
                decoder,
                payload.as_ptr(),
                payload.len() as c_int,
                output.as_mut_ptr(),
                SERVER_MEDIA_OPUS_FRAME_SIZE as c_int,
                0,
            )
        };
        unsafe { opus_decoder_destroy(decoder) };
        assert_eq!(decoded, SERVER_MEDIA_OPUS_FRAME_SIZE as c_int);
        output
    }

    extern "C" {
        fn opus_decoder_create(fs: c_int, channels: c_int, error: *mut c_int) -> *mut c_void;

        fn opus_decode_float(
            st: *mut c_void,
            data: *const c_uchar,
            len: c_int,
            pcm: *mut f32,
            frame_size: c_int,
            decode_fec: c_int,
        ) -> c_int;

        fn opus_decoder_destroy(st: *mut c_void);
    }
}
