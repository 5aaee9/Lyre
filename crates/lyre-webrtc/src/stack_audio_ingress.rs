use tracing::warn;

use crate::{
    egress::SERVER_MEDIA_EGRESS_PAYLOAD_TYPE, media_ingress::MediaIngressRecorder,
    ServerMediaDecodeError, ServerMediaDecodeFailure, ServerMediaJitterBuffer,
    ServerMediaJitterBufferOutput, ServerMediaOpusDecoder, ServerMediaPcmConcealer,
    ServerMediaRtpPacket,
};

pub(crate) const CONCEALMENT_UNAVAILABLE_ERROR: &str =
    "packet loss concealment required but not available with current Opus decoder";

pub(crate) fn handle_audio_rtp_packet(
    media_ingress: &MediaIngressRecorder,
    decoder: &mut ServerMediaOpusDecoder,
    jitter_buffer: &mut ServerMediaJitterBuffer,
    concealer: &mut ServerMediaPcmConcealer,
    packet: ServerMediaRtpPacket,
) {
    media_ingress.record_rtp_packet(packet.clone());

    if packet.payload_type != SERVER_MEDIA_EGRESS_PAYLOAD_TYPE {
        record_non_opus_payload(media_ingress, packet);
        return;
    }

    for output in jitter_buffer.push(packet) {
        match output {
            ServerMediaJitterBufferOutput::Packet(packet) => {
                decode_packet(media_ingress, decoder, concealer, packet);
            }
            ServerMediaJitterBufferOutput::ConcealmentRequired(event) => {
                record_concealment(media_ingress, concealer, event);
            }
        }
    }
}

fn record_non_opus_payload(media_ingress: &MediaIngressRecorder, packet: ServerMediaRtpPacket) {
    media_ingress.record_decode_failure(ServerMediaDecodeFailure {
        track_id: packet.track_id,
        sequence_number: packet.sequence_number,
        rtp_timestamp: packet.timestamp,
        error: format!(
            "server media audio RTP packet uses non-Opus payload type {}",
            packet.payload_type
        ),
    });
}

fn decode_packet(
    media_ingress: &MediaIngressRecorder,
    decoder: &mut ServerMediaOpusDecoder,
    concealer: &mut ServerMediaPcmConcealer,
    packet: ServerMediaRtpPacket,
) {
    match decoder.decode_packet(&packet) {
        Ok(frame) => {
            concealer.observe_decoded(frame.clone());
            media_ingress.record_pcm_frame(frame);
        }
        Err(error) => {
            let message = match &error {
                ServerMediaDecodeError::InvalidDecoderConfig { message }
                | ServerMediaDecodeError::Decode { message } => message.clone(),
            };
            warn!(error = %error, "failed to decode server media Opus RTP packet");
            media_ingress.record_decode_failure(ServerMediaDecodeFailure {
                track_id: packet.track_id,
                sequence_number: packet.sequence_number,
                rtp_timestamp: packet.timestamp,
                error: message,
            });
        }
    }
}

fn record_concealment(
    media_ingress: &MediaIngressRecorder,
    concealer: &mut ServerMediaPcmConcealer,
    event: crate::ServerMediaConcealmentRequired,
) {
    if let Some(frame) = concealer.conceal(&event) {
        media_ingress.record_pcm_frame(frame);
        return;
    }

    media_ingress.record_decode_failure(ServerMediaDecodeFailure {
        track_id: event.track_id,
        sequence_number: event.sequence_number,
        rtp_timestamp: event.rtp_timestamp,
        error: CONCEALMENT_UNAVAILABLE_ERROR.to_owned(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(payload_type: u8) -> ServerMediaRtpPacket {
        ServerMediaRtpPacket {
            track_id: "audio-main".to_owned(),
            sequence_number: 42,
            timestamp: 9_600,
            marker: true,
            payload_type,
            payload: vec![0x01, 0x02, 0x03],
        }
    }

    #[test]
    fn non_opus_payload_is_recorded_without_pcm_decode() {
        let media_ingress = MediaIngressRecorder::default();
        let mut decoder = ServerMediaOpusDecoder::new().unwrap();
        let mut jitter_buffer = ServerMediaJitterBuffer::default();
        let mut concealer = ServerMediaPcmConcealer::default();

        handle_audio_rtp_packet(
            &media_ingress,
            &mut decoder,
            &mut jitter_buffer,
            &mut concealer,
            packet(13),
        );

        assert_eq!(media_ingress.received_rtp_packets().len(), 1);
        assert!(media_ingress.drain_pcm_frames().is_empty());
        assert_eq!(
            media_ingress.drain_decode_failures(),
            vec![ServerMediaDecodeFailure {
                track_id: "audio-main".to_owned(),
                sequence_number: 42,
                rtp_timestamp: 9_600,
                error: "server media audio RTP packet uses non-Opus payload type 13".to_owned(),
            }]
        );
    }
}
