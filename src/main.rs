use std::path::PathBuf;

use clap::{Parser, Subcommand};
use common::client::MultiBeaconClient;
use dashmap::DashMap;
use eyre::Result;
use forward_service::{RpcForward, SharedState};
use hashbrown::HashMap;
use lookahead::{Lookahead, LookaheadManager, LookaheadProviderOptions, RelayLookaheadProvider};
use tokio::sync::broadcast;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::{config::Config, lookahead::lookahead_managers_from_config};

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
    /// configuration file containing lookahead providers configuration.
    #[clap(short, long)]
    config: PathBuf,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// execute the forward service
    Forward {
        #[clap(short, long, value_delimiter = ' ', num_args = 1..)]
        beacon_urls: Vec<String>,
        #[clap(short, long)]
        port: Option<u16>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    initialize_tracing_log();
    let cli = Cli::parse();
    match &cli.command {
        Commands::Forward { beacon_urls, port } => {
            let config = Config::from_file(&cli.config)?;
            let (beacon_tx, beacon_rx) = broadcast::channel(16);
            let client = MultiBeaconClient::from_endpoint_strs(&beacon_urls);
            client.subscribe_to_head_events(beacon_tx.clone()).await;
            let listening_addr = format!("0.0.0.0:{}", port.unwrap_or(8000));

            let managers = lookahead_managers_from_config(config, beacon_tx);
            let join_handle = RpcForward::new(SharedState::new(managers)?, listening_addr)
                .start_service()
                .await?;
            join_handle.await??;
        }
    }
    Ok(())
}

fn initialize_tracing_log() {
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
