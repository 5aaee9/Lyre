use crate::{noise_runtime_error, NoiseCancellationError, NoiseProvider};
use ort::session::{builder::GraphOptimizationLevel, Session};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub(super) struct EncoderInputs {
    pub(super) feat_erb: String,
    pub(super) feat_spec: String,
}

#[derive(Debug, Clone)]
pub(super) struct EncoderOutputs {
    pub(super) e0: String,
    pub(super) e1: String,
    pub(super) e2: String,
    pub(super) e3: String,
    pub(super) emb: String,
    pub(super) c0: String,
}

#[derive(Debug, Clone)]
pub(super) struct ErbDecoderInputs {
    pub(super) emb: String,
    pub(super) e3: String,
    pub(super) e2: String,
    pub(super) e1: String,
    pub(super) e0: String,
}

#[derive(Debug, Clone)]
pub(super) struct DfDecoderInputs {
    pub(super) emb: String,
    pub(super) c0: String,
}

pub(super) fn build_session(
    runtime: &super::DeepFilterNetRuntimeConfig,
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

pub(super) fn encoder_inputs(session: &Session) -> Result<EncoderInputs, NoiseCancellationError> {
    Ok(EncoderInputs {
        feat_erb: input_name(session, 0, "DeepFilterNet encoder")?,
        feat_spec: input_name(session, 1, "DeepFilterNet encoder")?,
    })
}

pub(super) fn encoder_outputs(session: &Session) -> Result<EncoderOutputs, NoiseCancellationError> {
    Ok(EncoderOutputs {
        e0: output_name(session, 0, "DeepFilterNet encoder")?,
        e1: output_name(session, 1, "DeepFilterNet encoder")?,
        e2: output_name(session, 2, "DeepFilterNet encoder")?,
        e3: output_name(session, 3, "DeepFilterNet encoder")?,
        emb: output_name(session, 4, "DeepFilterNet encoder")?,
        c0: output_name(session, 5, "DeepFilterNet encoder")?,
    })
}

pub(super) fn erb_decoder_inputs(
    session: &Session,
) -> Result<ErbDecoderInputs, NoiseCancellationError> {
    Ok(ErbDecoderInputs {
        emb: input_name(session, 0, "DeepFilterNet ERB decoder")?,
        e3: input_name(session, 1, "DeepFilterNet ERB decoder")?,
        e2: input_name(session, 2, "DeepFilterNet ERB decoder")?,
        e1: input_name(session, 3, "DeepFilterNet ERB decoder")?,
        e0: input_name(session, 4, "DeepFilterNet ERB decoder")?,
    })
}

pub(super) fn df_decoder_inputs(
    session: &Session,
) -> Result<DfDecoderInputs, NoiseCancellationError> {
    Ok(DfDecoderInputs {
        emb: input_name(session, 0, "DeepFilterNet DF decoder")?,
        c0: input_name(session, 1, "DeepFilterNet DF decoder")?,
    })
}

pub(super) fn first_output_name(
    session: &Session,
    model: &str,
) -> Result<String, NoiseCancellationError> {
    output_name(session, 0, model)
}

pub(super) fn tensor_output(
    outputs: &ort::session::SessionOutputs<'_>,
    name: &str,
) -> Result<Vec<f32>, NoiseCancellationError> {
    let (_, values) = outputs[name]
        .try_extract_tensor::<f32>()
        .map_err(|error| noise_runtime_error(NoiseProvider::Deepfilternet, error))?;
    Ok(values.to_vec())
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
