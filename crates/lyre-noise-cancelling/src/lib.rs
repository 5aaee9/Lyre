pub use lyre_core::{NoiseCancellationConfig, NoiseProvider};

pub trait NoiseCanceller {
    fn process_frame(&self, samples: &[f32]) -> Vec<f32>;
}

#[derive(Debug, Clone)]
pub struct PassthroughNoiseCanceller {
    config: NoiseCancellationConfig,
}

impl PassthroughNoiseCanceller {
    pub fn new(config: NoiseCancellationConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &NoiseCancellationConfig {
        &self.config
    }
}

impl Default for PassthroughNoiseCanceller {
    fn default() -> Self {
        Self::new(NoiseCancellationConfig::default())
    }
}

impl NoiseCanceller for PassthroughNoiseCanceller {
    fn process_frame(&self, samples: &[f32]) -> Vec<f32> {
        samples.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_off() {
        let canceller = PassthroughNoiseCanceller::default();
        assert_eq!(canceller.config().provider, NoiseProvider::Off);
    }

    #[test]
    fn passthrough_returns_samples_unchanged() {
        let canceller = PassthroughNoiseCanceller::default();
        let samples = [-0.2, 0.0, 0.3];

        assert_eq!(canceller.process_frame(&samples), samples);
    }
}
