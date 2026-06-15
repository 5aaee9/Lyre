use super::ice::{parse_ice_server_entries, IceServerConfigError};
use clap::Args;
use lyre_core::{default_ice_servers, IceServerConfig, TurnRestCredentialsConfig};
use lyre_noise_cancelling::DeepFilterNetRuntimeConfig;
use std::{env, net::IpAddr, path::PathBuf, str::FromStr};
use thiserror::Error;

#[derive(Debug, Args)]
pub struct ServeArgs {
    #[arg(
        long,
        default_value = "0.0.0.0",
        env = "LYRE_HOST",
        help = "Network host for the API listener"
    )]
    pub host: String,
    #[arg(
        long,
        default_value_t = 8080,
        env = "LYRE_PORT",
        help = "TCP port for the API listener"
    )]
    pub port: u16,
    #[arg(long = "ice-server", help = "ICE server exposed to WebRTC clients")]
    pub ice_servers: Vec<String>,
    #[arg(
        long = "cors-allowed-origin",
        help = "Browser origin allowed to call the API with CORS"
    )]
    pub cors_allowed_origins: Vec<String>,
    #[arg(
        long,
        env = "LYRE_TURN_REST_SECRET",
        help = "Shared secret for issuing TURN REST credentials"
    )]
    pub turn_rest_secret: Option<String>,
    #[arg(
        long,
        default_value_t = 3600,
        env = "LYRE_TURN_REST_TTL_SECONDS",
        help = "Seconds before generated TURN REST credentials expire"
    )]
    pub turn_rest_ttl_seconds: u64,
    #[arg(
        long,
        default_value = "lyre",
        env = "LYRE_TURN_REST_IDENTITY",
        help = "Identity prefix used when generating TURN REST credentials"
    )]
    pub turn_rest_identity: String,
    #[arg(
        long,
        default_value_t = false,
        env = "LYRE_EMBEDDED_TURN",
        help = "Run an embedded UDP TURN relay with the API server"
    )]
    pub embedded_turn: bool,
    #[arg(
        long,
        default_value = "0.0.0.0:3478",
        env = "LYRE_EMBEDDED_TURN_LISTEN",
        help = "Socket address where the embedded TURN relay listens"
    )]
    pub embedded_turn_listen: String,
    #[arg(
        long,
        default_value = "127.0.0.1:3478",
        env = "LYRE_EMBEDDED_TURN_EXTERNAL",
        help = "Public socket address advertised for the embedded TURN relay"
    )]
    pub embedded_turn_external: String,
    #[arg(
        long,
        default_value = "lyre.local",
        env = "LYRE_EMBEDDED_TURN_REALM",
        help = "Authentication realm for the embedded TURN relay"
    )]
    pub embedded_turn_realm: String,
    #[arg(
        long,
        default_value = "49152..65535",
        env = "LYRE_EMBEDDED_TURN_PORT_RANGE",
        help = "UDP relay port range for the embedded TURN relay"
    )]
    pub embedded_turn_port_range: String,
    #[arg(
        long,
        env = "LYRE_SERVER_MEDIA_PUBLIC_IP",
        help = "Public IP advertised in server-media WebRTC host ICE candidates"
    )]
    pub server_media_public_ip: Option<String>,
    #[arg(
        long,
        env = "LYRE_SERVER_MEDIA_PORT_RANGE",
        help = "UDP port range used by server-media WebRTC host candidates"
    )]
    pub server_media_port_range: Option<String>,
    #[arg(
        long,
        env = "LYRE_STATE_FILE",
        help = "JSON state file used to persist anonymous rooms and access tokens"
    )]
    pub state_file: Option<String>,
    #[arg(
        long,
        default_value_t = super::deepfilternet::DEFAULT_DEEPFILTERNET_FFT_SIZE,
        env = "LYRE_DEEPFILTERNET_FFT_SIZE",
        help = "DeepFilterNet FFT window size"
    )]
    pub deepfilternet_fft_size: usize,
    #[arg(
        long,
        default_value_t = super::deepfilternet::DEFAULT_DEEPFILTERNET_HOP_SIZE,
        env = "LYRE_DEEPFILTERNET_HOP_SIZE",
        help = "DeepFilterNet hop size"
    )]
    pub deepfilternet_hop_size: usize,
    #[arg(
        long,
        default_value_t = super::deepfilternet::DEFAULT_DEEPFILTERNET_ERB_BANDS,
        env = "LYRE_DEEPFILTERNET_ERB_BANDS",
        help = "DeepFilterNet ERB band count"
    )]
    pub deepfilternet_erb_bands: usize,
    #[arg(
        long,
        default_value_t = super::deepfilternet::DEFAULT_DEEPFILTERNET_MIN_ERB_FREQS,
        env = "LYRE_DEEPFILTERNET_MIN_ERB_FREQS",
        help = "DeepFilterNet minimum ERB frequency bin count"
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

    pub fn effective_cors_allowed_origins(&self) -> Vec<String> {
        if !self.cors_allowed_origins.is_empty() {
            return self
                .cors_allowed_origins
                .iter()
                .map(|origin| origin.trim().to_owned())
                .filter(|origin| !origin.is_empty())
                .collect();
        }
        env::var("LYRE_CORS_ALLOWED_ORIGINS")
            .ok()
            .map(|raw| {
                raw.split(';')
                    .map(str::trim)
                    .filter(|origin| !origin.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
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

    pub fn effective_server_media_public_ip(
        &self,
    ) -> Result<Option<IpAddr>, ServerMediaConfigError> {
        let Some(value) = self
            .server_media_public_ip
            .clone()
            .or_else(|| env::var("LYRE_SERVER_MEDIA_PUBLIC_IP").ok())
        else {
            return Ok(None);
        };
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        trimmed
            .parse()
            .map(Some)
            .map_err(|_| ServerMediaConfigError::InvalidPublicIp {
                value: value.to_owned(),
            })
    }

    pub fn effective_server_media_port_range(
        &self,
    ) -> Result<Option<ServerMediaPortRange>, ServerMediaConfigError> {
        let Some(value) = self
            .server_media_port_range
            .clone()
            .or_else(|| env::var("LYRE_SERVER_MEDIA_PORT_RANGE").ok())
        else {
            return Ok(None);
        };
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        trimmed.parse().map(Some)
    }

    pub fn effective_server_media_port_range_with_embedded_turn(
        &self,
        embedded_turn: Option<&lyre_turn::EmbeddedTurnConfig>,
    ) -> Result<Option<ServerMediaPortRange>, ServerMediaConfigError> {
        match self.effective_server_media_port_range()? {
            Some(range) => Ok(Some(range)),
            None => Ok(embedded_turn.map(|config| ServerMediaPortRange {
                start: config.port_range.start,
                end: config.port_range.end,
            })),
        }
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

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ServerMediaConfigError {
    #[error("server media public IP must be an IP address, got `{value}`")]
    InvalidPublicIp { value: String },
    #[error("server media port range must use <start>..<end>, got `{value}`")]
    InvalidPortRangeFormat { value: String },
    #[error("server media port range start must be <= end, got `{value}`")]
    PortRangeStartAfterEnd { value: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerMediaPortRange {
    pub start: u16,
    pub end: u16,
}

impl From<ServerMediaPortRange> for lyre_web::ServerMediaPortRange {
    fn from(range: ServerMediaPortRange) -> Self {
        Self {
            start: range.start,
            end: range.end,
        }
    }
}

impl FromStr for ServerMediaPortRange {
    type Err = ServerMediaConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some((start, end)) = value.split_once("..") else {
            return Err(ServerMediaConfigError::InvalidPortRangeFormat {
                value: value.to_owned(),
            });
        };
        if start.is_empty() || end.is_empty() {
            return Err(ServerMediaConfigError::InvalidPortRangeFormat {
                value: value.to_owned(),
            });
        }
        let start =
            start
                .parse::<u16>()
                .map_err(|_| ServerMediaConfigError::InvalidPortRangeFormat {
                    value: value.to_owned(),
                })?;
        let end =
            end.parse::<u16>()
                .map_err(|_| ServerMediaConfigError::InvalidPortRangeFormat {
                    value: value.to_owned(),
                })?;
        if start > end {
            return Err(ServerMediaConfigError::PortRangeStartAfterEnd {
                value: value.to_owned(),
            });
        }
        Ok(Self { start, end })
    }
}

#[cfg(test)]
mod tests;
