use anyhow::{Context, Result};
use std::{net::SocketAddr, str::FromStr};
use thiserror::Error;
use turn_server::{
    config::{Auth, Config, Interface, Server},
    service::session::ports::PortRange,
};

const MIN_RELAY_PORT: u16 = 49152;
const MAX_RELAY_PORT: u16 = 65535;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedTurnConfig {
    pub listen: SocketAddr,
    pub external: SocketAddr,
    pub realm: String,
    pub port_range: EmbeddedTurnPortRange,
    pub static_auth_secret: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddedTurnPortRange {
    pub start: u16,
    pub end: u16,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EmbeddedTurnConfigError {
    #[error("embedded TURN requires a TURN REST shared secret")]
    MissingTurnRestSecret,
    #[error("embedded TURN realm must not be blank")]
    BlankRealm,
    #[error("embedded TURN port range must use <start>..<end>, got `{value}`")]
    InvalidPortRangeFormat { value: String },
    #[error("embedded TURN relay ports must be within 49152..65535, got `{value}`")]
    PortRangeOutsideRelayRange { value: String },
    #[error("embedded TURN relay port range start must be <= end, got `{value}`")]
    PortRangeStartAfterEnd { value: String },
}

impl Default for EmbeddedTurnPortRange {
    fn default() -> Self {
        Self {
            start: MIN_RELAY_PORT,
            end: MAX_RELAY_PORT,
        }
    }
}

impl FromStr for EmbeddedTurnPortRange {
    type Err = EmbeddedTurnConfigError;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        let Some((start, end)) = value.split_once("..") else {
            return Err(EmbeddedTurnConfigError::InvalidPortRangeFormat {
                value: value.to_owned(),
            });
        };
        if start.is_empty() || end.is_empty() {
            return Err(EmbeddedTurnConfigError::InvalidPortRangeFormat {
                value: value.to_owned(),
            });
        }
        let start =
            start
                .parse::<u16>()
                .map_err(|_| EmbeddedTurnConfigError::InvalidPortRangeFormat {
                    value: value.to_owned(),
                })?;
        let end =
            end.parse::<u16>()
                .map_err(|_| EmbeddedTurnConfigError::InvalidPortRangeFormat {
                    value: value.to_owned(),
                })?;
        if start < MIN_RELAY_PORT {
            return Err(EmbeddedTurnConfigError::PortRangeOutsideRelayRange {
                value: value.to_owned(),
            });
        }
        if start > end {
            return Err(EmbeddedTurnConfigError::PortRangeStartAfterEnd {
                value: value.to_owned(),
            });
        }
        Ok(Self { start, end })
    }
}

impl std::fmt::Display for EmbeddedTurnPortRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

impl EmbeddedTurnConfig {
    pub fn ice_server_url(&self) -> String {
        format!("turn:{}", self.external)
    }

    pub fn to_turn_server_config(&self) -> Config {
        Config {
            server: Server {
                realm: self.realm.clone(),
                interfaces: vec![Interface::Udp {
                    listen: self.listen,
                    external: self.external,
                    idle_timeout: 20,
                    mtu: 1500,
                }],
                port_range: PortRange::from_str(&self.port_range.to_string())
                    .expect("validated embedded TURN port range must parse"),
                max_threads: 1,
            },
            auth: Auth {
                static_auth_secret: Some(self.static_auth_secret.clone()),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

pub async fn run_embedded_turn(config: EmbeddedTurnConfig) -> Result<()> {
    let addr = config.listen;
    turn_server::start_server(config.to_turn_server_config())
        .await
        .with_context(|| format!("embedded TURN server failed at {addr}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> EmbeddedTurnConfig {
        EmbeddedTurnConfig {
            listen: "0.0.0.0:3478".parse().unwrap(),
            external: "127.0.0.1:3478".parse().unwrap(),
            realm: "lyre.local".to_owned(),
            port_range: EmbeddedTurnPortRange::default(),
            static_auth_secret: "secret".to_owned(),
        }
    }

    #[test]
    fn embedded_turn_defaults_generate_local_ice_url() {
        let config = config();
        assert_eq!(config.ice_server_url(), "turn:127.0.0.1:3478");
    }

    #[test]
    fn parses_valid_port_range() {
        assert_eq!(
            "50000..50100".parse::<EmbeddedTurnPortRange>().unwrap(),
            EmbeddedTurnPortRange {
                start: 50000,
                end: 50100
            }
        );
    }

    #[test]
    fn rejects_invalid_port_ranges() {
        assert_eq!(
            "49152-65535".parse::<EmbeddedTurnPortRange>(),
            Err(EmbeddedTurnConfigError::InvalidPortRangeFormat {
                value: "49152-65535".to_owned()
            })
        );
        assert_eq!(
            "49152..".parse::<EmbeddedTurnPortRange>(),
            Err(EmbeddedTurnConfigError::InvalidPortRangeFormat {
                value: "49152..".to_owned()
            })
        );
        assert_eq!(
            "49151..65535".parse::<EmbeddedTurnPortRange>(),
            Err(EmbeddedTurnConfigError::PortRangeOutsideRelayRange {
                value: "49151..65535".to_owned()
            })
        );
        assert_eq!(
            "60000..59999".parse::<EmbeddedTurnPortRange>(),
            Err(EmbeddedTurnConfigError::PortRangeStartAfterEnd {
                value: "60000..59999".to_owned()
            })
        );
        assert_eq!(
            "49152..70000".parse::<EmbeddedTurnPortRange>(),
            Err(EmbeddedTurnConfigError::InvalidPortRangeFormat {
                value: "49152..70000".to_owned()
            })
        );
    }

    #[test]
    fn converts_to_turn_server_config() {
        let turn_config = config().to_turn_server_config();
        assert_eq!(turn_config.server.realm, "lyre.local");
        assert_eq!(turn_config.server.port_range.start(), 49152);
        assert_eq!(turn_config.server.port_range.end(), 65535);
        assert_eq!(turn_config.server.interfaces.len(), 1);
        assert_eq!(
            turn_config.auth.static_auth_secret.as_deref(),
            Some("secret")
        );
    }
}
