use std::path::PathBuf;

use clap::{Parser, Subcommand};
use config::PreconfRpcConfig;
use eyre::Result;
use forward_service::{RpcForward, SharedState};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod common;
mod config;
mod constants;
mod forward_service;
mod lookahead;
mod preconf;
mod relay_client;
mod ssz;

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
    initialize_tracing_log();
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

pub fn initialize_tracing_log() {
    let level_env = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_owned());

    let filter = match level_env.parse::<EnvFilter>() {
        Ok(f) => f,
        Err(_) => {
            eprintln!("Invalid RUST_LOG value {}, defaulting to info", level_env);
            EnvFilter::new("info")
        }
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().compact().with_target(true).with_file(false))
        .try_init()
        .unwrap();
}
