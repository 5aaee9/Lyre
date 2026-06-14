pub use lyre_core::{NoiseCancellationConfig, NoiseProvider};

use lyre_core::{AudioFrame, AudioFrameProcessor};
use nnnoiseless::DenoiseState;
use std::{collections::HashMap, sync::Mutex};
use thiserror::Error;

pub const RNNOISE_SAMPLE_RATE_HZ: u32 = 48_000;
pub const RNNOISE_CHANNELS: u16 = 1;
pub const RNNOISE_FRAME_SIZE: usize = DenoiseState::FRAME_SIZE;

#[derive(Debug, Clone, Copy)]
pub struct NoiseFrame<'a> {
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub samples: &'a [f32],
}

#[derive(Debug, Clone, PartialEq)]
pub struct NoiseFrameOutput {
    pub samples: Vec<f32>,
    pub voice_activity_probability: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum NoiseCancellationError {
    #[error("noise provider `{provider:?}` is not supported by the server runtime")]
    UnsupportedProvider { provider: NoiseProvider },
    #[error(
        "noise provider `{provider:?}` requires {expected_sample_rate_hz} Hz, {expected_channels} channel(s), and {expected_samples} samples, got {sample_rate_hz} Hz, {channels} channel(s), and {samples} samples"
    )]
    InvalidFrameShape {
        provider: NoiseProvider,
        sample_rate_hz: u32,
        channels: u16,
        samples: usize,
        expected_sample_rate_hz: u32,
        expected_channels: u16,
        expected_samples: usize,
    },
}

pub trait NoiseCanceller: Send {
    fn process_frame(
        &mut self,
        frame: NoiseFrame<'_>,
    ) -> Result<NoiseFrameOutput, NoiseCancellationError>;
}

pub fn build_noise_canceller(
    config: NoiseCancellationConfig,
) -> Result<Box<dyn NoiseCanceller + Send>, NoiseCancellationError> {
    match config.provider {
        NoiseProvider::Off => Ok(Box::new(PassthroughNoiseCanceller::new(config))),
        NoiseProvider::Rnnoise => Ok(Box::new(RnnoiseNoiseCanceller::new(config))),
        NoiseProvider::Deepfilternet => Err(NoiseCancellationError::UnsupportedProvider {
            provider: NoiseProvider::Deepfilternet,
        }),
    }
}

#[derive(Debug, Clone)]
pub struct PassthroughNoiseCanceller {
    config: NoiseCancellationConfig,
}

impl PassthroughNoiseCanceller {
    pub fn new(config: NoiseCancellationConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &NoiseCancellationConfig {
        &self.config
    }
}

impl Default for PassthroughNoiseCanceller {
    fn default() -> Self {
        Self::new(NoiseCancellationConfig::default())
    }
}

impl NoiseCanceller for PassthroughNoiseCanceller {
    fn process_frame(
        &mut self,
        frame: NoiseFrame<'_>,
    ) -> Result<NoiseFrameOutput, NoiseCancellationError> {
        Ok(NoiseFrameOutput {
            samples: frame.samples.to_vec(),
            voice_activity_probability: None,
        })
    }
}

pub struct RnnoiseNoiseCanceller {
    config: NoiseCancellationConfig,
    state: Box<DenoiseState<'static>>,
}

impl RnnoiseNoiseCanceller {
    pub fn new(config: NoiseCancellationConfig) -> Self {
        Self {
            config,
            state: DenoiseState::new(),
        }
    }

    pub fn config(&self) -> &NoiseCancellationConfig {
        &self.config
    }
}

impl NoiseCanceller for RnnoiseNoiseCanceller {
    fn process_frame(
        &mut self,
        frame: NoiseFrame<'_>,
    ) -> Result<NoiseFrameOutput, NoiseCancellationError> {
        validate_rnnoise_frame(frame)?;

        let mut output = vec![0.0; RNNOISE_FRAME_SIZE];
        let vad = self.state.process_frame(&mut output, frame.samples);
        Ok(NoiseFrameOutput {
            samples: output,
            voice_activity_probability: Some(vad),
        })
    }
}

fn validate_rnnoise_frame(frame: NoiseFrame<'_>) -> Result<(), NoiseCancellationError> {
    if frame.sample_rate_hz == RNNOISE_SAMPLE_RATE_HZ
        && frame.channels == RNNOISE_CHANNELS
        && frame.samples.len() == RNNOISE_FRAME_SIZE
    {
        return Ok(());
    }

    Err(NoiseCancellationError::InvalidFrameShape {
        provider: NoiseProvider::Rnnoise,
        sample_rate_hz: frame.sample_rate_hz,
        channels: frame.channels,
        samples: frame.samples.len(),
        expected_sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
        expected_channels: RNNOISE_CHANNELS,
        expected_samples: RNNOISE_FRAME_SIZE,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct NoiseConfigKey {
    provider: NoiseProviderKey,
    intensity_bits: u32,
    voice_activity_threshold_bits: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum NoiseProviderKey {
    Off,
    Rnnoise,
    Deepfilternet,
}

impl From<NoiseProvider> for NoiseProviderKey {
    fn from(provider: NoiseProvider) -> Self {
        match provider {
            NoiseProvider::Off => Self::Off,
            NoiseProvider::Rnnoise => Self::Rnnoise,
            NoiseProvider::Deepfilternet => Self::Deepfilternet,
        }
    }
}

impl From<&NoiseCancellationConfig> for NoiseConfigKey {
    fn from(config: &NoiseCancellationConfig) -> Self {
        Self {
            provider: NoiseProviderKey::from(config.provider),
            intensity_bits: config.intensity.to_bits(),
            voice_activity_threshold_bits: config.voice_activity_threshold.to_bits(),
        }
    }
}

#[derive(Default)]
pub struct NoiseCancellingAudioFrameProcessor {
    cancellers: Mutex<HashMap<NoiseConfigKey, Box<dyn NoiseCanceller + Send>>>,
}

impl AudioFrameProcessor for NoiseCancellingAudioFrameProcessor {
    fn process(&self, frame: &AudioFrame, noise: &NoiseCancellationConfig) -> Vec<f32> {
        let mut cancellers = self
            .cancellers
            .lock()
            .expect("noise canceller mutex poisoned");
        let key = NoiseConfigKey::from(noise);
        let canceller = match cancellers.get_mut(&key) {
            Some(canceller) => canceller,
            None => match build_noise_canceller(noise.clone()) {
                Ok(canceller) => cancellers.entry(key).or_insert(canceller),
                Err(error) => {
                    tracing::warn!(
                        error = format_args!("{error:#}"),
                        room_id = %frame.room_id,
                        user_id = %frame.user_id,
                        track_id = %frame.track_id,
                        sample_rate_hz = frame.sample_rate_hz,
                        channels = frame.channels,
                        samples = frame.samples.len(),
                        "noise canceller unavailable; passing audio frame through"
                    );
                    return frame.samples.clone();
                }
            },
        };

        match canceller.process_frame(NoiseFrame {
            sample_rate_hz: frame.sample_rate_hz,
            channels: frame.channels,
            samples: &frame.samples,
        }) {
            Ok(output) => output.samples,
            Err(error) => {
                tracing::warn!(
                    error = format_args!("{error:#}"),
                    room_id = %frame.room_id,
                    user_id = %frame.user_id,
                    track_id = %frame.track_id,
                    sample_rate_hz = frame.sample_rate_hz,
                    channels = frame.channels,
                    samples = frame.samples.len(),
                    "noise cancellation failed; passing audio frame through"
                );
                frame.samples.clone()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyre_core::{AudioFrame, AudioFrameProcessor, RoomId, UserId};

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
    fn factory_rejects_deepfilternet_until_real_backend_exists() {
        let result = build_noise_canceller(config(NoiseProvider::Deepfilternet));

        assert!(matches!(
            result,
            Err(NoiseCancellationError::UnsupportedProvider {
                provider: NoiseProvider::Deepfilternet,
            })
        ));
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
        let first = NoiseConfigKey::from(&NoiseCancellationConfig {
            provider: NoiseProvider::Rnnoise,
            intensity: 0.5,
            voice_activity_threshold: 0.35,
        });
        let second = NoiseConfigKey::from(&NoiseCancellationConfig {
            provider: NoiseProvider::Rnnoise,
            intensity: f32::from_bits(0.5f32.to_bits()),
            voice_activity_threshold: 0.35,
        });
        let third = NoiseConfigKey::from(&NoiseCancellationConfig {
            provider: NoiseProvider::Rnnoise,
            intensity: 0.6,
            voice_activity_threshold: 0.35,
        });

        assert_eq!(first, second);
        assert_ne!(first, third);
    }
}
