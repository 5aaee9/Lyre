use super::{EnvVarGuard, ENV_LOCK};
use crate::cli::serve::StateFileConfigError;
use crate::cli::{Cli, Commands};
use clap::Parser;
use std::path::PathBuf;

#[test]
fn parses_state_file_cli_arg() {
    let cli =
        Cli::try_parse_from(["lyre", "serve", "--state-file", "/tmp/lyre-state.json"]).unwrap();
    match cli.command {
        Commands::Serve(args) => {
            assert_eq!(
                args.effective_state_file().unwrap().unwrap(),
                PathBuf::from("/tmp/lyre-state.json")
            );
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn state_file_env_enables_persistence() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _state_file = EnvVarGuard::set("LYRE_STATE_FILE", "/tmp/lyre-env-state.json");
    let cli = Cli::try_parse_from(["lyre", "serve"]).unwrap();
    match cli.command {
        Commands::Serve(args) => {
            assert_eq!(
                args.effective_state_file().unwrap().unwrap(),
                PathBuf::from("/tmp/lyre-env-state.json")
            );
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn state_file_cli_takes_precedence_over_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _state_file = EnvVarGuard::set("LYRE_STATE_FILE", "/tmp/lyre-env-state.json");
    let cli =
        Cli::try_parse_from(["lyre", "serve", "--state-file", "/tmp/lyre-cli-state.json"]).unwrap();
    match cli.command {
        Commands::Serve(args) => {
            assert_eq!(
                args.effective_state_file().unwrap().unwrap(),
                PathBuf::from("/tmp/lyre-cli-state.json")
            );
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn rejects_blank_state_file_path() {
    let cli = Cli::try_parse_from(["lyre", "serve", "--state-file", " "]).unwrap();
    match cli.command {
        Commands::Serve(args) => {
            assert_eq!(
                args.effective_state_file(),
                Err(StateFileConfigError::BlankPath)
            );
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}
