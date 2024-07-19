use clap::{Parser, Subcommand};
use common::client::MultiBeaconClient;
use dashmap::DashMap;
use eyre::Result;
use forward_service::{RpcForward, SharedState};
use hashbrown::HashMap;
use lookahead::{Lookahead, LookaheadProvider};
use tokio::sync::broadcast;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod common;
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
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// execute the forward service
    Forward {
        #[clap(short, long, value_delimiter = ' ', num_args = 1..)]
        relay_urls: Vec<String>,
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
        Commands::Forward { relay_urls, beacon_urls, port } => {
            let (beacon_tx, beacon_rx) = broadcast::channel(16);
            let client = MultiBeaconClient::from_endpoint_strs(&beacon_urls);
            client.subscribe_to_head_events(beacon_tx.clone()).await;

            let listening_addr = format!("0.0.0.0:{}", port.unwrap_or(8000));
            let lookahead = Lookahead::Multi(DashMap::new().into());
            let lookahead_provider =
                LookaheadProvider::new(lookahead.clone(), relay_urls.clone(), HashMap::new());
            let join_handle_provider = tokio::spawn(async move {
                lookahead_provider.run(beacon_rx).await;
            });
            let join_handle = RpcForward::new(SharedState::new(lookahead), listening_addr)
                .start_service()
                .await?;
            tokio::select! {
                _ = join_handle_provider => {
                    panic!("service to fetch next preconfer stopped.")
                }
                _ = join_handle => {
                    panic!("forward service stopped.")
                }
            }
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
