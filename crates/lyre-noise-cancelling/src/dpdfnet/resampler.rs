use crate::{noise_runtime_error, NoiseCancellationError, NoiseFrame, NoiseProvider};
use rubato::{
    audioadapter_buffers::direct::SequentialSliceOfVecs, Async, FixedAsync, Indexing, Resampler,
    SincInterpolationParameters, SincInterpolationType, WindowFunction,
};

pub(super) struct DpdfNetResampler {
    source_sample_rate_hz: u32,
    model_sample_rate_hz: u32,
    source_hop_size: usize,
    model_hop_size: usize,
    downsampler: Async<f32>,
    upsampler: Async<f32>,
}

impl DpdfNetResampler {
    pub(super) fn new(
        source_sample_rate_hz: u32,
        model_sample_rate_hz: u32,
    ) -> Result<Option<Self>, NoiseCancellationError> {
        if source_sample_rate_hz == model_sample_rate_hz {
            return Ok(None);
        }

        if source_sample_rate_hz != super::DPDFNET_SERVER_SAMPLE_RATE_HZ
            || model_sample_rate_hz != 16_000
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

    pub(super) fn downsample(
        &mut self,
        frame: NoiseFrame<'_>,
    ) -> Result<Vec<f32>, NoiseCancellationError> {
        validate_resampled_dpdfnet_frame(
            frame,
            self.source_sample_rate_hz,
            super::DPDFNET_CHANNELS,
            self.source_hop_size,
        )?;
        let mut output =
            resample_dpdfnet_samples(&mut self.downsampler, frame.samples, self.source_hop_size)?;
        normalize_resampled_len(&mut output, frame.samples.len() / 3);
        Ok(output)
    }

    pub(super) fn upsample(
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
        usize::from(super::DPDFNET_CHANNELS),
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
    let input = SequentialSliceOfVecs::new(
        &input_data,
        usize::from(super::DPDFNET_CHANNELS),
        samples.len(),
    )
    .map_err(|error| noise_runtime_error(NoiseProvider::Dpdfnet, error))?;
    let output_len = samples.len() / input_chunk_size * resampler.output_frames_max();
    let mut output_data = vec![vec![0.0; output_len]];
    let mut output = SequentialSliceOfVecs::new_mut(
        &mut output_data,
        usize::from(super::DPDFNET_CHANNELS),
        output_len,
    )
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
