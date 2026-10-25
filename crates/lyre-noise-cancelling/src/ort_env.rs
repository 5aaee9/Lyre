use crate::{noise_runtime_error, NoiseCancellationError, NoiseProvider};
use ort::{
    environment::Environment,
    session::{builder::SessionBuilder, Session},
};
use std::sync::OnceLock;

// ONNX Runtime allows only one environment per process, shared by all providers.
static ORT_ENVIRONMENT: OnceLock<Result<Environment, String>> = OnceLock::new();

pub(crate) fn session_builder(
    provider: NoiseProvider,
) -> Result<SessionBuilder, NoiseCancellationError> {
    let environment = ORT_ENVIRONMENT
        .get_or_init(|| ort::init().build().map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|error| noise_runtime_error(provider, error))?;
    Session::builder(environment).map_err(|error| noise_runtime_error(provider, error))
}
