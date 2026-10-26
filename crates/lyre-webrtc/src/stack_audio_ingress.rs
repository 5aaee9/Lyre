use std::sync::Arc;

use rtc::peer_connection::configuration::media_engine::MIME_TYPE_OPUS;
use tracing::warn;
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};

use crate::{
    media_ingress::MediaIngressRecorder, payload_dump::PayloadDumper, ServerMediaDecodeError,
    ServerMediaDecodeFailure, ServerMediaJitterBuffer, ServerMediaJitterBufferOutput,
    ServerMediaOpusDecoder, ServerMediaPcmConcealer, ServerMediaRtpPacket,
};

/// How long an audio track may stay quiet before packets held behind a lost packet are
/// released, so the tail of a talk spurt is not delayed until the speaker talks again.
pub(crate) const JITTER_BUFFER_IDLE_FLUSH: std::time::Duration =
    std::time::Duration::from_millis(60);

pub(crate) const CONCEALMENT_UNAVAILABLE_ERROR: &str =
    "packet loss concealment required but not available with current Opus decoder";

pub(crate) async fn run_remote_audio_track(
    track: Arc<dyn TrackRemote>,
    track_id: String,
    media_ingress: MediaIngressRecorder,
    payload_dumper: PayloadDumper,
) {
    let mut decoder = match ServerMediaOpusDecoder::new() {
        Ok(decoder) => decoder,
        Err(error) => {
            warn!(error = %error, "failed to initialize server media Opus decoder");
            return;
        }
    };
    let mut jitter_buffer = ServerMediaJitterBuffer::default();
    let mut concealer = ServerMediaPcmConcealer::default();
    loop {
        let event = match tokio::time::timeout(JITTER_BUFFER_IDLE_FLUSH, track.poll()).await {
            Ok(Some(event)) => event,
            Ok(None) => break,
            Err(_) => {
                handle_jitter_buffer_outputs(
                    &media_ingress,
                    &mut decoder,
                    &mut concealer,
                    jitter_buffer.flush(),
                );
                continue;
            }
        };
        if let TrackRemoteEvent::OnRtpPacket(packet) = event {
            let negotiated_codec_is_opus = track
                .codec(packet.header.ssrc)
                .await
                .is_some_and(|codec| codec.mime_type.eq_ignore_ascii_case(MIME_TYPE_OPUS));
            let packet = ServerMediaRtpPacket {
                track_id: track_id.clone(),
                sequence_number: packet.header.sequence_number,
                timestamp: packet.header.timestamp,
                marker: packet.header.marker,
                payload_type: packet.header.payload_type,
                payload: packet.payload.to_vec(),
            };
            payload_dumper.dump_inbound(&packet);
            handle_audio_rtp_packet(
                &media_ingress,
                &mut decoder,
                &mut jitter_buffer,
                &mut concealer,
                negotiated_codec_is_opus,
                packet,
            );
        }
    }
}

pub(crate) fn handle_audio_rtp_packet(
    media_ingress: &MediaIngressRecorder,
    decoder: &mut ServerMediaOpusDecoder,
    jitter_buffer: &mut ServerMediaJitterBuffer,
    concealer: &mut ServerMediaPcmConcealer,
    negotiated_codec_is_opus: bool,
    packet: ServerMediaRtpPacket,
) {
    media_ingress.record_rtp_packet(packet.clone());

    if !negotiated_codec_is_opus {
        record_non_opus_payload(media_ingress, packet);
        return;
    }

    handle_jitter_buffer_outputs(
        media_ingress,
        decoder,
        concealer,
        jitter_buffer.push(packet),
    );
}

fn handle_jitter_buffer_outputs(
    media_ingress: &MediaIngressRecorder,
    decoder: &mut ServerMediaOpusDecoder,
    concealer: &mut ServerMediaPcmConcealer,
    outputs: Vec<ServerMediaJitterBufferOutput>,
) {
    for output in outputs {
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
            false,
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
