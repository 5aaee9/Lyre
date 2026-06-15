mod deepfilternet;

pub use lyre_core::{NoiseCancellationConfig, NoiseProvider};

pub use deepfilternet::{
    DeepFilterNetNoiseCanceller, DeepFilterNetRuntimeConfig, DEEPFILTERNET_CHANNELS,
    DEEPFILTERNET_DEFAULT_ERB_BANDS, DEEPFILTERNET_DEFAULT_FFT_SIZE,
    DEEPFILTERNET_DEFAULT_MIN_ERB_FREQS, DEEPFILTERNET_FRAME_SIZE, DEEPFILTERNET_SAMPLE_RATE_HZ,
};

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
        "noise provider `{provider:?}` requires {expected_sample_rate_hz} Hz, {expected_channels} channel(s), and a non-empty multiple of {expected_samples} samples, got {sample_rate_hz} Hz, {channels} channel(s), and {samples} samples"
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
    #[error("invalid DeepFilterNet runtime config: {reason}")]
    InvalidDeepFilterNetRuntimeConfig { reason: String },
}

pub(crate) fn invalid_deepfilternet_runtime_config(
    reason: impl Into<String>,
) -> NoiseCancellationError {
    NoiseCancellationError::InvalidDeepFilterNetRuntimeConfig {
        reason: reason.into(),
    }
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
    build_noise_canceller_with_runtime_config(config, DeepFilterNetRuntimeConfig::default())
}

pub fn build_noise_canceller_with_runtime_config(
    config: NoiseCancellationConfig,
    deepfilternet_runtime: DeepFilterNetRuntimeConfig,
) -> Result<Box<dyn NoiseCanceller + Send>, NoiseCancellationError> {
    match config.provider {
        NoiseProvider::Off => Ok(Box::new(PassthroughNoiseCanceller::new(config))),
        NoiseProvider::Rnnoise => Ok(Box::new(RnnoiseNoiseCanceller::new(config))),
        NoiseProvider::Deepfilternet => Ok(Box::new(DeepFilterNetNoiseCanceller::new(
            config,
            deepfilternet_runtime,
        )?)),
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

        let mut samples = Vec::with_capacity(frame.samples.len());
        let mut vad_total = 0.0;
        let mut chunks = 0;

        for chunk in frame.samples.chunks_exact(RNNOISE_FRAME_SIZE) {
            let mut output = vec![0.0; RNNOISE_FRAME_SIZE];
            vad_total += self.state.process_frame(&mut output, chunk);
            samples.extend(output);
            chunks += 1;
        }

        Ok(NoiseFrameOutput {
            samples,
            voice_activity_probability: Some(vad_total / chunks as f32),
        })
    }
}

fn validate_rnnoise_frame(frame: NoiseFrame<'_>) -> Result<(), NoiseCancellationError> {
    if frame.sample_rate_hz == RNNOISE_SAMPLE_RATE_HZ
        && frame.channels == RNNOISE_CHANNELS
        && !frame.samples.is_empty()
        && frame.samples.len().is_multiple_of(RNNOISE_FRAME_SIZE)
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
    deepfilternet_runtime: Option<DeepFilterNetRuntimeConfig>,
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

impl NoiseConfigKey {
    fn new(
        config: &NoiseCancellationConfig,
        deepfilternet_runtime: DeepFilterNetRuntimeConfig,
    ) -> Self {
        Self {
            provider: NoiseProviderKey::from(config.provider),
            intensity_bits: config.intensity.to_bits(),
            voice_activity_threshold_bits: config.voice_activity_threshold.to_bits(),
            deepfilternet_runtime: (config.provider == NoiseProvider::Deepfilternet)
                .then_some(deepfilternet_runtime),
        }
    }
}

pub struct NoiseCancellingAudioFrameProcessor {
    cancellers: Mutex<HashMap<NoiseConfigKey, Box<dyn NoiseCanceller + Send>>>,
    deepfilternet_runtime: DeepFilterNetRuntimeConfig,
}

impl NoiseCancellingAudioFrameProcessor {
    pub fn new(deepfilternet_runtime: DeepFilterNetRuntimeConfig) -> Self {
        Self {
            cancellers: Mutex::new(HashMap::new()),
            deepfilternet_runtime,
        }
    }
}

impl Default for NoiseCancellingAudioFrameProcessor {
    fn default() -> Self {
        Self::new(DeepFilterNetRuntimeConfig::default())
    }
}

impl AudioFrameProcessor for NoiseCancellingAudioFrameProcessor {
    fn process(&self, frame: &AudioFrame, noise: &NoiseCancellationConfig) -> Vec<f32> {
        let mut cancellers = self
            .cancellers
            .lock()
            .expect("noise canceller mutex poisoned");
        let key = NoiseConfigKey::new(noise, self.deepfilternet_runtime);
        let canceller = match cancellers.get_mut(&key) {
            Some(canceller) => canceller,
            None => match build_noise_canceller_with_runtime_config(
                noise.clone(),
                self.deepfilternet_runtime,
            ) {
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
mod tests;
