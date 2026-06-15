use tracing::warn;

use crate::{
    media_ingress::MediaIngressRecorder, ServerMediaDecodeError, ServerMediaDecodeFailure,
    ServerMediaJitterBuffer, ServerMediaJitterBufferOutput, ServerMediaOpusDecoder,
    ServerMediaRtpPacket,
};

pub(crate) const CONCEALMENT_UNAVAILABLE_ERROR: &str =
    "packet loss concealment required but not available with current Opus decoder";

pub(crate) fn handle_audio_rtp_packet(
    media_ingress: &MediaIngressRecorder,
    decoder: &mut ServerMediaOpusDecoder,
    jitter_buffer: &mut ServerMediaJitterBuffer,
    packet: ServerMediaRtpPacket,
) {
    media_ingress.record_rtp_packet(packet.clone());

    for output in jitter_buffer.push(packet) {
        match output {
            ServerMediaJitterBufferOutput::Packet(packet) => {
                decode_packet(media_ingress, decoder, packet);
            }
            ServerMediaJitterBufferOutput::ConcealmentRequired(event) => {
                media_ingress.record_decode_failure(ServerMediaDecodeFailure {
                    track_id: event.track_id,
                    sequence_number: event.sequence_number,
                    rtp_timestamp: event.rtp_timestamp,
                    error: CONCEALMENT_UNAVAILABLE_ERROR.to_owned(),
                });
            }
        }
    }
}

fn decode_packet(
    media_ingress: &MediaIngressRecorder,
    decoder: &mut ServerMediaOpusDecoder,
    packet: ServerMediaRtpPacket,
) {
    match decoder.decode_packet(&packet) {
        Ok(frame) => media_ingress.record_pcm_frame(frame),
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
