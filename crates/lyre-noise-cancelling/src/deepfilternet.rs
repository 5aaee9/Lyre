use crate::{
    dsp::deepfilternet::{
        DeepFilterNetDsp, DEEPFILTERNET_DSP_CHANNELS, DEEPFILTERNET_DSP_DF_BINS,
        DEEPFILTERNET_DSP_DF_ORDER, DEEPFILTERNET_DSP_ERB_BANDS, DEEPFILTERNET_DSP_FRAME_SIZE,
        DEEPFILTERNET_DSP_SAMPLE_RATE_HZ,
    },
    model_file_unavailable, noise_runtime_error, NoiseCancellationConfig, NoiseCancellationError,
    NoiseCanceller, NoiseFrame, NoiseFrameOutput, NoiseProvider,
};
use ort::{inputs, session::Session, value::TensorRef};
use std::path::PathBuf;

mod session_io;

use session_io::{
    build_session, df_decoder_inputs, encoder_inputs, encoder_outputs, erb_decoder_inputs,
    first_output_name, tensor_output, DfDecoderInputs, EncoderInputs, EncoderOutputs,
    ErbDecoderInputs,
};

pub const DEEPFILTERNET_SAMPLE_RATE_HZ: u32 = DEEPFILTERNET_DSP_SAMPLE_RATE_HZ;
pub const DEEPFILTERNET_CHANNELS: u16 = DEEPFILTERNET_DSP_CHANNELS as u16;
pub const DEEPFILTERNET_FRAME_SIZE: usize = DEEPFILTERNET_DSP_FRAME_SIZE;
pub const DEEPFILTERNET_ERB_BANDS: usize = DEEPFILTERNET_DSP_ERB_BANDS;
pub const DEEPFILTERNET_DEFAULT_MODEL_DIR: &str = "deepfilternet/onnx";
pub const DEEPFILTERNET_DEFAULT_INTRA_THREADS: usize = 1;
pub const DEEPFILTERNET_DEFAULT_INTER_THREADS: usize = 1;

const ENC_MODEL: &str = "enc.onnx";
const ERB_DEC_MODEL: &str = "erb_dec.onnx";
const DF_DEC_MODEL: &str = "df_dec.onnx";
const DF_BINS: usize = DEEPFILTERNET_DSP_DF_BINS;

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
    dsp: DeepFilterNetDsp,
    encoder: Session,
    erb_decoder: Session,
    df_decoder: Session,
    encoder_inputs: EncoderInputs,
    encoder_outputs: EncoderOutputs,
    erb_decoder_inputs: ErbDecoderInputs,
    erb_decoder_output: String,
    df_decoder_inputs: DfDecoderInputs,
    df_decoder_output: String,
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
            dsp: DeepFilterNetDsp::new(),
            encoder,
            erb_decoder,
            df_decoder,
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
        let features = self.dsp.extract_features(chunk).ok_or_else(|| {
            noise_runtime_error(
                NoiseProvider::Deepfilternet,
                "DeepFilterNet feature extraction failed",
            )
        })?;

        let feat_erb_tensor = TensorRef::from_array_view((
            vec![1, 1, 1, DEEPFILTERNET_ERB_BANDS],
            features.feat_erb.as_slice(),
        ))
        .map_err(|error| noise_runtime_error(NoiseProvider::Deepfilternet, error))?;
        let feat_spec_tensor =
            TensorRef::from_array_view((vec![1, 2, 1, DF_BINS], features.feat_spec.as_slice()))
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

        let mut output = vec![0.0; DEEPFILTERNET_FRAME_SIZE];
        if !self.dsp.synthesize(&mask, &coefs, &mut output) {
            return Err(noise_runtime_error(
                NoiseProvider::Deepfilternet,
                format!(
                    "DeepFilterNet model output shape mismatch: mask {}, coefs {}, expected at least {} and {}",
                    mask.len(),
                    coefs.len(),
                    DEEPFILTERNET_ERB_BANDS,
                    DEEPFILTERNET_DSP_DF_BINS * DEEPFILTERNET_DSP_DF_ORDER * 2
                ),
            ));
        }
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
