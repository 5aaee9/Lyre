use super::{default_serve_args, EnvVarGuard, ENV_LOCK};
use crate::cli::{Cli, Commands};
use clap::Parser;

#[test]
fn parses_cli_cors_allowed_origins() {
    let cli = Cli::try_parse_from([
        "lyre",
        "serve",
        "--cors-allowed-origin",
        "https://app.example.test",
        "--cors-allowed-origin",
        "https://admin.example.test",
    ])
    .unwrap();

    match cli.command {
        Commands::Serve(args) => assert_eq!(
            args.cors_allowed_origins,
            ["https://app.example.test", "https://admin.example.test"]
        ),
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn env_cors_allowed_origins_are_semicolon_separated() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _cors_allowed_origins = EnvVarGuard::set(
        "LYRE_CORS_ALLOWED_ORIGINS",
        "https://app.example.test; https://admin.example.test ",
    );
    let args = default_serve_args();

    assert_eq!(
        args.effective_cors_allowed_origins(),
        ["https://app.example.test", "https://admin.example.test"]
    );
}

#[test]
fn cli_cors_allowed_origins_take_precedence_over_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _cors_allowed_origins =
        EnvVarGuard::set("LYRE_CORS_ALLOWED_ORIGINS", "https://env.example.test");
    let mut args = default_serve_args();
    args.cors_allowed_origins = vec!["https://cli.example.test".to_owned()];

    assert_eq!(
        args.effective_cors_allowed_origins(),
        ["https://cli.example.test"]
    );
}
