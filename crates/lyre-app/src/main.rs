mod cli;

use anyhow::{Context, Result};
use clap::Parser;
use cli::{Cli, Commands, ConfigSubcommand};
use lyre_web::ServeConfig;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve(args) => {
            let args = *args;
            let (host, port) = args.effective_bind()?;
            let ice_servers = args.effective_ice_servers()?;
            let turn_rest_credentials = args.effective_turn_rest_credentials()?;
            let embedded_turn = args.effective_embedded_turn_config()?;
            let state_file = args.effective_state_file()?;
            let deepfilternet_runtime = args.effective_deepfilternet_runtime()?;
            let cors_allowed_origins = args.effective_cors_allowed_origins();
            lyre_web::serve(ServeConfig {
                host,
                port,
                ice_servers,
                turn_rest_credentials,
                embedded_turn,
                state_file,
                deepfilternet_runtime,
                cors_allowed_origins,
            })
            .await
            .context("failed to run Lyre server")?;
        }
        Commands::Config(config) => match config.command {
            ConfigSubcommand::Print => {
                let json = serde_json::to_string_pretty(&cli::config_print())
                    .context("failed to serialize Lyre config")?;
                println!("{json}");
            }
        },
    }

    Ok(())
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "lyre=info,tower_http=info".into());
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}
