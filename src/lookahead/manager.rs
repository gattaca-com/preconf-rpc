use std::str::FromStr;

use alloy::rpc::types::beacon::{events::HeadEvent, BlsPublicKey};
use dashmap::DashMap;
use eyre::{bail, ContextCompat, Result};
use hashbrown::HashMap;
use tokio::sync::broadcast;
use url::Url;

use super::{
    provider::LookaheadProvider, Lookahead, LookaheadEntry, LookaheadProviderOptions,
    RelayLookaheadProvider,
};
use crate::config::Config;

#[derive(Debug)]
/// Manages the state of the lookahead provider.
enum LookaheadProviderManager {
    Initialized(LookaheadProvider),
    Running,
}

#[derive(Debug, Clone)]
pub enum UrlProvider {
    LookaheadEntry,
    UrlMap(HashMap<BlsPublicKey, Url>),
}

#[derive(Debug)]
/// Manages the lookahead for preconfer elections.
pub struct LookaheadManager {
    lookahead: Lookahead,
    provider_manager: Option<LookaheadProviderManager>,
    url_provider: UrlProvider,
}

impl Default for LookaheadManager {
    fn default() -> Self {
        Self {
            lookahead: Lookahead { map: DashMap::new().into() },
            provider_manager: Some(LookaheadProviderManager::Initialized(LookaheadProvider::None)),
            url_provider: UrlProvider::LookaheadEntry,
        }
    }
}

impl LookaheadManager {
    pub fn new(
        lookahead: Lookahead,
        lookahead_provider: LookaheadProvider,
        url_provider: UrlProvider,
    ) -> Self {
        Self {
            lookahead,
            provider_manager: Some(LookaheadProviderManager::Initialized(lookahead_provider)),
            url_provider,
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

    fn get_next_elected_preconfer(&self) -> Option<LookaheadEntry> {
        self.lookahead.get_next_elected_preconfer()
    }

    pub fn get_url(&self) -> Result<Url> {
        match self.get_next_elected_preconfer() {
            None => bail!("no lookahead provider found"),
            Some(entry) => match &self.url_provider {
                UrlProvider::LookaheadEntry => {
                    Ok(Url::from_str(&entry.url).expect("not a valid url"))
                }
                UrlProvider::UrlMap(m) => {
                    let pub_key = entry.election.preconfer_pubkey();
                    m.get(&pub_key)
                        .cloned()
                        .wrap_err(format!("could not find key for pubkey {}", pub_key))
                }
            },
        }
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
                r_c.relays,
                HashMap::new(),
            )),
        }
        .build_relay_provider();
        let url_provider = match r_c.url_provider {
            crate::config::UrlProvider::Lookahead => UrlProvider::LookaheadEntry,
            crate::config::UrlProvider::Registry => {
                UrlProvider::UrlMap(r_c.registry.expect("registry is empty"))
            }
        };
        map.insert(r_c.chain_id, LookaheadManager::new(lookahead, provider, url_provider));
    }
    map
}
