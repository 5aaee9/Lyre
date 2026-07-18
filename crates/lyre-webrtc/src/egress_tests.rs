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
