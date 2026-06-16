use super::*;

fn local_deepfilternet_runtime() -> Option<DeepFilterNetRuntimeConfig> {
    let model_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(DEEPFILTERNET_DEFAULT_MODEL_DIR);
    model_dir
        .join("enc.onnx")
        .is_file()
        .then(|| DeepFilterNetRuntimeConfig {
            model_dir,
            ..DeepFilterNetRuntimeConfig::default()
        })
}

#[test]
fn factory_loads_deepfilternet3_from_configured_model_directory() {
    let result = build_noise_canceller_with_model_config(
        config(NoiseProvider::Deepfilternet),
        NoiseModelRuntimeConfig {
            deepfilternet: DeepFilterNetRuntimeConfig {
                model_dir: std::path::PathBuf::from("missing-deepfilternet/onnx"),
                ..DeepFilterNetRuntimeConfig::default()
            },
            ..NoiseModelRuntimeConfig::default()
        },
    );

    let Err(error) = result else {
        panic!("expected missing DeepFilterNet model error");
    };
    assert!(matches!(
        error,
        NoiseCancellationError::ModelFileUnavailable {
            provider: NoiseProvider::Deepfilternet,
            ..
        }
    ));
    assert!(error
        .to_string()
        .contains("missing-deepfilternet/onnx/enc.onnx"));
}

#[test]
fn deepfilternet3_processes_960_sample_mono_frame_in_chunks_when_model_is_available() {
    let Some(runtime) = local_deepfilternet_runtime() else {
        return;
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

    assert_eq!(output.samples.len(), DEEPFILTERNET_FRAME_SIZE * 2);
    assert!(output.samples.iter().all(|sample| sample.is_finite()));
    assert_eq!(output.voice_activity_probability, None);
}

#[test]
fn deepfilternet_rejects_wrong_sample_rate_channels_and_frame_size() {
    let Some(runtime) = local_deepfilternet_runtime() else {
        return;
    };
    let mut canceller =
        build_noise_canceller_with_runtime_config(config(NoiseProvider::Deepfilternet), runtime)
            .unwrap();

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
    let Some(runtime) = local_deepfilternet_runtime() else {
        return;
    };
    let mut canceller =
        build_noise_canceller_with_runtime_config(config(NoiseProvider::Deepfilternet), runtime)
            .unwrap();

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
fn deepfilternet_runtime_rejects_zero_threads() {
    assert!(DeepFilterNetRuntimeConfig {
        intra_threads: 0,
        ..DeepFilterNetRuntimeConfig::default()
    }
    .validate()
    .is_err());
}
