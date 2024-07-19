use alloy::rpc::types::beacon::events::HeadEvent;
use dashmap::DashMap;
use eyre::{bail, Result};
use hashbrown::HashMap;
use tokio::sync::broadcast;

use super::{
    provider::LookaheadProvider, Lookahead, LookaheadEntry, LookaheadProviderOptions,
    RelayLookaheadProvider,
};
use crate::config::Config;

/// Manages the state of the lookahead provider.
enum LookaheadProviderManager {
    Initialized(LookaheadProvider),
    Running,
}

/// Manages the lookahead for preconfer elections.
pub struct LookaheadManager {
    lookahead: Lookahead,
    provider_manager: Option<LookaheadProviderManager>,
}

impl LookaheadManager {
    pub fn new(lookahead: Lookahead, lookahead_provider: LookaheadProvider) -> Self {
        Self {
            lookahead,
            provider_manager: Some(LookaheadProviderManager::Initialized(lookahead_provider)),
        }
    }

    /// Runs the lookahead provider in a separate thread.
    /// It returns an error if the provider is already running.
    pub fn run_provider(&mut self) -> Result<()> {
        let provider_manager =
            self.provider_manager.take().expect("provider manager should never be None");
        match provider_manager {
            LookaheadProviderManager::Initialized(provider) => {
                let _handle = tokio::spawn(async move {
                    provider.run().await;
                });
                self.provider_manager = Some(LookaheadProviderManager::Running);
                Ok(())
            }
            _ => bail!("context provider is already running."),
        }
    }

    pub fn get_next_elected_preconfer(&self) -> Option<LookaheadEntry> {
        self.lookahead.get_next_elected_preconfer()
    }
}

/// BBuilds a map of lookahead managers from the configuration, keyed by the chain-id.
pub fn lookahead_managers_from_config(
    config: Config,
    beacon_tx: broadcast::Sender<HeadEvent>,
) -> HashMap<u16, LookaheadManager> {
    // build managers from relay lookahead providers
    let mut map = HashMap::new();
    for r_c in config.lookahead_providers_relays {
        let lookahead = Lookahead { map: DashMap::new().into() };
        let provider = LookaheadProviderOptions {
            head_event_receiver: Some(beacon_tx.subscribe()),
            relay_provider: Some(RelayLookaheadProvider::new(
                lookahead.clone(),
                r_c.relay_urls,
                HashMap::new(),
            )),
        }
        .build_relay_provider();
        map.insert(r_c.chain_id, LookaheadManager::new(lookahead, provider));
    }
    map
}
