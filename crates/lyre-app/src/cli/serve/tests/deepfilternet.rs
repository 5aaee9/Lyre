use super::{EnvVarGuard, ENV_LOCK};
use crate::cli::{Cli, Commands};
use clap::Parser;

#[test]
fn parses_deepfilternet_model_runtime_cli_args() {
    let _guard = ENV_LOCK.lock().unwrap();
    let cli = Cli::try_parse_from([
        "lyre",
        "serve",
        "--deepfilternet-model-dir",
        "/models/deepfilternet",
        "--deepfilternet-intra-threads",
        "4",
        "--deepfilternet-inter-threads",
        "2",
    ])
    .unwrap();

    match cli.command {
        Commands::Serve(args) => {
            let runtime = args.effective_deepfilternet_runtime().unwrap();
            assert_eq!(
                runtime.model_dir,
                std::path::PathBuf::from("/models/deepfilternet")
            );
            assert_eq!(runtime.intra_threads, 4);
            assert_eq!(runtime.inter_threads, 2);
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn deepfilternet_runtime_env_enables_config() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _model_dir = EnvVarGuard::set("LYRE_DEEPFILTERNET_MODEL_DIR", "/env/deepfilternet");
    let _intra_threads = EnvVarGuard::set("LYRE_DEEPFILTERNET_INTRA_THREADS", "3");
    let _inter_threads = EnvVarGuard::set("LYRE_DEEPFILTERNET_INTER_THREADS", "2");

    let cli = Cli::try_parse_from(["lyre", "serve"]).unwrap();

    match cli.command {
        Commands::Serve(args) => {
            let runtime = args.effective_deepfilternet_runtime().unwrap();
            assert_eq!(
                runtime.model_dir,
                std::path::PathBuf::from("/env/deepfilternet")
            );
            assert_eq!(runtime.intra_threads, 3);
            assert_eq!(runtime.inter_threads, 2);
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn rejects_invalid_deepfilternet_runtime_config() {
    let _guard = ENV_LOCK.lock().unwrap();
    let cli = Cli::try_parse_from(["lyre", "serve", "--deepfilternet-intra-threads", "0"]).unwrap();

    match cli.command {
        Commands::Serve(args) => {
            assert!(args.effective_deepfilternet_runtime().is_err());
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn parses_dpdfnet_model_dir_cli_arg() {
    let _guard = ENV_LOCK.lock().unwrap();
    let cli =
        Cli::try_parse_from(["lyre", "serve", "--dpdfnet-model-dir", "/models/dpdfnet"]).unwrap();

    match cli.command {
        Commands::Serve(args) => {
            let runtime = args.effective_noise_model_runtime().unwrap();
            assert_eq!(
                runtime.dpdfnet.model_dir,
                std::path::PathBuf::from("/models/dpdfnet")
            );
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn parses_dpdfnet_thread_cli_args() {
    let _guard = ENV_LOCK.lock().unwrap();
    let cli = Cli::try_parse_from([
        "lyre",
        "serve",
        "--dpdfnet-intra-threads",
        "4",
        "--dpdfnet-inter-threads",
        "2",
    ])
    .unwrap();

    match cli.command {
        Commands::Serve(args) => {
            let runtime = args.effective_noise_model_runtime().unwrap();
            assert_eq!(runtime.dpdfnet.intra_threads, 4);
            assert_eq!(runtime.dpdfnet.inter_threads, 2);
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn dpdfnet_thread_env_enables_config() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _intra_threads = EnvVarGuard::set("LYRE_DPDFNET_INTRA_THREADS", "3");
    let _inter_threads = EnvVarGuard::set("LYRE_DPDFNET_INTER_THREADS", "2");

    let cli = Cli::try_parse_from(["lyre", "serve"]).unwrap();

    match cli.command {
        Commands::Serve(args) => {
            let runtime = args.effective_noise_model_runtime().unwrap();
            assert_eq!(runtime.dpdfnet.intra_threads, 3);
            assert_eq!(runtime.dpdfnet.inter_threads, 2);
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}

#[test]
fn dpdfnet_intra_threads_defaults_to_low_cpu_single_thread() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _intra_threads = EnvVarGuard::remove("LYRE_DPDFNET_INTRA_THREADS");

    let cli = Cli::try_parse_from(["lyre", "serve"]).unwrap();

    match cli.command {
        Commands::Serve(args) => {
            let runtime = args.effective_noise_model_runtime().unwrap();
            assert_eq!(
                runtime.dpdfnet.intra_threads,
                lyre_noise_cancelling::DPDFNET_DEFAULT_INTRA_THREADS
            );
            assert!(lyre_noise_cancelling::dpdfnet_available_parallelism() >= 1);
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}
