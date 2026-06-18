use crate::{
    dsp::dpdfnet::DpdfNetDsp, model_file_unavailable, noise_runtime_error, NoiseCancellationConfig,
    NoiseCancellationError, NoiseCanceller, NoiseFrame, NoiseFrameOutput, NoiseProvider,
};
use ort::{
    inputs,
    session::{builder::GraphOptimizationLevel, ModelMetadata, Session},
    value::TensorRef,
};
use std::path::PathBuf;

mod resampler;

use resampler::DpdfNetResampler;

pub const DPDFNET_CHANNELS: u16 = 1;
pub const DPDFNET_DEFAULT_MODEL: &str = "dpdfnet2_48khz_hr";
pub const DPDFNET_DEFAULT_MODEL_DIR: &str = "dpdfnet/onnx";
pub const DPDFNET_DEFAULT_INTRA_THREADS: usize = 1;
pub const DPDFNET_DEFAULT_INTER_THREADS: usize = 1;
pub const DPDFNET_SERVER_SAMPLE_RATE_HZ: u32 = 48_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DpdfNetModelSpec {
    pub name: &'static str,
    pub sample_rate_hz: u32,
    pub window_len: usize,
    pub hop_size: usize,
}

pub const DPDFNET_SUPPORTED_MODELS: [DpdfNetModelSpec; 6] = [
    DpdfNetModelSpec {
        name: "baseline",
        sample_rate_hz: 16_000,
        window_len: 320,
        hop_size: 160,
    },
    DpdfNetModelSpec {
        name: "dpdfnet2",
        sample_rate_hz: 16_000,
        window_len: 320,
        hop_size: 160,
    },
    DpdfNetModelSpec {
        name: "dpdfnet4",
        sample_rate_hz: 16_000,
        window_len: 320,
        hop_size: 160,
    },
    DpdfNetModelSpec {
        name: "dpdfnet8",
        sample_rate_hz: 16_000,
        window_len: 320,
        hop_size: 160,
    },
    DpdfNetModelSpec {
        name: "dpdfnet2_48khz_hr",
        sample_rate_hz: 48_000,
        window_len: 960,
        hop_size: 480,
    },
    DpdfNetModelSpec {
        name: "dpdfnet8_48khz_hr",
        sample_rate_hz: 48_000,
        window_len: 960,
        hop_size: 480,
    },
];

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DpdfNetRuntimeConfig {
    pub model_dir: PathBuf,
    pub intra_threads: usize,
    pub inter_threads: usize,
}

impl Default for DpdfNetRuntimeConfig {
    fn default() -> Self {
        Self {
            model_dir: PathBuf::from(DPDFNET_DEFAULT_MODEL_DIR),
            intra_threads: DPDFNET_DEFAULT_INTRA_THREADS,
            inter_threads: DPDFNET_DEFAULT_INTER_THREADS,
        }
    }
}

impl DpdfNetRuntimeConfig {
    pub fn model_path(&self, model: &str) -> PathBuf {
        self.model_dir.join(format!("{model}.onnx"))
    }
}

pub fn dpdfnet_default_intra_threads() -> usize {
    DPDFNET_DEFAULT_INTRA_THREADS
}

pub fn dpdfnet_available_parallelism() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
}

pub struct DpdfNetNoiseCanceller {
    config: NoiseCancellationConfig,
    spec: DpdfNetModelSpec,
    session: Session,
    state: Vec<f32>,
    dsp: DpdfNetDsp,
    resampler: Option<DpdfNetResampler>,
    input_spec_name: String,
    input_state_name: String,
    output_spec_name: String,
    output_state_name: String,
}

impl DpdfNetNoiseCanceller {
    pub fn new(
        config: NoiseCancellationConfig,
        runtime: &DpdfNetRuntimeConfig,
    ) -> Result<Self, NoiseCancellationError> {
        let spec = spec_for_model(&config.dpdfnet.model)?;
        let model_path = runtime.model_path(spec.name);
        if !model_path.is_file() {
            return Err(model_file_unavailable(NoiseProvider::Dpdfnet, model_path));
        }

        let session = Session::builder()
            .map_err(|error| noise_runtime_error(NoiseProvider::Dpdfnet, error))?
            .with_optimization_level(GraphOptimizationLevel::All)
            .map_err(|error| noise_runtime_error(NoiseProvider::Dpdfnet, error))?
            .with_inter_op_spinning(false)
            .map_err(|error| noise_runtime_error(NoiseProvider::Dpdfnet, error))?
            .with_intra_op_spinning(false)
            .map_err(|error| noise_runtime_error(NoiseProvider::Dpdfnet, error))?
            .with_intra_threads(runtime.intra_threads)
            .map_err(|error| noise_runtime_error(NoiseProvider::Dpdfnet, error))?
            .with_inter_threads(runtime.inter_threads)
            .map_err(|error| noise_runtime_error(NoiseProvider::Dpdfnet, error))?
            .commit_from_file(&model_path)
            .map_err(|error| noise_runtime_error(NoiseProvider::Dpdfnet, error))?;
        let state = load_initial_state(&session)?;
        validate_state_shape(&session, &state)?;
        if session.inputs().len() < 2 || session.outputs().len() < 2 {
            return Err(noise_runtime_error(
                NoiseProvider::Dpdfnet,
                "DPDFNet ONNX model must expose spec/state inputs and spec/state outputs",
            ));
        }
        let input_spec_name = session.inputs()[0].name().to_owned();
        let input_state_name = session.inputs()[1].name().to_owned();
        let output_spec_name = session.outputs()[0].name().to_owned();
        let output_state_name = session.outputs()[1].name().to_owned();
        let dsp = DpdfNetDsp::new(spec.window_len, spec.hop_size).ok_or_else(|| {
            noise_runtime_error(NoiseProvider::Dpdfnet, "invalid DPDFNet DSP shape")
        })?;

        Ok(Self {
            config,
            spec,
            session,
            state,
            dsp,
            resampler: DpdfNetResampler::new(DPDFNET_SERVER_SAMPLE_RATE_HZ, spec.sample_rate_hz)?,
            input_spec_name,
            input_state_name,
            output_spec_name,
            output_state_name,
        })
    }

    pub fn config(&self) -> &NoiseCancellationConfig {
        &self.config
    }

    pub fn spec(&self) -> DpdfNetModelSpec {
        self.spec
    }
}

impl NoiseCanceller for DpdfNetNoiseCanceller {
    fn process_frame(
        &mut self,
        frame: NoiseFrame<'_>,
    ) -> Result<NoiseFrameOutput, NoiseCancellationError> {
        let model_input_samples = match &mut self.resampler {
            Some(resampler) => resampler.downsample(frame)?,
            None => {
                validate_dpdfnet_frame(self.spec, frame)?;
                frame.samples.to_vec()
            }
        };

        let mut samples = Vec::with_capacity(model_input_samples.len());
        for chunk in model_input_samples.chunks_exact(self.spec.hop_size) {
            let mut spec = vec![0.0; (self.spec.window_len / 2 + 1) * 2];
            if !self.dsp.process_stft(chunk, &mut spec) {
                return Err(noise_runtime_error(
                    NoiseProvider::Dpdfnet,
                    "DPDFNet STFT preprocessing failed",
                ));
            }
            let spec_tensor = TensorRef::from_array_view((
                vec![1, 1, self.spec.window_len / 2 + 1, 2],
                spec.as_slice(),
            ))
            .map_err(|error| noise_runtime_error(NoiseProvider::Dpdfnet, error))?;
            let state_tensor =
                TensorRef::from_array_view((vec![self.state.len()], self.state.as_slice()))
                    .map_err(|error| noise_runtime_error(NoiseProvider::Dpdfnet, error))?;
            let outputs = self
                .session
                .run(inputs![
                    self.input_spec_name.as_str() => spec_tensor,
                    self.input_state_name.as_str() => state_tensor,
                ])
                .map_err(|error| noise_runtime_error(NoiseProvider::Dpdfnet, error))?;
            let (_, spec_out) = outputs[self.output_spec_name.as_str()]
                .try_extract_tensor::<f32>()
                .map_err(|error| noise_runtime_error(NoiseProvider::Dpdfnet, error))?;
            let (_, state_out) = outputs[self.output_state_name.as_str()]
                .try_extract_tensor::<f32>()
                .map_err(|error| noise_runtime_error(NoiseProvider::Dpdfnet, error))?;
            self.state.clear();
            self.state.extend_from_slice(state_out);
            let mut output = vec![0.0; self.spec.hop_size];
            if !self.dsp.process_istft(spec_out, &mut output) {
                return Err(noise_runtime_error(
                    NoiseProvider::Dpdfnet,
                    format!(
                        "DPDFNet output spec has {} values, expected {}",
                        spec_out.len(),
                        (self.spec.window_len / 2 + 1) * 2
                    ),
                ));
            }
            samples.extend(output);
        }

        let output_samples = match &mut self.resampler {
            Some(resampler) => resampler.upsample(&samples, frame.samples.len())?,
            None => samples,
        };

        Ok(NoiseFrameOutput {
            samples: output_samples,
            voice_activity_probability: None,
        })
    }
}

fn spec_for_model(model: &str) -> Result<DpdfNetModelSpec, NoiseCancellationError> {
    DPDFNET_SUPPORTED_MODELS
        .iter()
        .copied()
        .find(|spec| spec.name == model)
        .ok_or_else(|| {
            noise_runtime_error(
                NoiseProvider::Dpdfnet,
                format!("unsupported DPDFNet model `{model}`"),
            )
        })
}

fn load_initial_state(session: &Session) -> Result<Vec<f32>, NoiseCancellationError> {
    let metadata = session
        .metadata()
        .map_err(|error| noise_runtime_error(NoiseProvider::Dpdfnet, error))?;
    let state_size = metadata_i64(&metadata, "state_size")? as usize;
    let erb_norm_state_size = metadata_i64(&metadata, "erb_norm_state_size")? as usize;
    let spec_norm_state_size = metadata_i64(&metadata, "spec_norm_state_size")? as usize;
    let erb_norm_init = metadata_f32_list(&metadata, "erb_norm_init")?;
    let spec_norm_init = metadata_f32_list(&metadata, "spec_norm_init")?;
    let mut state = vec![0.0; state_size];
    state[..erb_norm_state_size].copy_from_slice(&erb_norm_init);
    state[erb_norm_state_size..erb_norm_state_size + spec_norm_state_size]
        .copy_from_slice(&spec_norm_init);
    Ok(state)
}

fn metadata_i64(metadata: &ModelMetadata<'_>, key: &str) -> Result<i64, NoiseCancellationError> {
    metadata
        .custom(key)
        .ok_or_else(|| {
            noise_runtime_error(
                NoiseProvider::Dpdfnet,
                format!("missing ONNX metadata `{key}`"),
            )
        })?
        .parse()
        .map_err(|error| noise_runtime_error(NoiseProvider::Dpdfnet, error))
}

fn metadata_f32_list(
    metadata: &ModelMetadata<'_>,
    key: &str,
) -> Result<Vec<f32>, NoiseCancellationError> {
    metadata
        .custom(key)
        .ok_or_else(|| {
            noise_runtime_error(
                NoiseProvider::Dpdfnet,
                format!("missing ONNX metadata `{key}`"),
            )
        })?
        .split(',')
        .map(|value| {
            value
                .parse()
                .map_err(|error| noise_runtime_error(NoiseProvider::Dpdfnet, error))
        })
        .collect()
}

fn validate_state_shape(session: &Session, state: &[f32]) -> Result<(), NoiseCancellationError> {
    let Some(input) = session.inputs().get(1) else {
        return Err(noise_runtime_error(
            NoiseProvider::Dpdfnet,
            "DPDFNet ONNX model is missing runtime state input",
        ));
    };
    let expected = input
        .dtype()
        .tensor_shape()
        .and_then(|shape| (shape.len() == 1 && shape[0] >= 0).then_some(shape[0] as usize));
    if let Some(expected) = expected {
        if expected != state.len() {
            return Err(noise_runtime_error(
                NoiseProvider::Dpdfnet,
                format!(
                    "DPDFNet initial state shape mismatch: expected {expected}, got {}",
                    state.len()
                ),
            ));
        }
    }
    Ok(())
}

fn validate_dpdfnet_frame(
    spec: DpdfNetModelSpec,
    frame: NoiseFrame<'_>,
) -> Result<(), NoiseCancellationError> {
    if frame.sample_rate_hz == spec.sample_rate_hz
        && frame.channels == DPDFNET_CHANNELS
        && !frame.samples.is_empty()
        && frame.samples.len().is_multiple_of(spec.hop_size)
    {
        return Ok(());
    }

    Err(NoiseCancellationError::InvalidFrameShape {
        provider: NoiseProvider::Dpdfnet,
        sample_rate_hz: frame.sample_rate_hz,
        channels: frame.channels,
        samples: frame.samples.len(),
        expected_sample_rate_hz: spec.sample_rate_hz,
        expected_channels: DPDFNET_CHANNELS,
        expected_samples: spec.hop_size,
    })
}
