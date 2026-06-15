use super::{EnvVarGuard, ENV_LOCK};
use crate::cli::{Cli, Commands};
use clap::Parser;

#[test]
fn parses_deepfilternet_runtime_cli_args() {
    let _guard = ENV_LOCK.lock().unwrap();
    let cli = Cli::try_parse_from([
        "lyre",
        "serve",
        "--deepfilternet-fft-size",
        "1920",
        "--deepfilternet-hop-size",
        "960",
        "--deepfilternet-erb-bands",
        "32",
        "--deepfilternet-min-erb-freqs",
        "2",
    ])
    .unwrap();

    match cli.command {
        Commands::Serve(args) => {
            let runtime = args.effective_deepfilternet_runtime().unwrap();
            assert_eq!(runtime.fft_size, 1920);
            assert_eq!(runtime.hop_size, 960);
            assert_eq!(runtime.erb_bands, 32);
            assert_eq!(runtime.min_erb_freqs, 2);
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn deepfilternet_runtime_env_enables_config() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _fft_size = EnvVarGuard::set("LYRE_DEEPFILTERNET_FFT_SIZE", "1920");
    let _hop_size = EnvVarGuard::set("LYRE_DEEPFILTERNET_HOP_SIZE", "960");

    let cli = Cli::try_parse_from(["lyre", "serve"]).unwrap();

    match cli.command {
        Commands::Serve(args) => {
            let runtime = args.effective_deepfilternet_runtime().unwrap();
            assert_eq!(runtime.fft_size, 1920);
            assert_eq!(runtime.hop_size, 960);
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn rejects_invalid_deepfilternet_runtime_config() {
    let _guard = ENV_LOCK.lock().unwrap();
    let cli = Cli::try_parse_from([
        "lyre",
        "serve",
        "--deepfilternet-fft-size",
        "480",
        "--deepfilternet-hop-size",
        "480",
    ])
    .unwrap();

    match cli.command {
        Commands::Serve(args) => {
            assert!(args.effective_deepfilternet_runtime().is_err());
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}
