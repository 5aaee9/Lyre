use super::*;

#[test]
fn factory_builds_deepfilternet_dsp_runtime() {
    let mut canceller = build_noise_canceller(config(NoiseProvider::Deepfilternet)).unwrap();

    let output = canceller
        .process_frame(NoiseFrame {
            sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
            channels: DEEPFILTERNET_CHANNELS,
            samples: &[0.25; DEEPFILTERNET_FRAME_SIZE],
        })
        .unwrap();

    assert_eq!(output.samples.len(), DEEPFILTERNET_FRAME_SIZE);
    assert!(output.samples.iter().all(|sample| sample.is_finite()));
    assert_eq!(output.voice_activity_probability, None);
}

#[test]
fn deepfilternet_rejects_wrong_sample_rate_channels_and_frame_size() {
    let mut canceller = build_noise_canceller(config(NoiseProvider::Deepfilternet)).unwrap();

    assert_eq!(
        canceller
            .process_frame(NoiseFrame {
                sample_rate_hz: 44_100,
                channels: 2,
                samples: &[0.0; 32],
            })
            .unwrap_err(),
        NoiseCancellationError::InvalidFrameShape {
            provider: NoiseProvider::Deepfilternet,
            sample_rate_hz: 44_100,
            channels: 2,
            samples: 32,
            expected_sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
            expected_channels: DEEPFILTERNET_CHANNELS,
            expected_samples: DEEPFILTERNET_FRAME_SIZE,
        }
    );
}

#[test]
fn deepfilternet_rejects_empty_or_non_multiple_frame_size() {
    let mut canceller = build_noise_canceller(config(NoiseProvider::Deepfilternet)).unwrap();

    for samples in [Vec::new(), vec![0.0; DEEPFILTERNET_FRAME_SIZE + 1]] {
        assert_eq!(
            canceller
                .process_frame(NoiseFrame {
                    sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
                    channels: DEEPFILTERNET_CHANNELS,
                    samples: &samples,
                })
                .unwrap_err(),
            NoiseCancellationError::InvalidFrameShape {
                provider: NoiseProvider::Deepfilternet,
                sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
                channels: DEEPFILTERNET_CHANNELS,
                samples: samples.len(),
                expected_sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
                expected_channels: DEEPFILTERNET_CHANNELS,
                expected_samples: DEEPFILTERNET_FRAME_SIZE,
            }
        );
    }
}

#[test]
fn deepfilternet_processes_960_sample_mono_frame_in_chunks() {
    let mut canceller = build_noise_canceller(config(NoiseProvider::Deepfilternet)).unwrap();
    let input = decoded_opus_frame_samples();

    let output = canceller
        .process_frame(NoiseFrame {
            sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
            channels: DEEPFILTERNET_CHANNELS,
            samples: &input,
        })
        .unwrap();

    assert_eq!(output.samples.len(), DEEPFILTERNET_FRAME_SIZE * 2);
    assert!(output.samples.iter().all(|sample| sample.is_finite()));
    assert_eq!(output.voice_activity_probability, None);
}

#[test]
fn deepfilternet_custom_runtime_accepts_configured_hop_size() {
    let runtime = DeepFilterNetRuntimeConfig {
        fft_size: 1920,
        hop_size: 960,
        ..DeepFilterNetRuntimeConfig::default()
    };
    let mut canceller =
        build_noise_canceller_with_runtime_config(config(NoiseProvider::Deepfilternet), runtime)
            .unwrap();
    let input = decoded_opus_frame_samples();

    let output = canceller
        .process_frame(NoiseFrame {
            sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
            channels: DEEPFILTERNET_CHANNELS,
            samples: &input,
        })
        .unwrap();

    assert_eq!(output.samples.len(), 960);
    assert!(output.samples.iter().all(|sample| sample.is_finite()));
}

#[test]
fn deepfilternet_custom_runtime_rejects_default_hop_size() {
    let runtime = DeepFilterNetRuntimeConfig {
        fft_size: 1920,
        hop_size: 960,
        ..DeepFilterNetRuntimeConfig::default()
    };
    let mut canceller =
        build_noise_canceller_with_runtime_config(config(NoiseProvider::Deepfilternet), runtime)
            .unwrap();

    assert_eq!(
        canceller
            .process_frame(NoiseFrame {
                sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
                channels: DEEPFILTERNET_CHANNELS,
                samples: &[0.0; DEEPFILTERNET_FRAME_SIZE],
            })
            .unwrap_err(),
        NoiseCancellationError::InvalidFrameShape {
            provider: NoiseProvider::Deepfilternet,
            sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
            channels: DEEPFILTERNET_CHANNELS,
            samples: DEEPFILTERNET_FRAME_SIZE,
            expected_sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
            expected_channels: DEEPFILTERNET_CHANNELS,
            expected_samples: 960,
        }
    );
}

#[test]
fn deepfilternet_runtime_rejects_invalid_hop_and_erb_configs() {
    assert!(DeepFilterNetRuntimeConfig {
        fft_size: 480,
        hop_size: 480,
        ..DeepFilterNetRuntimeConfig::default()
    }
    .validate()
    .is_err());

    assert!(DeepFilterNetRuntimeConfig {
        fft_size: 960,
        erb_bands: 300,
        min_erb_freqs: 2,
        ..DeepFilterNetRuntimeConfig::default()
    }
    .validate()
    .is_err());
}
