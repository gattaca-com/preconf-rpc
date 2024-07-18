use std::path::PathBuf;

use clap::{Parser, Subcommand};
use config::PreconfRpcConfig;
use eyre::Result;
use forward_service::{RpcForward, SharedState};

mod config;
mod forward_service;

#[derive(Debug, Parser)]
#[command(name = "preconf-rpc")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// configuration file path.
    #[arg(short, long)]
    config: PathBuf,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// execute the forward service
    Forward,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config: PreconfRpcConfig = cli.config.into();
    match &cli.command {
        Commands::Forward => {
            let join_handle = RpcForward::new(
                SharedState::new(config.redirection_urls),
                config.forward.listening_addr(),
            )
            .start_service()
            .await?;
            join_handle.await??;
        }
    }
    Ok(())
}
