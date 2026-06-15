use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoiseProvider {
    Off,
    Rnnoise,
    Deepfilternet,
    Dpdfnet,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoiseCancellationConfig {
    pub provider: NoiseProvider,
    pub intensity: f32,
    pub voice_activity_threshold: f32,
    #[serde(default)]
    pub dpdfnet: DpdfNetConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DpdfNetConfig {
    pub model: String,
}

impl Default for DpdfNetConfig {
    fn default() -> Self {
        Self {
            model: "dpdfnet2_48khz_hr".to_owned(),
        }
    }
}

impl Default for NoiseCancellationConfig {
    fn default() -> Self {
        Self {
            provider: NoiseProvider::Off,
            intensity: 0.5,
            voice_activity_threshold: 0.35,
            dpdfnet: DpdfNetConfig::default(),
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
        NoiseCancellationConfig {
            provider: NoiseProvider::Dpdfnet,
            dpdfnet: DpdfNetConfig {
                model: "dpdfnet2_48khz_hr".to_owned(),
            },
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
        assert_eq!(
            serde_json::to_string(&NoiseProvider::Dpdfnet).unwrap(),
            "\"dpdfnet\""
        );
    }

    #[test]
    fn dpdfnet_config_serializes_provider_specific_model() {
        let json = serde_json::to_value(NoiseCancellationConfig {
            provider: NoiseProvider::Dpdfnet,
            intensity: 0.5,
            voice_activity_threshold: 0.35,
            dpdfnet: DpdfNetConfig {
                model: "dpdfnet8_48khz_hr".to_owned(),
            },
        })
        .unwrap();

        assert_eq!(json["provider"], "dpdfnet");
        assert_eq!(json["dpdfnet"]["model"], "dpdfnet8_48khz_hr");
    }
}
