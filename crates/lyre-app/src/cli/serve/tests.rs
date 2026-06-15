use super::ServeArgs;
use std::sync::Mutex;

mod bind;
mod cors;
mod deepfilternet;
mod ice;
mod server_media;
mod state;
mod turn;

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvVarGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn remove(key: &'static str) -> Self {
        let previous = std::env::var(key).ok();
        std::env::remove_var(key);
        Self { key, previous }
    }

    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

fn default_serve_args() -> ServeArgs {
    ServeArgs {
        host: "0.0.0.0".to_owned(),
        port: 8080,
        ice_servers: Vec::new(),
        cors_allowed_origins: Vec::new(),
        turn_rest_secret: None,
        turn_rest_ttl_seconds: 3600,
        turn_rest_identity: "lyre".to_owned(),
        embedded_turn: false,
        embedded_turn_listen: "0.0.0.0:3478".to_owned(),
        embedded_turn_external: "127.0.0.1:3478".to_owned(),
        embedded_turn_realm: "lyre.local".to_owned(),
        embedded_turn_port_range: "49152..65535".to_owned(),
        server_media_public_ip: None,
        state_file: None,
        deepfilternet_fft_size: 960,
        deepfilternet_hop_size: 480,
        deepfilternet_erb_bands: 32,
        deepfilternet_min_erb_freqs: 2,
    }
}
