use crate::{noise_runtime_error, NoiseCancellationError, NoiseProvider};
use rustfft::{num_complex::Complex32, Fft, FftPlanner};
use std::sync::Arc;

pub(super) fn vorbis_window(window_len: usize) -> Vec<f32> {
    let half = window_len as f32 / 2.0;
    (0..window_len)
        .map(|index| {
            let sin = (0.5 * std::f32::consts::PI * (index as f32 + 0.5) / half).sin();
            (0.5 * std::f32::consts::PI * sin * sin).sin()
        })
        .collect()
}

pub(super) struct StftStreamingPreprocess {
    window_len: usize,
    hop_size: usize,
    window: Vec<f32>,
    buffer: Vec<f32>,
    fft: Arc<dyn Fft<f32>>,
}

impl StftStreamingPreprocess {
    pub(super) fn new(window_len: usize, hop_size: usize, window: Vec<f32>) -> Self {
        let mut planner = FftPlanner::new();
        Self {
            window_len,
            hop_size,
            window,
            buffer: vec![0.0; window_len],
            fft: planner.plan_fft_forward(window_len),
        }
    }

    pub(super) fn process(&mut self, input: &[f32]) -> Vec<f32> {
        self.buffer.copy_within(self.hop_size.., 0);
        self.buffer[self.window_len - self.hop_size..].copy_from_slice(input);
        let mut spectrum = self
            .buffer
            .iter()
            .zip(self.window.iter())
            .map(|(sample, window)| Complex32::new(sample * window, 0.0))
            .collect::<Vec<_>>();
        self.fft.process(&mut spectrum);

        let mut output = Vec::with_capacity((self.window_len / 2 + 1) * 2);
        for bin in spectrum.iter().take(self.window_len / 2 + 1) {
            output.push(bin.re);
            output.push(bin.im);
        }
        output
    }
}

pub(super) struct IstftStreamingPostprocess {
    window_len: usize,
    hop_size: usize,
    window: Vec<f32>,
    buffer: Vec<f32>,
    ifft: Arc<dyn Fft<f32>>,
}

impl IstftStreamingPostprocess {
    pub(super) fn new(window_len: usize, hop_size: usize, window: Vec<f32>) -> Self {
        let mut planner = FftPlanner::new();
        Self {
            window_len,
            hop_size,
            window,
            buffer: vec![0.0; window_len],
            ifft: planner.plan_fft_inverse(window_len),
        }
    }

    pub(super) fn process(&mut self, spec: &[f32]) -> Result<Vec<f32>, NoiseCancellationError> {
        let expected = (self.window_len / 2 + 1) * 2;
        if spec.len() != expected {
            return Err(noise_runtime_error(
                NoiseProvider::Dpdfnet,
                format!(
                    "DPDFNet output spec has {} values, expected {expected}",
                    spec.len()
                ),
            ));
        }

        let mut spectrum = vec![Complex32::ZERO; self.window_len];
        for (index, pair) in spec.chunks_exact(2).enumerate() {
            spectrum[index] = Complex32::new(pair[0], pair[1]);
        }
        for index in 1..self.window_len / 2 {
            spectrum[self.window_len - index] = spectrum[index].conj();
        }
        self.ifft.process(&mut spectrum);
        let scale = self.window_len as f32;
        let frame = spectrum
            .iter()
            .zip(self.window.iter())
            .map(|(sample, window)| sample.re / scale * window)
            .collect::<Vec<_>>();

        self.buffer.copy_within(self.hop_size.., 0);
        self.buffer[self.window_len - self.hop_size..].fill(0.0);
        for (dst, sample) in self.buffer.iter_mut().zip(frame.iter()) {
            *dst += sample;
        }
        Ok(self.buffer[..self.hop_size].to_vec())
    }
}
