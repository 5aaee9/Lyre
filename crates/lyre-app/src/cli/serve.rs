use super::ice::{parse_ice_server_entries, IceServerConfigError};
use clap::Args;
use lyre_core::{default_ice_servers, IceServerConfig, TurnRestCredentialsConfig};
use lyre_noise_cancelling::DeepFilterNetRuntimeConfig;
use std::{env, path::PathBuf};
use thiserror::Error;

#[derive(Debug, Args)]
pub struct ServeArgs {
    #[arg(long, default_value = "0.0.0.0", env = "LYRE_HOST")]
    pub host: String,
    #[arg(long, default_value_t = 8080, env = "LYRE_PORT")]
    pub port: u16,
    #[arg(long = "ice-server")]
    pub ice_servers: Vec<String>,
    #[arg(long, env = "LYRE_TURN_REST_SECRET")]
    pub turn_rest_secret: Option<String>,
    #[arg(long, default_value_t = 3600, env = "LYRE_TURN_REST_TTL_SECONDS")]
    pub turn_rest_ttl_seconds: u64,
    #[arg(long, default_value = "lyre", env = "LYRE_TURN_REST_IDENTITY")]
    pub turn_rest_identity: String,
    #[arg(long, default_value_t = false, env = "LYRE_EMBEDDED_TURN")]
    pub embedded_turn: bool,
    #[arg(
        long,
        default_value = "0.0.0.0:3478",
        env = "LYRE_EMBEDDED_TURN_LISTEN"
    )]
    pub embedded_turn_listen: String,
    #[arg(
        long,
        default_value = "127.0.0.1:3478",
        env = "LYRE_EMBEDDED_TURN_EXTERNAL"
    )]
    pub embedded_turn_external: String,
    #[arg(long, default_value = "lyre.local", env = "LYRE_EMBEDDED_TURN_REALM")]
    pub embedded_turn_realm: String,
    #[arg(
        long,
        default_value = "49152..65535",
        env = "LYRE_EMBEDDED_TURN_PORT_RANGE"
    )]
    pub embedded_turn_port_range: String,
    #[arg(long, env = "LYRE_STATE_FILE")]
    pub state_file: Option<String>,
    #[arg(
        long,
        default_value_t = super::deepfilternet::DEFAULT_DEEPFILTERNET_FFT_SIZE,
        env = "LYRE_DEEPFILTERNET_FFT_SIZE"
    )]
    pub deepfilternet_fft_size: usize,
    #[arg(
        long,
        default_value_t = super::deepfilternet::DEFAULT_DEEPFILTERNET_HOP_SIZE,
        env = "LYRE_DEEPFILTERNET_HOP_SIZE"
    )]
    pub deepfilternet_hop_size: usize,
    #[arg(
        long,
        default_value_t = super::deepfilternet::DEFAULT_DEEPFILTERNET_ERB_BANDS,
        env = "LYRE_DEEPFILTERNET_ERB_BANDS"
    )]
    pub deepfilternet_erb_bands: usize,
    #[arg(
        long,
        default_value_t = super::deepfilternet::DEFAULT_DEEPFILTERNET_MIN_ERB_FREQS,
        env = "LYRE_DEEPFILTERNET_MIN_ERB_FREQS"
    )]
    pub deepfilternet_min_erb_freqs: usize,
}

impl ServeArgs {
    pub fn effective_bind(&self) -> Result<(String, u16), BindConfigError> {
        let api_bind = env::var("LYRE_API_BIND")
            .ok()
            .map(|bind| parse_api_bind(&bind))
            .transpose()?;
        if self.host != "0.0.0.0" || self.port != 8080 {
            return Ok((self.host.clone(), self.port));
        }
        Ok(api_bind.unwrap_or_else(|| (self.host.clone(), self.port)))
    }

    pub fn effective_ice_servers(&self) -> Result<Vec<IceServerConfig>, IceServerConfigError> {
        if !self.ice_servers.is_empty() {
            return parse_ice_server_entries(&self.ice_servers);
        }
        if let Ok(raw) = env::var("LYRE_ICE_SERVERS") {
            let entries = raw.split(';').map(str::to_owned).collect::<Vec<_>>();
            return parse_ice_server_entries(&entries);
        }
        if self.embedded_turn {
            let external = self
                .embedded_turn_external
                .parse::<std::net::SocketAddr>()
                .map_err(|_| IceServerConfigError::InvalidEmbeddedTurnExternal {
                    value: self.embedded_turn_external.clone(),
                })?;
            return Ok(vec![IceServerConfig {
                urls: vec![format!("turn:{external}")],
                username: None,
                credential: None,
            }]);
        }
        Ok(default_ice_servers())
    }

    pub fn effective_turn_rest_credentials(
        &self,
    ) -> Result<Option<TurnRestCredentialsConfig>, TurnRestConfigError> {
        let secret = self
            .turn_rest_secret
            .clone()
            .or_else(|| env::var("LYRE_TURN_REST_SECRET").ok());
        let Some(secret) = secret else {
            return Ok(None);
        };
        if secret.trim().is_empty() {
            return Err(TurnRestConfigError::BlankSecret);
        }
        if self.turn_rest_identity.trim().is_empty() {
            return Err(TurnRestConfigError::BlankIdentity);
        }
        Ok(Some(TurnRestCredentialsConfig {
            secret: secret.clone(),
            ttl_seconds: self.turn_rest_ttl_seconds,
            identity: self.turn_rest_identity.trim().to_owned(),
        }))
    }

    pub fn effective_embedded_turn_config(
        &self,
    ) -> Result<Option<lyre_turn::EmbeddedTurnConfig>, TurnRestConfigError> {
        if !self.embedded_turn {
            return Ok(None);
        }
        let Some(turn_rest) = self.effective_turn_rest_credentials()? else {
            return Err(TurnRestConfigError::EmbeddedTurn(
                lyre_turn::EmbeddedTurnConfigError::MissingTurnRestSecret,
            ));
        };
        let listen = self.embedded_turn_listen.parse().map_err(|_| {
            TurnRestConfigError::InvalidEmbeddedTurnListen {
                value: self.embedded_turn_listen.clone(),
            }
        })?;
        let external = self.embedded_turn_external.parse().map_err(|_| {
            TurnRestConfigError::InvalidEmbeddedTurnExternal {
                value: self.embedded_turn_external.clone(),
            }
        })?;
        if self.embedded_turn_realm.trim().is_empty() {
            return Err(TurnRestConfigError::EmbeddedTurn(
                lyre_turn::EmbeddedTurnConfigError::BlankRealm,
            ));
        }
        let port_range = self.embedded_turn_port_range.parse()?;
        Ok(Some(lyre_turn::EmbeddedTurnConfig {
            listen,
            external,
            realm: self.embedded_turn_realm.trim().to_owned(),
            port_range,
            static_auth_secret: turn_rest.secret,
        }))
    }

    pub fn effective_state_file(&self) -> Result<Option<PathBuf>, StateFileConfigError> {
        let Some(path) = self
            .state_file
            .clone()
            .or_else(|| env::var("LYRE_STATE_FILE").ok())
        else {
            return Ok(None);
        };
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return Err(StateFileConfigError::BlankPath);
        }
        Ok(Some(PathBuf::from(trimmed)))
    }

    pub fn effective_deepfilternet_runtime(
        &self,
    ) -> Result<DeepFilterNetRuntimeConfig, super::deepfilternet::DeepFilterNetConfigError> {
        super::deepfilternet::validate_deepfilternet_runtime(DeepFilterNetRuntimeConfig {
            fft_size: self.deepfilternet_fft_size,
            hop_size: self.deepfilternet_hop_size,
            erb_bands: self.deepfilternet_erb_bands,
            min_erb_freqs: self.deepfilternet_min_erb_freqs,
            ..DeepFilterNetRuntimeConfig::default()
        })
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BindConfigError {
    #[error("LYRE_API_BIND must be formatted as host:port, got `{value}`")]
    InvalidFormat { value: String },
    #[error("LYRE_API_BIND port must be a valid u16, got `{value}`")]
    InvalidPort { value: String },
}

fn parse_api_bind(value: &str) -> Result<(String, u16), BindConfigError> {
    let Some((host, port)) = value.rsplit_once(':') else {
        return Err(BindConfigError::InvalidFormat {
            value: value.to_owned(),
        });
    };
    if host.is_empty() {
        return Err(BindConfigError::InvalidFormat {
            value: value.to_owned(),
        });
    }
    let port = port
        .parse::<u16>()
        .map_err(|_| BindConfigError::InvalidPort {
            value: value.to_owned(),
        })?;
    Ok((host.to_owned(), port))
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TurnRestConfigError {
    #[error("TURN REST shared secret must not be blank")]
    BlankSecret,
    #[error("TURN REST identity must not be blank")]
    BlankIdentity,
    #[error(transparent)]
    EmbeddedTurn(#[from] lyre_turn::EmbeddedTurnConfigError),
    #[error("embedded TURN listen address must be a valid socket address, got `{value}`")]
    InvalidEmbeddedTurnListen { value: String },
    #[error("embedded TURN external address must be an IP socket address, got `{value}`")]
    InvalidEmbeddedTurnExternal { value: String },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StateFileConfigError {
    #[error("state file path must not be blank")]
    BlankPath,
}

#[cfg(test)]
mod tests;
