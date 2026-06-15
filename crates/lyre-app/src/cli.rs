mod config;
mod deepfilternet;
mod ice;
mod serve;

use clap::{Args, Parser, Subcommand};

pub use config::config_print;
pub use serve::ServeArgs;

#[derive(Debug, Parser)]
#[command(name = "lyre")]
#[command(about = "Lyre VOIP server")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Serve(Box<ServeArgs>),
    Config(ConfigCommand),
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
