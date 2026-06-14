use clap::{Args, Parser, Subcommand};
use lyre_core::{
    default_ice_servers, supported_noise_providers, IceServerConfig, TurnRestCredentialsConfig,
    DEFAULT_ROOM_ID,
};
use serde::Serialize;
use std::env;
use thiserror::Error;

#[derive(Debug, Parser)]
#[command(name = "lyre")]
#[command(about = "Lyre VOIP server")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Serve(ServeArgs),
    Config(ConfigCommand),
}

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
pub enum IceServerConfigError {
    #[error("ICE server entry must not be blank: `{value}`")]
    BlankEntry { value: String },
    #[error("ICE server entry contains a blank URL: `{value}`")]
    BlankUrl { value: String },
    #[error("ICE server entry has too many `|` separators: `{value}`")]
    TooManyFields { value: String },
    #[error("ICE server configuration must contain at least one server")]
    Empty,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TurnRestConfigError {
    #[error("TURN REST shared secret must not be blank")]
    BlankSecret,
    #[error("TURN REST identity must not be blank")]
    BlankIdentity,
}

fn parse_ice_server_entries(
    entries: &[String],
) -> Result<Vec<IceServerConfig>, IceServerConfigError> {
    let mut servers = Vec::with_capacity(entries.len());
    for entry in entries {
        servers.push(parse_ice_server_entry(entry)?);
    }
    if servers.is_empty() {
        return Err(IceServerConfigError::Empty);
    }
    Ok(servers)
}

fn parse_ice_server_entry(entry: &str) -> Result<IceServerConfig, IceServerConfigError> {
    if entry.trim().is_empty() {
        return Err(IceServerConfigError::BlankEntry {
            value: entry.to_owned(),
        });
    }
    let parts = entry.split('|').collect::<Vec<_>>();
    if parts.len() > 3 {
        return Err(IceServerConfigError::TooManyFields {
            value: entry.to_owned(),
        });
    }
    let urls = parts[0]
        .split(',')
        .map(str::trim)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if urls.iter().any(|url| url.is_empty()) {
        return Err(IceServerConfigError::BlankUrl {
            value: entry.to_owned(),
        });
    }
    Ok(IceServerConfig {
        urls,
        username: parts.get(1).map(|value| (*value).to_owned()),
        credential: parts.get(2).map(|value| (*value).to_owned()),
    })
}

#[derive(Debug, Args)]
pub struct ConfigCommand {
    #[command(subcommand)]
    pub command: ConfigSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum ConfigSubcommand {
    Print,
}

#[derive(Debug, Serialize)]
pub struct ConfigPrint {
    pub default_room_id: &'static str,
    pub noise_providers: Vec<lyre_core::NoiseCancellationConfig>,
    pub ice_servers: Vec<IceServerConfig>,
}

pub fn config_print() -> ConfigPrint {
    ConfigPrint {
        default_room_id: DEFAULT_ROOM_ID,
        noise_providers: supported_noise_providers(),
        ice_servers: default_ice_servers(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn default_serve_args() -> ServeArgs {
        ServeArgs {
            host: "0.0.0.0".to_owned(),
            port: 8080,
            ice_servers: Vec::new(),
            turn_rest_secret: None,
            turn_rest_ttl_seconds: 3600,
            turn_rest_identity: "lyre".to_owned(),
        }
    }

    #[test]
    fn parses_default_serve_args() {
        let cli = Cli::try_parse_from(["lyre", "serve"]).unwrap();
        match cli.command {
            Commands::Serve(args) => {
                assert_eq!(args.host, "0.0.0.0");
                assert_eq!(args.port, 8080);
                assert!(args.ice_servers.is_empty());
            }
            Commands::Config(_) => panic!("expected serve"),
        }
    }

    #[test]
    fn parses_custom_serve_args() {
        let cli = Cli::try_parse_from(["lyre", "serve", "--host", "127.0.0.1", "--port", "9000"])
            .unwrap();
        match cli.command {
            Commands::Serve(args) => {
                assert_eq!(args.host, "127.0.0.1");
                assert_eq!(args.port, 9000);
            }
            Commands::Config(_) => panic!("expected serve"),
        }
    }

    #[test]
    fn serve_args_honor_api_bind_env() {
        let args = default_serve_args();
        std::env::set_var("LYRE_API_BIND", "127.0.0.1:9001");

        assert_eq!(
            args.effective_bind().unwrap(),
            ("127.0.0.1".to_owned(), 9001)
        );

        std::env::remove_var("LYRE_API_BIND");
    }

    #[test]
    fn malformed_api_bind_env_is_error() {
        assert_eq!(
            parse_api_bind("127.0.0.1"),
            Err(BindConfigError::InvalidFormat {
                value: "127.0.0.1".to_owned()
            })
        );
        assert_eq!(
            parse_api_bind("127.0.0.1:not-a-port"),
            Err(BindConfigError::InvalidPort {
                value: "127.0.0.1:not-a-port".to_owned()
            })
        );
    }

    #[test]
    fn malformed_api_bind_env_is_error_with_explicit_host_port() {
        let mut args = default_serve_args();
        args.host = "127.0.0.1".to_owned();
        args.port = 9000;
        std::env::set_var("LYRE_API_BIND", "bad-bind");

        assert_eq!(
            args.effective_bind(),
            Err(BindConfigError::InvalidFormat {
                value: "bad-bind".to_owned()
            })
        );

        std::env::remove_var("LYRE_API_BIND");
    }

    #[test]
    fn config_print_has_defaults() {
        let value = serde_json::to_value(config_print()).unwrap();
        assert_eq!(value["default_room_id"], "DEFAULT");
        assert_eq!(value["noise_providers"].as_array().unwrap().len(), 3);
        assert_eq!(
            value["ice_servers"][0]["urls"][0],
            "stun:stun.l.google.com:19302"
        );
    }

    #[test]
    fn ice_servers_default_when_unconfigured() {
        std::env::remove_var("LYRE_ICE_SERVERS");
        let args = default_serve_args();

        assert_eq!(args.effective_ice_servers().unwrap(), default_ice_servers());
    }

    #[test]
    fn parses_cli_ice_servers_with_credentials() {
        let mut args = default_serve_args();
        args.ice_servers = vec![
            "stun:a.example:3478,stun:b.example:3478".to_owned(),
            "turn:turn.example:3478|user|pass".to_owned(),
        ];

        let servers = args.effective_ice_servers().unwrap();

        assert_eq!(
            servers[0].urls,
            ["stun:a.example:3478", "stun:b.example:3478"]
        );
        assert_eq!(servers[1].username.as_deref(), Some("user"));
        assert_eq!(servers[1].credential.as_deref(), Some("pass"));
    }

    #[test]
    fn env_ice_servers_are_semicolon_separated() {
        std::env::set_var(
            "LYRE_ICE_SERVERS",
            "stun:a.example:3478;turn:turn.example:3478||pass",
        );
        let args = default_serve_args();

        let servers = args.effective_ice_servers().unwrap();

        assert_eq!(servers.len(), 2);
        assert_eq!(servers[1].username.as_deref(), Some(""));
        assert_eq!(servers[1].credential.as_deref(), Some("pass"));
        std::env::remove_var("LYRE_ICE_SERVERS");
    }

    #[test]
    fn cli_ice_servers_take_precedence_over_env() {
        std::env::set_var("LYRE_ICE_SERVERS", "stun:env.example:3478");
        let mut args = default_serve_args();
        args.ice_servers = vec!["stun:cli.example:3478".to_owned()];

        let servers = args.effective_ice_servers().unwrap();

        assert_eq!(servers[0].urls, ["stun:cli.example:3478"]);
        std::env::remove_var("LYRE_ICE_SERVERS");
    }

    #[test]
    fn parses_turn_rest_cli_args() {
        let cli = Cli::try_parse_from([
            "lyre",
            "serve",
            "--turn-rest-secret",
            "secret",
            "--turn-rest-ttl-seconds",
            "600",
            "--turn-rest-identity",
            "room-a",
        ])
        .unwrap();
        match cli.command {
            Commands::Serve(args) => {
                let config = args.effective_turn_rest_credentials().unwrap().unwrap();
                assert_eq!(config.secret, "secret");
                assert_eq!(config.ttl_seconds, 600);
                assert_eq!(config.identity, "room-a");
            }
            Commands::Config(_) => panic!("expected serve"),
        }
    }

    #[test]
    fn turn_rest_secret_env_enables_default_ttl_and_identity() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("LYRE_TURN_REST_SECRET", "secret");
        let cli = Cli::try_parse_from(["lyre", "serve"]).unwrap();
        match cli.command {
            Commands::Serve(args) => {
                let config = args.effective_turn_rest_credentials().unwrap().unwrap();
                assert_eq!(config.ttl_seconds, 3600);
                assert_eq!(config.identity, "lyre");
            }
            Commands::Config(_) => panic!("expected serve"),
        }
        std::env::remove_var("LYRE_TURN_REST_SECRET");
    }

    #[test]
    fn rejects_blank_turn_rest_cli_secret() {
        let cli = Cli::try_parse_from(["lyre", "serve", "--turn-rest-secret", " "]).unwrap();
        match cli.command {
            Commands::Serve(args) => {
                assert_eq!(
                    args.effective_turn_rest_credentials(),
                    Err(TurnRestConfigError::BlankSecret)
                );
            }
            Commands::Config(_) => panic!("expected serve"),
        }
    }

    #[test]
    fn rejects_blank_turn_rest_env_secret() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("LYRE_TURN_REST_SECRET", " ");
        let cli = Cli::try_parse_from(["lyre", "serve"]).unwrap();
        match cli.command {
            Commands::Serve(args) => {
                assert_eq!(
                    args.effective_turn_rest_credentials(),
                    Err(TurnRestConfigError::BlankSecret)
                );
            }
            Commands::Config(_) => panic!("expected serve"),
        }
        std::env::remove_var("LYRE_TURN_REST_SECRET");
    }

    #[test]
    fn rejects_invalid_ice_server_entries() {
        assert_eq!(
            parse_ice_server_entry(" "),
            Err(IceServerConfigError::BlankEntry {
                value: " ".to_owned()
            })
        );
        assert_eq!(
            parse_ice_server_entry("stun:a.example,"),
            Err(IceServerConfigError::BlankUrl {
                value: "stun:a.example,".to_owned()
            })
        );
        assert_eq!(
            parse_ice_server_entry("turn:x|u|p|extra"),
            Err(IceServerConfigError::TooManyFields {
                value: "turn:x|u|p|extra".to_owned()
            })
        );
    }
}
