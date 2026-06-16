use lyre_core::{default_ice_servers, supported_noise_providers, IceServerConfig, DEFAULT_ROOM_ID};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ConfigPrint {
    pub default_room_id: &'static str,
    pub noise_providers: Vec<lyre_core::NoiseCancellationConfig>,
    pub ice_servers: Vec<IceServerConfig>,
    pub deepfilternet_runtime: DeepFilterNetRuntimeConfigPrint,
    pub dpdfnet_runtime: DpdfNetRuntimeConfigPrint,
}

#[derive(Debug, Serialize)]
pub struct DeepFilterNetRuntimeConfigPrint {
    pub model_dir: String,
    pub intra_threads: usize,
    pub inter_threads: usize,
}

#[derive(Debug, Serialize)]
pub struct DpdfNetRuntimeConfigPrint {
    pub model_dir: String,
    pub intra_threads: usize,
    pub inter_threads: usize,
}

impl From<lyre_noise_cancelling::DeepFilterNetRuntimeConfig> for DeepFilterNetRuntimeConfigPrint {
    fn from(config: lyre_noise_cancelling::DeepFilterNetRuntimeConfig) -> Self {
        Self {
            model_dir: config.model_dir.display().to_string(),
            intra_threads: config.intra_threads,
            inter_threads: config.inter_threads,
        }
    }
}

pub fn config_print() -> ConfigPrint {
    ConfigPrint {
        default_room_id: DEFAULT_ROOM_ID,
        noise_providers: supported_noise_providers(),
        ice_servers: default_ice_servers(),
        deepfilternet_runtime: lyre_noise_cancelling::DeepFilterNetRuntimeConfig::default().into(),
        dpdfnet_runtime: DpdfNetRuntimeConfigPrint {
            model_dir: lyre_noise_cancelling::DPDFNET_DEFAULT_MODEL_DIR.to_owned(),
            intra_threads: lyre_noise_cancelling::dpdfnet_default_intra_threads(),
            inter_threads: lyre_noise_cancelling::DPDFNET_DEFAULT_INTER_THREADS,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_print_has_defaults() {
        let value = serde_json::to_value(config_print()).unwrap();
        assert_eq!(value["default_room_id"], "DEFAULT");
        assert_eq!(value["noise_providers"].as_array().unwrap().len(), 4);
        assert_eq!(
            value["ice_servers"][0]["urls"][0],
            "stun:stun.l.google.com:19302"
        );
        assert_eq!(
            value["deepfilternet_runtime"]["model_dir"],
            "deepfilternet/onnx"
        );
        assert_eq!(value["deepfilternet_runtime"]["intra_threads"], 1);
        assert_eq!(value["deepfilternet_runtime"]["inter_threads"], 1);
        assert_eq!(value["dpdfnet_runtime"]["model_dir"], "dpdfnet/onnx");
        assert!(value["dpdfnet_runtime"]["intra_threads"].as_u64().unwrap() >= 1);
        assert_eq!(value["dpdfnet_runtime"]["inter_threads"], 1);
    }
}
