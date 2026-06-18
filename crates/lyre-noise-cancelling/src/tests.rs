use super::*;
use lyre_core::{AudioFrame, AudioFrameProcessor, DpdfNetConfig, RoomId, UserId};
use nnnoiseless::DenoiseState;

use crate::server::NoiseConfigKey;

mod deepfilternet;

fn config(provider: NoiseProvider) -> NoiseCancellationConfig {
    NoiseCancellationConfig {
        provider,
        intensity: 0.5,
        voice_activity_threshold: 0.35,
        ..NoiseCancellationConfig::default()
    }
}

fn dpdfnet_config(model: &str) -> NoiseCancellationConfig {
    NoiseCancellationConfig {
        provider: NoiseProvider::Dpdfnet,
        dpdfnet: DpdfNetConfig {
            model: model.to_owned(),
        },
        ..NoiseCancellationConfig::default()
    }
}

fn rnnoise_frame() -> NoiseFrame<'static> {
    NoiseFrame {
        sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
        channels: RNNOISE_CHANNELS,
        samples: &[0.1; RNNOISE_FRAME_SIZE],
    }
}

fn decoded_opus_frame_samples() -> Vec<f32> {
    (0..RNNOISE_FRAME_SIZE * 2)
        .map(|index| ((index as f32) / 12.0).sin() * 0.1)
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
fn factory_loads_dpdfnet_from_configured_model_directory() {
    let result = build_noise_canceller_with_model_config(
        dpdfnet_config("dpdfnet2_48khz_hr"),
        NoiseModelRuntimeConfig {
            dpdfnet: DpdfNetRuntimeConfig {
                model_dir: std::path::PathBuf::from("dpdfnet/onnx"),
                ..DpdfNetRuntimeConfig::default()
            },
            ..NoiseModelRuntimeConfig::default()
        },
    );

    let Err(error) = result else {
        panic!("expected missing DPDFNet model error");
    };
    assert!(matches!(
        error,
        NoiseCancellationError::ModelFileUnavailable {
            provider: NoiseProvider::Dpdfnet,
            ..
        }
    ));
    assert!(error
        .to_string()
        .contains("dpdfnet/onnx/dpdfnet2_48khz_hr.onnx"));
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
fn rnnoise_matches_16_bit_pcm_contract_for_decoded_opus_pcm() {
    let mut canceller = build_noise_canceller(config(NoiseProvider::Rnnoise)).unwrap();
    let mut reference_state = DenoiseState::new();
    let input = (0..RNNOISE_FRAME_SIZE)
        .map(|index| ((index as f32) / 24.0).sin() * 0.1)
        .collect::<Vec<_>>();

    let output = canceller
        .process_frame(NoiseFrame {
            sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
            channels: RNNOISE_CHANNELS,
            samples: &input,
        })
        .unwrap();

    let reference_input = input
        .iter()
        .map(|sample| sample * PCM_F32_TO_I16_SCALE)
        .collect::<Vec<_>>();
    let mut reference_output = vec![0.0; RNNOISE_FRAME_SIZE];
    reference_state.process_frame(&mut reference_output, &reference_input);

    let max_delta = output
        .samples
        .iter()
        .zip(reference_output.iter())
        .map(|(actual, expected)| (actual - expected / PCM_F32_TO_I16_SCALE).abs())
        .fold(0.0_f32, f32::max);
    assert!(max_delta < 0.000001, "max_delta={max_delta}");
}

#[test]
fn shared_rnnoise_processor_matches_server_canceller_output() {
    let input = (0..RNNOISE_FRAME_SIZE)
        .map(|index| ((index as f32) / 24.0).sin() * 0.1)
        .collect::<Vec<_>>();
    let mut shared = RnnoiseFrameProcessor::new();
    let mut server = build_noise_canceller(config(NoiseProvider::Rnnoise)).unwrap();

    let shared_output = shared.process_samples(&input);
    let server_output = server
        .process_frame(NoiseFrame {
            sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
            channels: RNNOISE_CHANNELS,
            samples: &input,
        })
        .unwrap();

    assert_eq!(shared_output.samples, server_output.samples);
    assert_eq!(
        Some(shared_output.voice_activity_probability),
        server_output.voice_activity_probability
    );
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
        rtp_timestamp: None,
        samples: vec![0.1; RNNOISE_FRAME_SIZE],
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
        rtp_timestamp: None,
        samples: input.clone(),
    };

    let output = processor.process(&frame, &config(NoiseProvider::Rnnoise));

    assert_eq!(output.len(), RNNOISE_FRAME_SIZE * 2);
    assert_ne!(output, input);
}

#[test]
fn audio_frame_processor_adapter_keeps_rnnoise_state_per_audio_source() {
    let processor = NoiseCancellingAudioFrameProcessor::default();
    let reference_processor = NoiseCancellingAudioFrameProcessor::default();
    let input = decoded_opus_frame_samples();
    let user_01_frame = AudioFrame {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
        track_id: "audio-main".to_owned(),
        sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
        channels: RNNOISE_CHANNELS,
        sequence: 1,
        rtp_timestamp: None,
        samples: input.clone(),
    };
    let user_02_frame = AudioFrame {
        user_id: UserId::from_external("user_02"),
        ..user_01_frame.clone()
    };

    processor.process(&user_01_frame, &config(NoiseProvider::Rnnoise));
    let output = processor.process(&user_02_frame, &config(NoiseProvider::Rnnoise));
    let expected = reference_processor.process(&user_02_frame, &config(NoiseProvider::Rnnoise));

    assert_eq!(output, expected);
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
        rtp_timestamp: None,
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
        rtp_timestamp: None,
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
    let frame = AudioFrame {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
        track_id: "audio-main".to_owned(),
        sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
        channels: RNNOISE_CHANNELS,
        sequence: 1,
        rtp_timestamp: None,
        samples: vec![0.1; RNNOISE_FRAME_SIZE],
    };
    let first = NoiseConfigKey::new(
        &frame,
        &NoiseCancellationConfig {
            provider: NoiseProvider::Rnnoise,
            intensity: 0.5,
            voice_activity_threshold: 0.35,
            ..NoiseCancellationConfig::default()
        },
        DeepFilterNetRuntimeConfig::default(),
    );
    let second = NoiseConfigKey::new(
        &frame,
        &NoiseCancellationConfig {
            provider: NoiseProvider::Rnnoise,
            intensity: f32::from_bits(0.5f32.to_bits()),
            voice_activity_threshold: 0.35,
            ..NoiseCancellationConfig::default()
        },
        DeepFilterNetRuntimeConfig::default(),
    );
    let third = NoiseConfigKey::new(
        &frame,
        &NoiseCancellationConfig {
            provider: NoiseProvider::Rnnoise,
            intensity: 0.6,
            voice_activity_threshold: 0.35,
            ..NoiseCancellationConfig::default()
        },
        DeepFilterNetRuntimeConfig::default(),
    );

    assert_eq!(first, second);
    assert_ne!(first, third);
}

#[test]
fn audio_frame_processor_adapter_preserves_deepfilternet_runtime_per_config_key() {
    let frame = AudioFrame {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
        track_id: "audio-main".to_owned(),
        sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
        channels: DEEPFILTERNET_CHANNELS,
        sequence: 1,
        rtp_timestamp: None,
        samples: vec![0.1; DEEPFILTERNET_FRAME_SIZE],
    };
    let default = NoiseConfigKey::new(
        &frame,
        &NoiseCancellationConfig {
            provider: NoiseProvider::Deepfilternet,
            intensity: 0.5,
            voice_activity_threshold: 0.35,
            ..NoiseCancellationConfig::default()
        },
        DeepFilterNetRuntimeConfig::default(),
    );
    let custom = NoiseConfigKey::new(
        &frame,
        &NoiseCancellationConfig {
            provider: NoiseProvider::Deepfilternet,
            intensity: 0.5,
            voice_activity_threshold: 0.35,
            ..NoiseCancellationConfig::default()
        },
        DeepFilterNetRuntimeConfig {
            model_dir: "custom-deepfilternet/onnx".into(),
            ..DeepFilterNetRuntimeConfig::default()
        },
    );
    let rnnoise_default = NoiseConfigKey::new(
        &frame,
        &NoiseCancellationConfig {
            provider: NoiseProvider::Rnnoise,
            intensity: 0.5,
            voice_activity_threshold: 0.35,
            ..NoiseCancellationConfig::default()
        },
        DeepFilterNetRuntimeConfig::default(),
    );
    let rnnoise_custom = NoiseConfigKey::new(
        &frame,
        &NoiseCancellationConfig {
            provider: NoiseProvider::Rnnoise,
            intensity: 0.5,
            voice_activity_threshold: 0.35,
            ..NoiseCancellationConfig::default()
        },
        DeepFilterNetRuntimeConfig {
            model_dir: "custom-deepfilternet/onnx".into(),
            ..DeepFilterNetRuntimeConfig::default()
        },
    );

    assert_ne!(default, custom);
    assert_eq!(rnnoise_default, rnnoise_custom);
}
