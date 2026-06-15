use super::*;
use lyre_core::{AudioFrame, AudioFrameProcessor, RoomId, UserId};

mod deepfilternet;

fn config(provider: NoiseProvider) -> NoiseCancellationConfig {
    NoiseCancellationConfig {
        provider,
        intensity: 0.5,
        voice_activity_threshold: 0.35,
    }
}

fn rnnoise_frame() -> NoiseFrame<'static> {
    NoiseFrame {
        sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
        channels: RNNOISE_CHANNELS,
        samples: &[120.0; RNNOISE_FRAME_SIZE],
    }
}

fn decoded_opus_frame_samples() -> Vec<f32> {
    (0..RNNOISE_FRAME_SIZE * 2)
        .map(|index| ((index as f32) / 12.0).sin() * 120.0)
        .collect()
}

#[test]
fn factory_builds_off_passthrough() {
    let mut canceller = build_noise_canceller(config(NoiseProvider::Off)).unwrap();

    let output = canceller
        .process_frame(NoiseFrame {
            sample_rate_hz: 44_100,
            channels: 2,
            samples: &[0.25, -0.5, 0.75],
        })
        .unwrap();

    assert_eq!(output.samples, vec![0.25, -0.5, 0.75]);
    assert_eq!(output.voice_activity_probability, None);
}

#[test]
fn rnnoise_rejects_wrong_sample_rate_channels_and_frame_size() {
    let mut canceller = build_noise_canceller(config(NoiseProvider::Rnnoise)).unwrap();

    assert_eq!(
        canceller
            .process_frame(NoiseFrame {
                sample_rate_hz: 44_100,
                channels: 2,
                samples: &[0.0; 32],
            })
            .unwrap_err(),
        NoiseCancellationError::InvalidFrameShape {
            provider: NoiseProvider::Rnnoise,
            sample_rate_hz: 44_100,
            channels: 2,
            samples: 32,
            expected_sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
            expected_channels: RNNOISE_CHANNELS,
            expected_samples: RNNOISE_FRAME_SIZE,
        }
    );
}

#[test]
fn rnnoise_processes_480_sample_mono_frame_and_reports_vad() {
    let mut canceller = build_noise_canceller(config(NoiseProvider::Rnnoise)).unwrap();

    let output = canceller.process_frame(rnnoise_frame()).unwrap();

    assert_eq!(output.samples.len(), RNNOISE_FRAME_SIZE);
    let vad = output.voice_activity_probability.unwrap();
    assert!((0.0..=1.0).contains(&vad));
}

#[test]
fn rnnoise_processes_960_sample_mono_frame_in_chunks_and_reports_average_vad() {
    let mut canceller = build_noise_canceller(config(NoiseProvider::Rnnoise)).unwrap();
    let input = decoded_opus_frame_samples();

    let output = canceller
        .process_frame(NoiseFrame {
            sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
            channels: RNNOISE_CHANNELS,
            samples: &input,
        })
        .unwrap();

    assert_eq!(output.samples.len(), RNNOISE_FRAME_SIZE * 2);
    assert_ne!(output.samples, input);
    let vad = output.voice_activity_probability.unwrap();
    assert!((0.0..=1.0).contains(&vad));
}

#[test]
fn rnnoise_rejects_empty_or_non_multiple_frame_size() {
    let mut canceller = build_noise_canceller(config(NoiseProvider::Rnnoise)).unwrap();

    for samples in [Vec::new(), vec![0.0; RNNOISE_FRAME_SIZE + 1]] {
        let error = canceller
            .process_frame(NoiseFrame {
                sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
                channels: RNNOISE_CHANNELS,
                samples: &samples,
            })
            .unwrap_err();
        assert_eq!(
            error,
            NoiseCancellationError::InvalidFrameShape {
                provider: NoiseProvider::Rnnoise,
                sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
                channels: RNNOISE_CHANNELS,
                samples: samples.len(),
                expected_sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
                expected_channels: RNNOISE_CHANNELS,
                expected_samples: RNNOISE_FRAME_SIZE,
            }
        );
        assert!(error
            .to_string()
            .contains("non-empty multiple of 480 samples"));
    }
}

#[test]
fn audio_frame_processor_adapter_uses_rnnoise_for_valid_audio() {
    let processor = NoiseCancellingAudioFrameProcessor::default();
    let frame = AudioFrame {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
        track_id: "audio-main".to_owned(),
        sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
        channels: RNNOISE_CHANNELS,
        sequence: 1,
        samples: vec![120.0; RNNOISE_FRAME_SIZE],
    };

    let output = processor.process(&frame, &config(NoiseProvider::Rnnoise));

    assert_eq!(output.len(), RNNOISE_FRAME_SIZE);
}

#[test]
fn audio_frame_processor_adapter_uses_rnnoise_for_decoded_opus_frame() {
    let processor = NoiseCancellingAudioFrameProcessor::default();
    let input = decoded_opus_frame_samples();
    let frame = AudioFrame {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
        track_id: "audio-main".to_owned(),
        sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
        channels: RNNOISE_CHANNELS,
        sequence: 1,
        samples: input.clone(),
    };

    let output = processor.process(&frame, &config(NoiseProvider::Rnnoise));

    assert_eq!(output.len(), RNNOISE_FRAME_SIZE * 2);
    assert_ne!(output, input);
}

#[test]
fn audio_frame_processor_adapter_uses_deepfilternet_for_valid_audio() {
    let processor = NoiseCancellingAudioFrameProcessor::default();
    let input = decoded_opus_frame_samples();
    let frame = AudioFrame {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
        track_id: "audio-main".to_owned(),
        sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
        channels: DEEPFILTERNET_CHANNELS,
        sequence: 1,
        samples: input,
    };

    let output = processor.process(&frame, &config(NoiseProvider::Deepfilternet));

    assert_eq!(output.len(), DEEPFILTERNET_FRAME_SIZE * 2);
    assert!(output.iter().all(|sample| sample.is_finite()));
}

#[test]
fn audio_frame_processor_adapter_passthroughs_invalid_or_unsupported_frames() {
    let processor = NoiseCancellingAudioFrameProcessor::default();
    let frame = AudioFrame {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
        track_id: "audio-main".to_owned(),
        sample_rate_hz: 44_100,
        channels: 2,
        sequence: 1,
        samples: vec![0.1, -0.2, 0.3],
    };

    assert_eq!(
        processor.process(&frame, &config(NoiseProvider::Rnnoise)),
        frame.samples
    );
    assert_eq!(
        processor.process(&frame, &config(NoiseProvider::Deepfilternet)),
        frame.samples
    );
}

#[test]
fn audio_frame_processor_adapter_preserves_state_per_config_key() {
    let first = NoiseConfigKey::new(
        &NoiseCancellationConfig {
            provider: NoiseProvider::Rnnoise,
            intensity: 0.5,
            voice_activity_threshold: 0.35,
        },
        DeepFilterNetRuntimeConfig::default(),
    );
    let second = NoiseConfigKey::new(
        &NoiseCancellationConfig {
            provider: NoiseProvider::Rnnoise,
            intensity: f32::from_bits(0.5f32.to_bits()),
            voice_activity_threshold: 0.35,
        },
        DeepFilterNetRuntimeConfig::default(),
    );
    let third = NoiseConfigKey::new(
        &NoiseCancellationConfig {
            provider: NoiseProvider::Rnnoise,
            intensity: 0.6,
            voice_activity_threshold: 0.35,
        },
        DeepFilterNetRuntimeConfig::default(),
    );

    assert_eq!(first, second);
    assert_ne!(first, third);
}

#[test]
fn audio_frame_processor_adapter_preserves_deepfilternet_runtime_per_config_key() {
    let default = NoiseConfigKey::new(
        &NoiseCancellationConfig {
            provider: NoiseProvider::Deepfilternet,
            intensity: 0.5,
            voice_activity_threshold: 0.35,
        },
        DeepFilterNetRuntimeConfig::default(),
    );
    let custom = NoiseConfigKey::new(
        &NoiseCancellationConfig {
            provider: NoiseProvider::Deepfilternet,
            intensity: 0.5,
            voice_activity_threshold: 0.35,
        },
        DeepFilterNetRuntimeConfig {
            fft_size: 1920,
            hop_size: 960,
            ..DeepFilterNetRuntimeConfig::default()
        },
    );
    let rnnoise_default = NoiseConfigKey::new(
        &NoiseCancellationConfig {
            provider: NoiseProvider::Rnnoise,
            intensity: 0.5,
            voice_activity_threshold: 0.35,
        },
        DeepFilterNetRuntimeConfig::default(),
    );
    let rnnoise_custom = NoiseConfigKey::new(
        &NoiseCancellationConfig {
            provider: NoiseProvider::Rnnoise,
            intensity: 0.5,
            voice_activity_threshold: 0.35,
        },
        DeepFilterNetRuntimeConfig {
            fft_size: 1920,
            hop_size: 960,
            ..DeepFilterNetRuntimeConfig::default()
        },
    );

    assert_ne!(default, custom);
    assert_eq!(rnnoise_default, rnnoise_custom);
}
