use crate::{
    invalid_deepfilternet_runtime_config, NoiseCancellationConfig, NoiseCancellationError,
    NoiseCanceller, NoiseFrame, NoiseFrameOutput, NoiseProvider,
};
use df::DFState;

pub const DEEPFILTERNET_SAMPLE_RATE_HZ: u32 = 48_000;
pub const DEEPFILTERNET_CHANNELS: u16 = 1;
pub const DEEPFILTERNET_FRAME_SIZE: usize = 480;
pub const DEEPFILTERNET_DEFAULT_FFT_SIZE: usize = 960;
pub const DEEPFILTERNET_DEFAULT_ERB_BANDS: usize = 32;
pub const DEEPFILTERNET_DEFAULT_MIN_ERB_FREQS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeepFilterNetRuntimeConfig {
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub fft_size: usize,
    pub hop_size: usize,
    pub erb_bands: usize,
    pub min_erb_freqs: usize,
}

impl Default for DeepFilterNetRuntimeConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: DEEPFILTERNET_SAMPLE_RATE_HZ,
            channels: DEEPFILTERNET_CHANNELS,
            fft_size: DEEPFILTERNET_DEFAULT_FFT_SIZE,
            hop_size: DEEPFILTERNET_FRAME_SIZE,
            erb_bands: DEEPFILTERNET_DEFAULT_ERB_BANDS,
            min_erb_freqs: DEEPFILTERNET_DEFAULT_MIN_ERB_FREQS,
        }
    }
}

impl DeepFilterNetRuntimeConfig {
    pub fn validate(self) -> Result<Self, NoiseCancellationError> {
        if self.sample_rate_hz != DEEPFILTERNET_SAMPLE_RATE_HZ {
            return Err(invalid_deepfilternet_runtime_config(format!(
                "sample_rate_hz must be {DEEPFILTERNET_SAMPLE_RATE_HZ}, got {}",
                self.sample_rate_hz
            )));
        }
        if self.channels != DEEPFILTERNET_CHANNELS {
            return Err(invalid_deepfilternet_runtime_config(format!(
                "channels must be {DEEPFILTERNET_CHANNELS}, got {}",
                self.channels
            )));
        }
        if self.fft_size == 0 {
            return Err(invalid_deepfilternet_runtime_config(
                "fft_size must be greater than zero",
            ));
        }
        if self.hop_size == 0 {
            return Err(invalid_deepfilternet_runtime_config(
                "hop_size must be greater than zero",
            ));
        }
        if self.hop_size.saturating_mul(2) > self.fft_size {
            return Err(invalid_deepfilternet_runtime_config(format!(
                "hop_size * 2 must be <= fft_size, got hop_size {} and fft_size {}",
                self.hop_size, self.fft_size
            )));
        }
        if self.erb_bands == 0 {
            return Err(invalid_deepfilternet_runtime_config(
                "erb_bands must be greater than zero",
            ));
        }
        if self.min_erb_freqs == 0 {
            return Err(invalid_deepfilternet_runtime_config(
                "min_erb_freqs must be greater than zero",
            ));
        }

        let freq_size = self.fft_size / 2 + 1;
        if self.erb_bands > freq_size {
            return Err(invalid_deepfilternet_runtime_config(format!(
                "erb_bands must be <= fft_size / 2 + 1 ({freq_size}), got {}",
                self.erb_bands
            )));
        }
        if self.min_erb_freqs > freq_size {
            return Err(invalid_deepfilternet_runtime_config(format!(
                "min_erb_freqs must be <= fft_size / 2 + 1 ({freq_size}), got {}",
                self.min_erb_freqs
            )));
        }
        if self
            .erb_bands
            .checked_mul(self.min_erb_freqs)
            .is_none_or(|minimum_bins| minimum_bins > freq_size)
        {
            return Err(invalid_deepfilternet_runtime_config(format!(
                "erb_bands * min_erb_freqs must fit fft_size / 2 + 1 ({freq_size}), got {} * {}",
                self.erb_bands, self.min_erb_freqs
            )));
        }

        let erb = df::erb_fb(
            self.sample_rate_hz as usize,
            self.fft_size,
            self.erb_bands,
            self.min_erb_freqs,
        );
        if erb.len() != self.erb_bands || erb.contains(&0) || erb.iter().sum::<usize>() != freq_size
        {
            return Err(invalid_deepfilternet_runtime_config(
                "ERB filter bank does not match configured FFT frequency bins",
            ));
        }
        Ok(self)
    }
}

pub struct DeepFilterNetNoiseCanceller {
    config: NoiseCancellationConfig,
    runtime: DeepFilterNetRuntimeConfig,
    state: DFState,
    delayed_samples: Vec<f32>,
}

impl DeepFilterNetNoiseCanceller {
    pub fn new(
        config: NoiseCancellationConfig,
        runtime: DeepFilterNetRuntimeConfig,
    ) -> Result<Self, NoiseCancellationError> {
        let runtime = runtime.validate()?;
        Ok(Self {
            config,
            runtime,
            state: DFState::new(
                runtime.sample_rate_hz as usize,
                runtime.fft_size,
                runtime.hop_size,
                runtime.erb_bands,
                runtime.min_erb_freqs,
            ),
            delayed_samples: Vec::new(),
        })
    }

    pub fn config(&self) -> &NoiseCancellationConfig {
        &self.config
    }

    pub fn runtime(&self) -> DeepFilterNetRuntimeConfig {
        self.runtime
    }
}

impl NoiseCanceller for DeepFilterNetNoiseCanceller {
    fn process_frame(
        &mut self,
        frame: NoiseFrame<'_>,
    ) -> Result<NoiseFrameOutput, NoiseCancellationError> {
        validate_deepfilternet_frame(self.runtime, frame)?;

        let mut reconstructed = Vec::with_capacity(frame.samples.len() + self.runtime.hop_size);
        for chunk in frame.samples.chunks_exact(self.runtime.hop_size) {
            let mut output = vec![0.0; self.runtime.hop_size];
            self.state.process_frame(chunk, &mut output);
            reconstructed.extend(output);
        }
        let silence = vec![0.0; self.runtime.hop_size];
        let mut delayed = vec![0.0; self.runtime.hop_size];
        self.state.process_frame(&silence, &mut delayed);
        reconstructed.extend(delayed);

        let mut samples = Vec::with_capacity(frame.samples.len());
        samples.append(&mut self.delayed_samples);
        samples.extend(reconstructed);
        self.delayed_samples = samples.split_off(frame.samples.len());

        Ok(NoiseFrameOutput {
            samples,
            voice_activity_probability: None,
        })
    }
}

fn validate_deepfilternet_frame(
    runtime: DeepFilterNetRuntimeConfig,
    frame: NoiseFrame<'_>,
) -> Result<(), NoiseCancellationError> {
    if frame.sample_rate_hz == runtime.sample_rate_hz
        && frame.channels == runtime.channels
        && !frame.samples.is_empty()
        && frame.samples.len().is_multiple_of(runtime.hop_size)
    {
        return Ok(());
    }

    Err(NoiseCancellationError::InvalidFrameShape {
        provider: NoiseProvider::Deepfilternet,
        sample_rate_hz: frame.sample_rate_hz,
        channels: frame.channels,
        samples: frame.samples.len(),
        expected_sample_rate_hz: runtime.sample_rate_hz,
        expected_channels: runtime.channels,
        expected_samples: runtime.hop_size,
    })
}
