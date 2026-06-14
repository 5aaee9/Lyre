use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoiseProvider {
    Off,
    Rnnoise,
    Deepfilternet,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoiseCancellationConfig {
    pub provider: NoiseProvider,
    pub intensity: f32,
    pub voice_activity_threshold: f32,
}

impl Default for NoiseCancellationConfig {
    fn default() -> Self {
        Self {
            provider: NoiseProvider::Off,
            intensity: 0.5,
            voice_activity_threshold: 0.35,
        }
    }
}

pub fn supported_noise_providers() -> Vec<NoiseCancellationConfig> {
    vec![
        NoiseCancellationConfig {
            provider: NoiseProvider::Off,
            ..NoiseCancellationConfig::default()
        },
        NoiseCancellationConfig {
            provider: NoiseProvider::Rnnoise,
            ..NoiseCancellationConfig::default()
        },
        NoiseCancellationConfig {
            provider: NoiseProvider::Deepfilternet,
            ..NoiseCancellationConfig::default()
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_names_are_lowercase_for_json() {
        assert_eq!(
            serde_json::to_string(&NoiseProvider::Off).unwrap(),
            "\"off\""
        );
        assert_eq!(
            serde_json::to_string(&NoiseProvider::Rnnoise).unwrap(),
            "\"rnnoise\""
        );
        assert_eq!(
            serde_json::to_string(&NoiseProvider::Deepfilternet).unwrap(),
            "\"deepfilternet\""
        );
    }
}
