use crate::{
    model_file_unavailable, noise_runtime_error, NoiseCancellationConfig, NoiseCancellationError,
    NoiseCanceller, NoiseFrame, NoiseFrameOutput, NoiseProvider,
};
use df::{band_compr, band_mean_norm_erb, band_unit_norm, Complex32, DFState};
use ort::{
    inputs,
    session::{builder::GraphOptimizationLevel, Session},
    value::TensorRef,
};
use std::path::PathBuf;

pub const DEEPFILTERNET_SAMPLE_RATE_HZ: u32 = 48_000;
pub const DEEPFILTERNET_CHANNELS: u16 = 1;
pub const DEEPFILTERNET_FRAME_SIZE: usize = 480;
pub const DEEPFILTERNET_FFT_SIZE: usize = 960;
pub const DEEPFILTERNET_ERB_BANDS: usize = 32;
pub const DEEPFILTERNET_MIN_ERB_FREQS: usize = 2;
pub const DEEPFILTERNET_DEFAULT_MODEL_DIR: &str = "deepfilternet/onnx";
pub const DEEPFILTERNET_DEFAULT_INTRA_THREADS: usize = 1;
pub const DEEPFILTERNET_DEFAULT_INTER_THREADS: usize = 1;

const ENC_MODEL: &str = "enc.onnx";
const ERB_DEC_MODEL: &str = "erb_dec.onnx";
const DF_DEC_MODEL: &str = "df_dec.onnx";
const DF_ORDER: usize = 5;
const DF_BINS: usize = 96;
const ALPHA: f32 = 0.99;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeepFilterNetRuntimeConfig {
    pub model_dir: PathBuf,
    pub intra_threads: usize,
    pub inter_threads: usize,
}

impl Default for DeepFilterNetRuntimeConfig {
    fn default() -> Self {
        Self {
            model_dir: PathBuf::from(DEEPFILTERNET_DEFAULT_MODEL_DIR),
            intra_threads: DEEPFILTERNET_DEFAULT_INTRA_THREADS,
            inter_threads: DEEPFILTERNET_DEFAULT_INTER_THREADS,
        }
    }
}

impl DeepFilterNetRuntimeConfig {
    pub fn encoder_path(&self) -> PathBuf {
        self.model_dir.join(ENC_MODEL)
    }

    pub fn erb_decoder_path(&self) -> PathBuf {
        self.model_dir.join(ERB_DEC_MODEL)
    }

    pub fn df_decoder_path(&self) -> PathBuf {
        self.model_dir.join(DF_DEC_MODEL)
    }

    pub fn validate(self) -> Result<Self, NoiseCancellationError> {
        if self.intra_threads == 0 {
            return Err(noise_runtime_error(
                NoiseProvider::Deepfilternet,
                "DeepFilterNet intra-op threads must be greater than zero",
            ));
        }
        if self.inter_threads == 0 {
            return Err(noise_runtime_error(
                NoiseProvider::Deepfilternet,
                "DeepFilterNet inter-op threads must be greater than zero",
            ));
        }
        Ok(self)
    }
}

pub struct DeepFilterNetNoiseCanceller {
    config: NoiseCancellationConfig,
    runtime: DeepFilterNetRuntimeConfig,
    state: DFState,
    encoder: Session,
    erb_decoder: Session,
    df_decoder: Session,
    erb_norm_state: Vec<f32>,
    spec_norm_state: Vec<f32>,
    spec_buffer: Vec<Vec<Complex32>>,
    encoder_inputs: EncoderInputs,
    encoder_outputs: EncoderOutputs,
    erb_decoder_inputs: ErbDecoderInputs,
    erb_decoder_output: String,
    df_decoder_inputs: DfDecoderInputs,
    df_decoder_output: String,
}

#[derive(Debug, Clone)]
struct EncoderInputs {
    feat_erb: String,
    feat_spec: String,
}

#[derive(Debug, Clone)]
struct EncoderOutputs {
    e0: String,
    e1: String,
    e2: String,
    e3: String,
    emb: String,
    c0: String,
}

#[derive(Debug, Clone)]
struct ErbDecoderInputs {
    emb: String,
    e3: String,
    e2: String,
    e1: String,
    e0: String,
}

#[derive(Debug, Clone)]
struct DfDecoderInputs {
    emb: String,
    c0: String,
}

impl DeepFilterNetNoiseCanceller {
    pub fn new(
        config: NoiseCancellationConfig,
        runtime: DeepFilterNetRuntimeConfig,
    ) -> Result<Self, NoiseCancellationError> {
        let runtime = runtime.validate()?;
        let encoder_path = runtime.encoder_path();
        let erb_decoder_path = runtime.erb_decoder_path();
        let df_decoder_path = runtime.df_decoder_path();
        for path in [&encoder_path, &erb_decoder_path, &df_decoder_path] {
            if !path.is_file() {
                return Err(model_file_unavailable(
                    NoiseProvider::Deepfilternet,
                    path.clone(),
                ));
            }
        }

        let encoder = build_session(&runtime, &encoder_path)?;
        let erb_decoder = build_session(&runtime, &erb_decoder_path)?;
        let df_decoder = build_session(&runtime, &df_decoder_path)?;
        let encoder_inputs = encoder_inputs(&encoder)?;
        let encoder_outputs = encoder_outputs(&encoder)?;
        let erb_decoder_inputs = erb_decoder_inputs(&erb_decoder)?;
        let erb_decoder_output = first_output_name(&erb_decoder, "DeepFilterNet ERB decoder")?;
        let df_decoder_inputs = df_decoder_inputs(&df_decoder)?;
        let df_decoder_output = first_output_name(&df_decoder, "DeepFilterNet DF decoder")?;

        Ok(Self {
            config,
            runtime,
            state: DFState::new(
                DEEPFILTERNET_SAMPLE_RATE_HZ as usize,
                DEEPFILTERNET_FFT_SIZE,
                DEEPFILTERNET_FRAME_SIZE,
                DEEPFILTERNET_ERB_BANDS,
                DEEPFILTERNET_MIN_ERB_FREQS,
            ),
            encoder,
            erb_decoder,
            df_decoder,
            erb_norm_state: vec![df::MEAN_NORM_INIT[0]; DEEPFILTERNET_ERB_BANDS],
            spec_norm_state: vec![df::UNIT_NORM_INIT[0]; DEEPFILTERNET_FFT_SIZE / 2 + 1],
            spec_buffer: vec![vec![Complex32::ZERO; DEEPFILTERNET_FFT_SIZE / 2 + 1]; DF_ORDER],
            encoder_inputs,
            encoder_outputs,
            erb_decoder_inputs,
            erb_decoder_output,
            df_decoder_inputs,
            df_decoder_output,
        })
    }

    pub fn config(&self) -> &NoiseCancellationConfig {
        &self.config
    }

    pub fn runtime(&self) -> &DeepFilterNetRuntimeConfig {
        &self.runtime
    }

    fn process_chunk(&mut self, chunk: &[f32]) -> Result<Vec<f32>, NoiseCancellationError> {
        let mut spec = vec![Complex32::ZERO; self.state.freq_size];
        self.state.analysis(chunk, &mut spec);
        self.spec_buffer.rotate_right(1);
        self.spec_buffer[0].copy_from_slice(&spec);

        let mut feat_erb = vec![0.0; DEEPFILTERNET_ERB_BANDS];
        let spec_power = spec
            .iter()
            .map(|bin| bin.re.mul_add(bin.re, bin.im * bin.im))
            .collect::<Vec<_>>();
        band_compr(&mut feat_erb, &spec_power, &self.state.erb);
        band_mean_norm_erb(&mut feat_erb, &mut self.erb_norm_state, ALPHA);

        let mut feat_spec = spec.clone();
        band_unit_norm(&mut feat_spec, &mut self.spec_norm_state, ALPHA);
        let feat_spec = complex_prefix_to_interleaved(&feat_spec, DF_BINS);

        let feat_erb_tensor = TensorRef::from_array_view((
            vec![1, 1, 1, DEEPFILTERNET_ERB_BANDS],
            feat_erb.as_slice(),
        ))
        .map_err(|error| noise_runtime_error(NoiseProvider::Deepfilternet, error))?;
        let feat_spec_tensor =
            TensorRef::from_array_view((vec![1, 2, 1, DF_BINS], feat_spec.as_slice()))
                .map_err(|error| noise_runtime_error(NoiseProvider::Deepfilternet, error))?;
        let (e0, e1, e2, e3, emb, c0) = {
            let encoder_outputs = self
                .encoder
                .run(inputs![
                    self.encoder_inputs.feat_erb.as_str() => feat_erb_tensor,
                    self.encoder_inputs.feat_spec.as_str() => feat_spec_tensor,
                ])
                .map_err(|error| noise_runtime_error(NoiseProvider::Deepfilternet, error))?;

            (
                tensor_output(&encoder_outputs, &self.encoder_outputs.e0)?,
                tensor_output(&encoder_outputs, &self.encoder_outputs.e1)?,
                tensor_output(&encoder_outputs, &self.encoder_outputs.e2)?,
                tensor_output(&encoder_outputs, &self.encoder_outputs.e3)?,
                tensor_output(&encoder_outputs, &self.encoder_outputs.emb)?,
                tensor_output(&encoder_outputs, &self.encoder_outputs.c0)?,
            )
        };

        let mask = self.run_erb_decoder(&emb, &e3, &e2, &e1, &e0)?;
        let coefs = self.run_df_decoder(&emb, &c0)?;

        apply_erb_mask(&mut spec, &mask, &self.state.erb);
        apply_deep_filter(&mut spec, &self.spec_buffer, &coefs);

        let mut output = vec![0.0; DEEPFILTERNET_FRAME_SIZE];
        self.state.synthesis(&mut spec, &mut output);
        Ok(output)
    }

    fn run_erb_decoder(
        &mut self,
        emb: &[f32],
        e3: &[f32],
        e2: &[f32],
        e1: &[f32],
        e0: &[f32],
    ) -> Result<Vec<f32>, NoiseCancellationError> {
        let emb_tensor = TensorRef::from_array_view((vec![1, 1, 512], emb))
            .map_err(|error| noise_runtime_error(NoiseProvider::Deepfilternet, error))?;
        let e3_tensor = TensorRef::from_array_view((vec![1, 64, 1, 8], e3))
            .map_err(|error| noise_runtime_error(NoiseProvider::Deepfilternet, error))?;
        let e2_tensor = TensorRef::from_array_view((vec![1, 64, 1, 8], e2))
            .map_err(|error| noise_runtime_error(NoiseProvider::Deepfilternet, error))?;
        let e1_tensor = TensorRef::from_array_view((vec![1, 64, 1, 16], e1))
            .map_err(|error| noise_runtime_error(NoiseProvider::Deepfilternet, error))?;
        let e0_tensor = TensorRef::from_array_view((vec![1, 64, 1, 32], e0))
            .map_err(|error| noise_runtime_error(NoiseProvider::Deepfilternet, error))?;
        let outputs = self
            .erb_decoder
            .run(inputs![
                self.erb_decoder_inputs.emb.as_str() => emb_tensor,
                self.erb_decoder_inputs.e3.as_str() => e3_tensor,
                self.erb_decoder_inputs.e2.as_str() => e2_tensor,
                self.erb_decoder_inputs.e1.as_str() => e1_tensor,
                self.erb_decoder_inputs.e0.as_str() => e0_tensor,
            ])
            .map_err(|error| noise_runtime_error(NoiseProvider::Deepfilternet, error))?;
        tensor_output(&outputs, &self.erb_decoder_output)
    }

    fn run_df_decoder(
        &mut self,
        emb: &[f32],
        c0: &[f32],
    ) -> Result<Vec<f32>, NoiseCancellationError> {
        let emb_tensor = TensorRef::from_array_view((vec![1, 1, 512], emb))
            .map_err(|error| noise_runtime_error(NoiseProvider::Deepfilternet, error))?;
        let c0_tensor = TensorRef::from_array_view((vec![1, 64, 1, DF_BINS], c0))
            .map_err(|error| noise_runtime_error(NoiseProvider::Deepfilternet, error))?;
        let outputs = self
            .df_decoder
            .run(inputs![
                self.df_decoder_inputs.emb.as_str() => emb_tensor,
                self.df_decoder_inputs.c0.as_str() => c0_tensor,
            ])
            .map_err(|error| noise_runtime_error(NoiseProvider::Deepfilternet, error))?;
        tensor_output(&outputs, &self.df_decoder_output)
    }
}

impl NoiseCanceller for DeepFilterNetNoiseCanceller {
    fn process_frame(
        &mut self,
        frame: NoiseFrame<'_>,
    ) -> Result<NoiseFrameOutput, NoiseCancellationError> {
        validate_deepfilternet_frame(frame)?;

        let mut samples = Vec::with_capacity(frame.samples.len());
        for chunk in frame.samples.chunks_exact(DEEPFILTERNET_FRAME_SIZE) {
            samples.extend(self.process_chunk(chunk)?);
        }

        Ok(NoiseFrameOutput {
            samples,
            voice_activity_probability: None,
        })
    }
}

fn build_session(
    runtime: &DeepFilterNetRuntimeConfig,
    path: &PathBuf,
) -> Result<Session, NoiseCancellationError> {
    Session::builder()
        .map_err(|error| noise_runtime_error(NoiseProvider::Deepfilternet, error))?
        .with_optimization_level(GraphOptimizationLevel::All)
        .map_err(|error| noise_runtime_error(NoiseProvider::Deepfilternet, error))?
        .with_inter_op_spinning(false)
        .map_err(|error| noise_runtime_error(NoiseProvider::Deepfilternet, error))?
        .with_intra_op_spinning(false)
        .map_err(|error| noise_runtime_error(NoiseProvider::Deepfilternet, error))?
        .with_intra_threads(runtime.intra_threads)
        .map_err(|error| noise_runtime_error(NoiseProvider::Deepfilternet, error))?
        .with_inter_threads(runtime.inter_threads)
        .map_err(|error| noise_runtime_error(NoiseProvider::Deepfilternet, error))?
        .commit_from_file(path)
        .map_err(|error| noise_runtime_error(NoiseProvider::Deepfilternet, error))
}

fn validate_deepfilternet_frame(frame: NoiseFrame<'_>) -> Result<(), NoiseCancellationError> {
    if frame.sample_rate_hz == DEEPFILTERNET_SAMPLE_RATE_HZ
        && frame.channels == DEEPFILTERNET_CHANNELS
        && !frame.samples.is_empty()
        && frame.samples.len().is_multiple_of(DEEPFILTERNET_FRAME_SIZE)
    {
        return Ok(());
    }

    Err(NoiseCancellationError::InvalidFrameShape {
        provider: NoiseProvider::Deepfilternet,
        sample_rate_hz: frame.sample_rate_hz,
        channels: frame.channels,
        samples: frame.samples.len(),
        expected_sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
        expected_channels: DEEPFILTERNET_CHANNELS,
        expected_samples: DEEPFILTERNET_FRAME_SIZE,
    })
}

fn encoder_inputs(session: &Session) -> Result<EncoderInputs, NoiseCancellationError> {
    Ok(EncoderInputs {
        feat_erb: input_name(session, 0, "DeepFilterNet encoder")?,
        feat_spec: input_name(session, 1, "DeepFilterNet encoder")?,
    })
}

fn encoder_outputs(session: &Session) -> Result<EncoderOutputs, NoiseCancellationError> {
    Ok(EncoderOutputs {
        e0: output_name(session, 0, "DeepFilterNet encoder")?,
        e1: output_name(session, 1, "DeepFilterNet encoder")?,
        e2: output_name(session, 2, "DeepFilterNet encoder")?,
        e3: output_name(session, 3, "DeepFilterNet encoder")?,
        emb: output_name(session, 4, "DeepFilterNet encoder")?,
        c0: output_name(session, 5, "DeepFilterNet encoder")?,
    })
}

fn erb_decoder_inputs(session: &Session) -> Result<ErbDecoderInputs, NoiseCancellationError> {
    Ok(ErbDecoderInputs {
        emb: input_name(session, 0, "DeepFilterNet ERB decoder")?,
        e3: input_name(session, 1, "DeepFilterNet ERB decoder")?,
        e2: input_name(session, 2, "DeepFilterNet ERB decoder")?,
        e1: input_name(session, 3, "DeepFilterNet ERB decoder")?,
        e0: input_name(session, 4, "DeepFilterNet ERB decoder")?,
    })
}

fn df_decoder_inputs(session: &Session) -> Result<DfDecoderInputs, NoiseCancellationError> {
    Ok(DfDecoderInputs {
        emb: input_name(session, 0, "DeepFilterNet DF decoder")?,
        c0: input_name(session, 1, "DeepFilterNet DF decoder")?,
    })
}

fn input_name(
    session: &Session,
    index: usize,
    model: &str,
) -> Result<String, NoiseCancellationError> {
    session
        .inputs()
        .get(index)
        .map(|input| input.name().to_owned())
        .ok_or_else(|| {
            noise_runtime_error(
                NoiseProvider::Deepfilternet,
                format!("{model} is missing input {index}"),
            )
        })
}

fn output_name(
    session: &Session,
    index: usize,
    model: &str,
) -> Result<String, NoiseCancellationError> {
    session
        .outputs()
        .get(index)
        .map(|output| output.name().to_owned())
        .ok_or_else(|| {
            noise_runtime_error(
                NoiseProvider::Deepfilternet,
                format!("{model} is missing output {index}"),
            )
        })
}

fn first_output_name(session: &Session, model: &str) -> Result<String, NoiseCancellationError> {
    output_name(session, 0, model)
}

fn tensor_output(
    outputs: &ort::session::SessionOutputs<'_>,
    name: &str,
) -> Result<Vec<f32>, NoiseCancellationError> {
    let (_, values) = outputs[name]
        .try_extract_tensor::<f32>()
        .map_err(|error| noise_runtime_error(NoiseProvider::Deepfilternet, error))?;
    Ok(values.to_vec())
}

fn complex_prefix_to_interleaved(spec: &[Complex32], bins: usize) -> Vec<f32> {
    let mut output = Vec::with_capacity(bins * 2);
    output.extend(spec.iter().take(bins).map(|bin| bin.re));
    output.extend(spec.iter().take(bins).map(|bin| bin.im));
    output
}

fn apply_erb_mask(spec: &mut [Complex32], mask: &[f32], erb: &[usize]) {
    let mut offset = 0;
    for (band_size, gain) in erb.iter().zip(mask.iter()) {
        for bin in &mut spec[offset..offset + band_size] {
            *bin *= *gain;
        }
        offset += band_size;
    }
}

fn apply_deep_filter(spec: &mut [Complex32], history: &[Vec<Complex32>], coefs: &[f32]) {
    let max_bins = DF_BINS.min(spec.len());
    if coefs.len() < max_bins * DF_ORDER * 2 {
        return;
    }

    for bin in 0..max_bins {
        let mut enhanced = Complex32::ZERO;
        for (order, frame) in history.iter().enumerate().take(DF_ORDER) {
            let coef_index = (bin * DF_ORDER + order) * 2;
            let coef = Complex32::new(coefs[coef_index], coefs[coef_index + 1]);
            enhanced += frame[bin] * coef;
        }
        spec[bin] = enhanced;
    }
}
