use super::{default_serve_args, EnvVarGuard, ENV_LOCK};
use crate::cli::serve::{parse_api_bind, BindConfigError};
use crate::cli::{Cli, Commands};
use clap::Parser;

#[test]
fn parses_default_serve_args() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _enable_prof = EnvVarGuard::remove("LYRE_ENABLE_PROF");
    let _embedded_turn = EnvVarGuard::remove("LYRE_EMBEDDED_TURN");
    let cli = Cli::try_parse_from(["lyre", "serve"]).unwrap();
    match cli.command {
        Commands::Serve(args) => {
            assert_eq!(args.host, "0.0.0.0");
            assert_eq!(args.port, 8080);
            assert!(args.ice_servers.is_empty());
            assert!(!args.embedded_turn);
            assert!(!args.enable_prof);
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn parses_custom_serve_args() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _embedded_turn = EnvVarGuard::remove("LYRE_EMBEDDED_TURN");
    let cli =
        Cli::try_parse_from(["lyre", "serve", "--host", "127.0.0.1", "--port", "9000"]).unwrap();
    match cli.command {
        Commands::Serve(args) => {
            assert_eq!(args.host, "127.0.0.1");
            assert_eq!(args.port, 9000);
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn parses_enable_prof_cli_arg() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _enable_prof = EnvVarGuard::remove("LYRE_ENABLE_PROF");
    let cli = Cli::try_parse_from(["lyre", "serve", "--enable-prof"]).unwrap();
    match cli.command {
        Commands::Serve(args) => assert!(args.enable_prof),
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn enable_prof_env_enables_profile_routes() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _enable_prof = EnvVarGuard::set("LYRE_ENABLE_PROF", "true");
    let cli = Cli::try_parse_from(["lyre", "serve"]).unwrap();
    match cli.command {
        Commands::Serve(args) => assert!(args.enable_prof),
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn serve_args_honor_api_bind_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    let args = default_serve_args();
    let _api_bind = EnvVarGuard::set("LYRE_API_BIND", "127.0.0.1:9001");

    assert_eq!(
        args.effective_bind().unwrap(),
        ("127.0.0.1".to_owned(), 9001)
    );
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
    let _guard = ENV_LOCK.lock().unwrap();
    let mut args = default_serve_args();
    args.host = "127.0.0.1".to_owned();
    args.port = 9000;
    let _api_bind = EnvVarGuard::set("LYRE_API_BIND", "bad-bind");

    assert_eq!(
        args.effective_bind(),
        Err(BindConfigError::InvalidFormat {
            value: "bad-bind".to_owned()
        })
    );
}
