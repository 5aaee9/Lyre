use clap::{Args, Parser, Subcommand};
use lyre_core::{supported_noise_providers, DEFAULT_ROOM_ID};
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
}

pub fn config_print() -> ConfigPrint {
    ConfigPrint {
        default_room_id: DEFAULT_ROOM_ID,
        noise_providers: supported_noise_providers(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_default_serve_args() {
        let cli = Cli::try_parse_from(["lyre", "serve"]).unwrap();
        match cli.command {
            Commands::Serve(args) => {
                assert_eq!(args.host, "0.0.0.0");
                assert_eq!(args.port, 8080);
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
        let args = ServeArgs {
            host: "0.0.0.0".to_owned(),
            port: 8080,
        };
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
        let args = ServeArgs {
            host: "127.0.0.1".to_owned(),
            port: 9000,
        };
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
    }
}
