use crate::{
    egress_clock::EgressRtpClock, payload_dump::PayloadDumper, SERVER_MEDIA_OPUS_FRAME_SIZE,
};
use bytes::Bytes;
use lyre_core::UserId;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind,
};
use std::{
    collections::BTreeMap,
    error::Error,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex, OnceLock, RwLock,
    },
    time::Instant,
};
use thiserror::Error;
use webrtc::{
    media_stream::{
        track_local::{static_rtp::TrackLocalStaticRTP, TrackLocal},
        MediaStreamTrack,
    },
    rtp_transceiver::RtpSender,
};

pub const SERVER_MEDIA_EGRESS_PAYLOAD_TYPE: u8 = 111;
pub const SERVER_MEDIA_EGRESS_SAMPLE_RATE_HZ: u32 = 48_000;
pub const SERVER_MEDIA_EGRESS_CHANNELS: u16 = 1;
const SERVER_MEDIA_EGRESS_BASE_SSRC: u32 = 5678;

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

#[derive(Clone)]
pub(crate) struct ServerMediaEgress {
    sources: Arc<RwLock<BTreeMap<UserId, ServerMediaEgressSource>>>,
    next_ssrc: Arc<AtomicU32>,
    payload_dumper: PayloadDumper,
}

#[derive(Clone)]
pub(crate) struct ServerMediaEgressSource {
    track: Arc<TrackLocalStaticRTP>,
    sender: Arc<OnceLock<Arc<dyn RtpSender>>>,
    negotiated_payload_type: Arc<OnceLock<u8>>,
    clock: Arc<Mutex<EgressRtpClock>>,
    encoder: Arc<Mutex<ServerMediaOpusEgress>>,
    ssrc: u32,
}

impl ServerMediaEgress {
    pub(crate) fn new(
        source_user_ids: &[UserId],
        payload_dumper: PayloadDumper,
    ) -> Result<Self, ServerMediaEgressError> {
        let egress = Self {
            sources: Arc::default(),
            next_ssrc: Arc::new(AtomicU32::new(SERVER_MEDIA_EGRESS_BASE_SSRC)),
            payload_dumper,
        };
        {
            let mut sources = egress.write_sources();
            for source_user_id in source_user_ids {
                sources.insert(source_user_id.clone(), egress.new_source(source_user_id)?);
            }
        }
        Ok(egress)
    }

    pub(super) fn new_source(
        &self,
        source_user_id: &UserId,
    ) -> Result<ServerMediaEgressSource, ServerMediaEgressError> {
        let ssrc = self.next_ssrc.fetch_add(1, Ordering::Relaxed);
        let track_id = server_media_source_track_id(source_user_id);
        Ok(ServerMediaEgressSource {
            track: Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
                track_id.clone(),
                track_id,
                "audio".to_owned(),
                RtpCodecKind::Audio,
                vec![RTCRtpEncodingParameters {
                    rtp_coding_parameters: RTCRtpCodingParameters {
                        ssrc: Some(ssrc),
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
            sender: Arc::new(OnceLock::new()),
            negotiated_payload_type: Arc::new(OnceLock::new()),
            clock: Arc::new(Mutex::new(EgressRtpClock::new(0, 0))),
            encoder: Arc::new(Mutex::new(ServerMediaOpusEgress::new()?)),
            ssrc,
        })
    }

    pub(super) fn write_sources(
        &self,
    ) -> std::sync::RwLockWriteGuard<'_, BTreeMap<UserId, ServerMediaEgressSource>> {
        self.sources
            .write()
            .expect("server media egress sources lock must not be poisoned")
    }

    #[cfg(test)]
    pub(crate) fn tracks(&self) -> Vec<Arc<TrackLocalStaticRTP>> {
        self.sources
            .read()
            .expect("server media egress sources lock must not be poisoned")
            .values()
            .map(|source| Arc::clone(&source.track))
            .collect()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn track_ids_for_test(&self) -> Vec<String> {
        self.sources
            .read()
            .expect("server media egress sources lock must not be poisoned")
            .keys()
            .map(server_media_source_track_id)
            .collect()
    }

    pub(crate) async fn send_processed_audio_frame(
        &self,
        source_user_id: &UserId,
        frame: ServerMediaProcessedAudioFrame,
    ) -> Result<usize, ServerMediaEgressError> {
        let source = self.source(source_user_id)?;
        let payloads = source
            .encoder
            .lock()
            .expect("server media egress lock must not be poisoned")
            .encode(&frame)?;
        for payload in &payloads {
            self.write(&source, payload).await?;
        }
        Ok(payloads.len())
    }

    pub(crate) async fn send_opus_rtp_packet(
        &self,
        source_user_id: &UserId,
        packet: ServerMediaEgressRtpPacket,
    ) -> Result<usize, ServerMediaEgressError> {
        let source = self.source(source_user_id)?;
        validate_opus_rtp_packet(&packet)?;
        self.write(&source, &packet.payload).await?;
        Ok(1)
    }

    /// Writes one Opus payload on the source's egress stream. Sequence numbers and
    /// timestamps come from the stream's own clock, never from the source, so source
    /// reconnects and pauses cannot make the receiver drop packets as replays or late audio.
    async fn write(
        &self,
        source: &ServerMediaEgressSource,
        payload: &[u8],
    ) -> Result<(), ServerMediaEgressError> {
        let payload_type = source.payload_type().await?;
        let stamp = source
            .clock
            .lock()
            .expect("server media egress clock lock must not be poisoned")
            .next(Instant::now());
        source
            .track
            .write_rtp(rtc::rtp::Packet {
                header: rtc::rtp::Header {
                    version: 2,
                    sequence_number: stamp.sequence_number,
                    timestamp: stamp.timestamp,
                    marker: stamp.marker,
                    payload_type,
                    ssrc: source.ssrc,
                    ..Default::default()
                },
                payload: Bytes::copy_from_slice(payload),
            })
            .await
            .map_err(|source| ServerMediaEgressError::WriteRtp {
                source: Box::new(source),
            })?;
        self.payload_dumper
            .dump_outbound(&ServerMediaEgressRtpPacket {
                sequence_number: stamp.sequence_number,
                timestamp: stamp.timestamp,
                payload_type,
                payload: payload.to_vec(),
            });
        Ok(())
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn sent_packets_for_test(&self) -> Vec<ServerMediaEgressRtpPacket> {
        Vec::new()
    }

    fn source(
        &self,
        source_user_id: &UserId,
    ) -> Result<ServerMediaEgressSource, ServerMediaEgressError> {
        self.sources
            .read()
            .expect("server media egress sources lock must not be poisoned")
            .get(source_user_id)
            .cloned()
            .ok_or_else(|| ServerMediaEgressError::SourceNotNegotiated {
                source_user_id: source_user_id.clone(),
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

impl ServerMediaEgressSource {
    /// Returns the Opus payload type negotiated on this sender leg; the sender rejects
    /// payload types that were not negotiated. Cached once negotiation has produced it.
    async fn payload_type(&self) -> Result<u8, ServerMediaEgressError> {
        if let Some(payload_type) = self.negotiated_payload_type.get() {
            return Ok(*payload_type);
        }
        let Some(sender) = self.sender.get() else {
            return Ok(SERVER_MEDIA_EGRESS_PAYLOAD_TYPE);
        };
        let negotiated = sender
            .get_parameters()
            .await
            .map_err(|source| ServerMediaEgressError::WriteRtp {
                source: Box::new(source),
            })?
            .rtp_parameters
            .codecs
            .iter()
            .find(|codec| codec.rtp_codec.mime_type.eq_ignore_ascii_case("audio/opus"))
            .map(|codec| codec.payload_type);
        Ok(match negotiated {
            Some(payload_type) => *self.negotiated_payload_type.get_or_init(|| payload_type),
            None => SERVER_MEDIA_EGRESS_PAYLOAD_TYPE,
        })
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

mod opus;
mod sources;

pub(crate) use opus::ServerMediaOpusEgress;
#[cfg(test)]
use opus::{fade_in_after_gap, SERVER_MEDIA_EGRESS_GAP_FADE_SAMPLES};

#[cfg(test)]
#[path = "egress_tests.rs"]
mod tests;
