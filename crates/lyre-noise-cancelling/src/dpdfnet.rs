use self::streaming::{vorbis_window, IstftStreamingPostprocess, StftStreamingPreprocess};
use crate::{
    model_file_unavailable, noise_runtime_error, NoiseCancellationConfig, NoiseCancellationError,
    NoiseCanceller, NoiseFrame, NoiseFrameOutput, NoiseProvider,
};
use ort::{
    inputs,
    session::{builder::GraphOptimizationLevel, ModelMetadata, Session},
    value::TensorRef,
};
use rubato::{
    audioadapter_buffers::direct::SequentialSliceOfVecs, Async, FixedAsync, Indexing, Resampler,
    SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use std::path::PathBuf;

mod streaming;

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
    stft: StftStreamingPreprocess,
    istft: IstftStreamingPostprocess,
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
        let window = vorbis_window(spec.window_len);

        Ok(Self {
            config,
            spec,
            session,
            state,
            stft: StftStreamingPreprocess::new(spec.window_len, spec.hop_size, window.clone()),
            istft: IstftStreamingPostprocess::new(spec.window_len, spec.hop_size, window),
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
            let spec = self.stft.process(chunk);
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
            samples.extend(self.istft.process(spec_out)?);
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

struct DpdfNetResampler {
    source_sample_rate_hz: u32,
    model_sample_rate_hz: u32,
    source_hop_size: usize,
    model_hop_size: usize,
    downsampler: Async<f32>,
    upsampler: Async<f32>,
}

impl DpdfNetResampler {
    fn new(
        source_sample_rate_hz: u32,
        model_sample_rate_hz: u32,
    ) -> Result<Option<Self>, NoiseCancellationError> {
        if source_sample_rate_hz == model_sample_rate_hz {
            return Ok(None);
        }

        if source_sample_rate_hz != DPDFNET_SERVER_SAMPLE_RATE_HZ || model_sample_rate_hz != 16_000
        {
            return Err(noise_runtime_error(
                NoiseProvider::Dpdfnet,
                format!(
                    "unsupported DPDFNet resampling path: {source_sample_rate_hz} Hz input to {model_sample_rate_hz} Hz model"
                ),
            ));
        }

        Ok(Some(Self {
            source_sample_rate_hz,
            model_sample_rate_hz,
            source_hop_size: 480,
            model_hop_size: 160,
            downsampler: new_dpdfnet_resampler(source_sample_rate_hz, model_sample_rate_hz, 480)?,
            upsampler: new_dpdfnet_resampler(model_sample_rate_hz, source_sample_rate_hz, 160)?,
        }))
    }

    fn downsample(&mut self, frame: NoiseFrame<'_>) -> Result<Vec<f32>, NoiseCancellationError> {
        validate_resampled_dpdfnet_frame(
            frame,
            self.source_sample_rate_hz,
            DPDFNET_CHANNELS,
            self.source_hop_size,
        )?;
        let mut output =
            resample_dpdfnet_samples(&mut self.downsampler, frame.samples, self.source_hop_size)?;
        normalize_resampled_len(&mut output, frame.samples.len() / 3);
        Ok(output)
    }

    fn upsample(
        &mut self,
        samples: &[f32],
        expected_output_len: usize,
    ) -> Result<Vec<f32>, NoiseCancellationError> {
        validate_resampled_samples(
            samples,
            self.model_sample_rate_hz,
            self.source_sample_rate_hz,
        )?;
        let mut output =
            resample_dpdfnet_samples(&mut self.upsampler, samples, self.model_hop_size)?;
        normalize_resampled_len(&mut output, expected_output_len);
        Ok(output)
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

fn validate_resampled_dpdfnet_frame(
    frame: NoiseFrame<'_>,
    expected_sample_rate_hz: u32,
    expected_channels: u16,
    expected_samples: usize,
) -> Result<(), NoiseCancellationError> {
    if frame.sample_rate_hz == expected_sample_rate_hz
        && frame.channels == expected_channels
        && !frame.samples.is_empty()
        && frame.samples.len().is_multiple_of(expected_samples)
    {
        return Ok(());
    }

    Err(NoiseCancellationError::InvalidFrameShape {
        provider: NoiseProvider::Dpdfnet,
        sample_rate_hz: frame.sample_rate_hz,
        channels: frame.channels,
        samples: frame.samples.len(),
        expected_sample_rate_hz,
        expected_channels,
        expected_samples,
    })
}

fn validate_resampled_samples(
    samples: &[f32],
    sample_rate_hz: u32,
    output_sample_rate_hz: u32,
) -> Result<(), NoiseCancellationError> {
    let ratio = usize::try_from(output_sample_rate_hz / sample_rate_hz).unwrap_or(0);
    if !samples.is_empty() && ratio > 0 {
        return Ok(());
    }

    Err(noise_runtime_error(
        NoiseProvider::Dpdfnet,
        format!(
            "invalid DPDFNet resampler output from {sample_rate_hz} Hz to {output_sample_rate_hz} Hz"
        ),
    ))
}

fn new_dpdfnet_resampler(
    input_sample_rate_hz: u32,
    output_sample_rate_hz: u32,
    chunk_size: usize,
) -> Result<Async<f32>, NoiseCancellationError> {
    Async::<f32>::new_sinc(
        f64::from(output_sample_rate_hz) / f64::from(input_sample_rate_hz),
        1.0,
        &SincInterpolationParameters {
            sinc_len: 64,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 128,
            window: WindowFunction::BlackmanHarris2,
        },
        chunk_size,
        usize::from(DPDFNET_CHANNELS),
        FixedAsync::Input,
    )
    .map_err(|error| noise_runtime_error(NoiseProvider::Dpdfnet, error))
}

fn resample_dpdfnet_samples(
    resampler: &mut Async<f32>,
    samples: &[f32],
    input_chunk_size: usize,
) -> Result<Vec<f32>, NoiseCancellationError> {
    let input_data = vec![samples.to_vec()];
    let input =
        SequentialSliceOfVecs::new(&input_data, usize::from(DPDFNET_CHANNELS), samples.len())
            .map_err(|error| noise_runtime_error(NoiseProvider::Dpdfnet, error))?;
    let output_len = samples.len() / input_chunk_size * resampler.output_frames_max();
    let mut output_data = vec![vec![0.0; output_len]];
    let mut output =
        SequentialSliceOfVecs::new_mut(&mut output_data, usize::from(DPDFNET_CHANNELS), output_len)
            .map_err(|error| noise_runtime_error(NoiseProvider::Dpdfnet, error))?;
    let mut written = 0;
    for input_offset in (0..samples.len()).step_by(input_chunk_size) {
        let (read, chunk_written) = resampler
            .process_into_buffer(
                &input,
                &mut output,
                Some(&Indexing {
                    input_offset,
                    output_offset: written,
                    partial_len: None,
                    active_channels_mask: None,
                }),
            )
            .map_err(|error| noise_runtime_error(NoiseProvider::Dpdfnet, error))?;
        if read != input_chunk_size {
            return Err(noise_runtime_error(
                NoiseProvider::Dpdfnet,
                format!("DPDFNet resampler read {read} frames, expected {input_chunk_size}"),
            ));
        }
        written += chunk_written;
    }
    let mut samples = output_data.remove(0);
    samples.truncate(written);
    Ok(samples)
}

fn normalize_resampled_len(samples: &mut Vec<f32>, expected_len: usize) {
    match samples.len().cmp(&expected_len) {
        std::cmp::Ordering::Greater => samples.truncate(expected_len),
        std::cmp::Ordering::Less => samples.resize(expected_len, 0.0),
        std::cmp::Ordering::Equal => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpdfnet_resampler_converts_48khz_frames_for_16khz_models_and_restores_length() {
        let input = (0..960)
            .map(|index| ((index as f32) / 24.0).sin() * 0.1)
            .collect::<Vec<_>>();
        let mut resampler = DpdfNetResampler::new(48_000, 16_000).unwrap().unwrap();

        let model_input = resampler
            .downsample(NoiseFrame {
                sample_rate_hz: 48_000,
                channels: 1,
                samples: &input,
            })
            .unwrap();
        let restored = resampler.upsample(&model_input, input.len()).unwrap();

        assert_eq!(model_input.len(), 320);
        assert_eq!(restored.len(), input.len());
        assert!(restored.iter().all(|sample| sample.is_finite()));
    }
}
