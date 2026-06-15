use lyre_noise_cancelling::DeepFilterNetRuntimeConfig;
use thiserror::Error;

pub const DEFAULT_DEEPFILTERNET_FFT_SIZE: usize = 960;
pub const DEFAULT_DEEPFILTERNET_HOP_SIZE: usize = 480;
pub const DEFAULT_DEEPFILTERNET_ERB_BANDS: usize = 32;
pub const DEFAULT_DEEPFILTERNET_MIN_ERB_FREQS: usize = 2;

#[derive(Debug, Error, PartialEq)]
pub enum DeepFilterNetConfigError {
    #[error(transparent)]
    InvalidRuntimeConfig(#[from] lyre_noise_cancelling::NoiseCancellationError),
}

pub(crate) fn validate_deepfilternet_runtime(
    runtime: DeepFilterNetRuntimeConfig,
) -> Result<DeepFilterNetRuntimeConfig, DeepFilterNetConfigError> {
    Ok(runtime.validate()?)
}
