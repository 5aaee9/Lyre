use super::{EnvVarGuard, ENV_LOCK};
use crate::cli::serve::TurnRestConfigError;
use crate::cli::{Cli, Commands};
use clap::Parser;

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
    let _turn_rest_secret = EnvVarGuard::set("LYRE_TURN_REST_SECRET", "secret");
    let cli = Cli::try_parse_from(["lyre", "serve"]).unwrap();
    match cli.command {
        Commands::Serve(args) => {
            let config = args.effective_turn_rest_credentials().unwrap().unwrap();
            assert_eq!(config.ttl_seconds, 3600);
            assert_eq!(config.identity, "lyre");
        }
        Commands::Config(_) => panic!("expected serve"),
    }
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
    let _turn_rest_secret = EnvVarGuard::set("LYRE_TURN_REST_SECRET", " ");
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
}

#[test]
fn parses_default_embedded_turn_config() {
    let cli = Cli::try_parse_from([
        "lyre",
        "serve",
        "--embedded-turn",
        "--turn-rest-secret",
        "secret",
    ])
    .unwrap();
    match cli.command {
        Commands::Serve(args) => {
            let config = args.effective_embedded_turn_config().unwrap().unwrap();
            assert_eq!(config.listen.to_string(), "0.0.0.0:3478");
            assert_eq!(config.external.to_string(), "127.0.0.1:3478");
            assert_eq!(config.realm, "lyre.local");
            assert_eq!(
                config.port_range,
                lyre_turn::EmbeddedTurnPortRange {
                    start: 49152,
                    end: 65535,
                }
            );
            assert_eq!(config.static_auth_secret, "secret");
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn rejects_embedded_turn_without_turn_rest_secret() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _turn_rest_secret = EnvVarGuard::remove("LYRE_TURN_REST_SECRET");
    let cli = Cli::try_parse_from(["lyre", "serve", "--embedded-turn"]).unwrap();
    match cli.command {
        Commands::Serve(args) => {
            assert_eq!(
                args.effective_embedded_turn_config(),
                Err(TurnRestConfigError::EmbeddedTurn(
                    lyre_turn::EmbeddedTurnConfigError::MissingTurnRestSecret
                ))
            );
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn parses_custom_embedded_turn_config() {
    let cli = Cli::try_parse_from([
        "lyre",
        "serve",
        "--embedded-turn",
        "--turn-rest-secret",
        "secret",
        "--embedded-turn-listen",
        "0.0.0.0:3479",
        "--embedded-turn-external",
        "203.0.113.10:3479",
        "--embedded-turn-realm",
        "turn.example",
        "--embedded-turn-port-range",
        "50000..50100",
    ])
    .unwrap();
    match cli.command {
        Commands::Serve(args) => {
            let config = args.effective_embedded_turn_config().unwrap().unwrap();
            assert_eq!(config.listen.to_string(), "0.0.0.0:3479");
            assert_eq!(config.external.to_string(), "203.0.113.10:3479");
            assert_eq!(config.realm, "turn.example");
            assert_eq!(
                config.port_range,
                lyre_turn::EmbeddedTurnPortRange {
                    start: 50000,
                    end: 50100,
                }
            );
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn embedded_turn_env_enables_default_config() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _embedded_turn = EnvVarGuard::set("LYRE_EMBEDDED_TURN", "true");
    let _turn_rest_secret = EnvVarGuard::set("LYRE_TURN_REST_SECRET", "secret");
    let cli = Cli::try_parse_from(["lyre", "serve"]).unwrap();
    match cli.command {
        Commands::Serve(args) => {
            let config = args.effective_embedded_turn_config().unwrap().unwrap();
            assert_eq!(config.external.to_string(), "127.0.0.1:3478");
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn rejects_embedded_turn_hostname_external() {
    let cli = Cli::try_parse_from([
        "lyre",
        "serve",
        "--embedded-turn",
        "--turn-rest-secret",
        "secret",
        "--embedded-turn-external",
        "turn.example.com:3478",
    ])
    .unwrap();
    match cli.command {
        Commands::Serve(args) => {
            assert_eq!(
                args.effective_embedded_turn_config(),
                Err(TurnRestConfigError::InvalidEmbeddedTurnExternal {
                    value: "turn.example.com:3478".to_owned()
                })
            );
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn rejects_invalid_embedded_turn_port_ranges() {
    for value in [
        "49152-65535",
        "49152..",
        "49151..65535",
        "60000..59999",
        "49152..70000",
    ] {
        let cli = Cli::try_parse_from([
            "lyre",
            "serve",
            "--embedded-turn",
            "--turn-rest-secret",
            "secret",
            "--embedded-turn-port-range",
            value,
        ])
        .unwrap();
        match cli.command {
            Commands::Serve(args) => {
                assert!(args.effective_embedded_turn_config().is_err());
            }
            Commands::Config(_) => panic!("expected serve"),
        }
    }
}
