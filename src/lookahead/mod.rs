use std::sync::Arc;

use alloy::rpc::types::beacon::BlsPublicKey;
use dashmap::DashMap;
use hashbrown::HashMap;
use tokio::sync::broadcast;

use crate::preconf::election::SignedPreconferElection;

mod manager;
mod provider;

pub use manager::*;
pub use provider::*;

/// Wraps a signed election and url.
#[derive(Debug, Clone, Default)]
pub struct LookaheadEntry {
    pub url: String,
    pub election: SignedPreconferElection,
}

impl LookaheadEntry {
    pub fn slot(&self) -> u64 {
        self.election.slot()
    }

    pub fn preconfer_pubkey(&self) -> BlsPublicKey {
        self.election.message.preconfer_pubkey
    }
}

#[derive(Debug, Clone)]
pub enum Lookahead {
    Single(Option<LookaheadEntry>),
    Multi(Arc<DashMap<u64, LookaheadEntry>>),
}

impl Lookahead {
    pub fn clear_slots(&mut self, head_slot: u64) {
        match self {
            Lookahead::Single(_) => (),
            Lookahead::Multi(m) => m.retain(|slot, _| *slot >= head_slot),
        }
    }
    pub fn insert(&mut self, election_slot: u64, slot: LookaheadEntry) {
        match self {
            Lookahead::Single(s) => *s = Some(slot),
            Lookahead::Multi(m) => {
                m.insert(election_slot, slot);
            }
        }
    }
    /// Returns the next preconfer. If there is no preconfer elected for the current slot,
    /// it will return the next known election. Or None, if there are no elected preconfers in
    /// the next epoch.
    /// Any elected preconfers older than `head_slot` will have been cleared so, we fetch this by
    /// getting the preconfer with the lowest slot number.
    pub fn get_next_elected_preconfer(&self) -> Option<LookaheadEntry> {
        match self {
            Lookahead::Single(s) => s.clone(),
            Lookahead::Multi(m) => {
                m.iter().min_by_key(|entry| entry.slot()).map(|entry| entry.value().clone())
            }
        }
    }
}

#[cfg(test)]
mod test {
    use provider::{LookaheadProviderOptions, RelayLookaheadProvider};

    use super::*;
    use crate::{common::client::MultiBeaconClient, initialize_tracing_log};

    #[ignore]
    #[tokio::test]
    async fn test_lookahead() {
        std::env::set_var("RUST_LOG", "lookahead=trace");

        initialize_tracing_log();

        let beacons = vec!["https://bn.bootnode.helder-devnets.xyz/".into()];

        let (beacon_tx, beacon_rx) = broadcast::channel(16);
        let client = MultiBeaconClient::from_endpoint_strs(&beacons);
        client.subscribe_to_head_events(beacon_tx.clone()).await;

        let lookahead = Lookahead::Multi(DashMap::new().into());
        let relays = vec!["http://18.192.244.122:4040".into()];
        let provider = LookaheadProviderOptions {
            head_event_receiver: Some(beacon_rx),
            relay_provider: Some(RelayLookaheadProvider::new(lookahead, relays, HashMap::new())),
        }
        .build_relay_provider();

        provider.run().await;
    }
}
