use super::{
    validate_frame, ServerMediaEgressError, ServerMediaProcessedAudioFrame,
    SERVER_MEDIA_EGRESS_CHANNELS, SERVER_MEDIA_EGRESS_SAMPLE_RATE_HZ,
};
use crate::SERVER_MEDIA_OPUS_FRAME_SIZE;

pub(super) const SERVER_MEDIA_EGRESS_GAP_FADE_SAMPLES: usize = 240;

pub(crate) struct ServerMediaOpusEgress {
    encoder: crate::libopus::SendOnly<crate::libopus::Encoder>,
    next_source_rtp_timestamp: Option<u32>,
}

impl ServerMediaOpusEgress {
    pub(crate) fn new() -> Result<Self, ServerMediaEgressError> {
        Ok(Self {
            encoder: new_opus_encoder()?,
            next_source_rtp_timestamp: None,
        })
    }

    pub(crate) fn encode(
        &mut self,
        frame: &ServerMediaProcessedAudioFrame,
    ) -> Result<Vec<Vec<u8>>, ServerMediaEgressError> {
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
        let mut payloads = Vec::new();
        for (chunk_index, chunk) in samples.chunks(SERVER_MEDIA_OPUS_FRAME_SIZE).enumerate() {
            let mut payload = vec![0_u8; 512];
            let payload_len = self
                .encoder
                .get_mut()
                .encode_float(chunk, SERVER_MEDIA_OPUS_FRAME_SIZE, &mut payload)
                .map_err(|message| ServerMediaEgressError::Encode { message })?;
            payload.truncate(payload_len);
            payloads.push(payload);
            if let Some(rtp_timestamp) = frame.rtp_timestamp {
                self.next_source_rtp_timestamp = Some(
                    rtp_timestamp
                        .wrapping_add(((chunk_index + 1) * SERVER_MEDIA_OPUS_FRAME_SIZE) as u32),
                );
            }
        }
        Ok(payloads)
    }
}

pub(super) fn fade_in_after_gap(samples: &[f32]) -> Vec<f32> {
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
