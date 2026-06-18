use rustfft::{num_complex::Complex32, Fft, FftPlanner};
use std::sync::Arc;

pub const DPDFNET_DSP_CHANNELS: u32 = 1;
pub const DPDFNET_DSP_SAMPLE_RATE_HZ: u32 = 48_000;

pub fn vorbis_window(window_len: usize) -> Vec<f32> {
    let half = window_len as f32 / 2.0;
    (0..window_len)
        .map(|index| {
            let sin = (0.5 * std::f32::consts::PI * (index as f32 + 0.5) / half).sin();
            (0.5 * std::f32::consts::PI * sin * sin).sin()
        })
        .collect()
}

pub struct DpdfNetDsp {
    window_len: usize,
    hop_size: usize,
    window: Vec<f32>,
    input_buffer: Vec<f32>,
    output_buffer: Vec<f32>,
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
}

impl DpdfNetDsp {
    pub fn new(window_len: usize, hop_size: usize) -> Option<Self> {
        if window_len == 0 || hop_size == 0 || hop_size > window_len {
            return None;
        }
        let mut planner = FftPlanner::new();
        let window = vorbis_window(window_len);
        Some(Self {
            window_len,
            hop_size,
            window,
            input_buffer: vec![0.0; window_len],
            output_buffer: vec![0.0; window_len],
            fft: planner.plan_fft_forward(window_len),
            ifft: planner.plan_fft_inverse(window_len),
        })
    }

    pub fn process_stft(&mut self, input: &[f32], spec_output: &mut [f32]) -> bool {
        let expected_spec_len = (self.window_len / 2 + 1) * 2;
        if input.len() != self.hop_size || spec_output.len() != expected_spec_len {
            return false;
        }
        self.input_buffer.copy_within(self.hop_size.., 0);
        self.input_buffer[self.window_len - self.hop_size..].copy_from_slice(input);
        let mut spectrum = self
            .input_buffer
            .iter()
            .zip(self.window.iter())
            .map(|(sample, window)| Complex32::new(sample * window, 0.0))
            .collect::<Vec<_>>();
        self.fft.process(&mut spectrum);
        for (index, bin) in spectrum.iter().take(self.window_len / 2 + 1).enumerate() {
            spec_output[index * 2] = bin.re;
            spec_output[index * 2 + 1] = bin.im;
        }
        true
    }

    pub fn process_istft(&mut self, spec: &[f32], output: &mut [f32]) -> bool {
        let expected_spec_len = (self.window_len / 2 + 1) * 2;
        if output.len() != self.hop_size || spec.len() != expected_spec_len {
            return false;
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

        self.output_buffer.copy_within(self.hop_size.., 0);
        self.output_buffer[self.window_len - self.hop_size..].fill(0.0);
        for (dst, sample) in self.output_buffer.iter_mut().zip(frame.iter()) {
            *dst += sample;
        }
        output.copy_from_slice(&self.output_buffer[..self.hop_size]);
        true
    }
}
