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
    #[command(subcommand, help = "Lyre command to run")]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    #[command(about = "Run the Lyre HTTP and WebSocket API server")]
    Serve(Box<ServeArgs>),
    #[command(about = "Inspect Lyre runtime configuration")]
    Config(ConfigCommand),
}

#[derive(Debug, Args)]
pub struct ConfigCommand {
    #[command(subcommand, help = "Configuration command to run")]
    pub command: ConfigSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum ConfigSubcommand {
    #[command(about = "Print default room and provider configuration")]
    Print,
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::CommandFactory;

    #[test]
    fn help_text_describes_commands_and_options() {
        let help = Cli::command().render_long_help().to_string();

        assert!(help.contains("Run the Lyre HTTP and WebSocket API server"));
        assert!(help.contains("Inspect Lyre runtime configuration"));

        let serve_help = Cli::command()
            .find_subcommand_mut("serve")
            .expect("serve command exists")
            .render_long_help()
            .to_string();

        for expected in [
            "Network host for the API listener",
            "TCP port for the API listener",
            "ICE server exposed to WebRTC clients",
            "Browser origin allowed to call the API with CORS",
            "Enable API CPU profiling routes",
            "Shared secret for issuing TURN REST credentials",
            "Seconds before generated TURN REST credentials expire",
            "Identity prefix used when generating TURN REST credentials",
            "Run an embedded UDP TURN relay with the API server",
            "Socket address where the embedded TURN relay listens",
            "Public socket address advertised for the embedded TURN relay",
            "Authentication realm for the embedded TURN relay",
            "UDP relay port range for the embedded TURN relay",
            "JSON state file used to persist anonymous rooms and access tokens",
            "Directory containing DeepFilterNet3 enc.onnx, erb_dec.onnx, and df_dec.onnx",
            "ONNX Runtime intra-op threads for DeepFilterNet3",
            "ONNX Runtime inter-op threads for DeepFilterNet3",
        ] {
            assert!(serve_help.contains(expected), "{expected}");
        }

        let config_help = Cli::command()
            .find_subcommand_mut("config")
            .expect("config command exists")
            .render_long_help()
            .to_string();

        assert!(config_help.contains("Print default room and provider configuration"));

        let print_help = Cli::command()
            .find_subcommand_mut("config")
            .expect("config command exists")
            .find_subcommand_mut("print")
            .expect("print command exists")
            .render_long_help()
            .to_string();

        assert!(print_help.contains("Print default room and provider configuration"));
    }
}
