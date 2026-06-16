use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum DeepFilterNetConfigError {
    #[error(transparent)]
    InvalidRuntimeConfig(#[from] lyre_noise_cancelling::NoiseCancellationError),
}

pub(crate) fn validate_deepfilternet_runtime(
    runtime: lyre_noise_cancelling::DeepFilterNetRuntimeConfig,
) -> Result<lyre_noise_cancelling::DeepFilterNetRuntimeConfig, DeepFilterNetConfigError> {
    Ok(runtime.validate()?)
}
