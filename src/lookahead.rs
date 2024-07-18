use std::sync::Arc;

use alloy::rpc::types::beacon::{events::HeadEvent, BlsPublicKey};
use dashmap::DashMap;
use futures_util::future::join_all;
use hashbrown::HashMap;
use tokio::sync::broadcast;
use tracing::{debug, info};

use crate::{
    constants::EPOCH_SLOTS,
    preconf::election::SignedPreconferElection,
    relay_client::{RelayClient, RelayClientConfig},
};

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

#[derive(Debug, Clone, Default)]
pub struct Lookahead(Arc<DashMap<u64, LookaheadEntry>>);

impl Lookahead {
    pub fn new() -> Self {
        Self(DashMap::new().into())
    }

    /// Returns the next preconfer. If there is no preconfer elected for the current slot,
    /// it will return the next known election. Or None, if there are no elected preconfers in
    /// the next epoch.
    /// Any elected preconfers older than `head_slot` will have been cleared so, we fetch this by
    /// getting the preconfer with the lowest slot number.
    pub fn get_next_elected_preconfer(&self) -> Option<LookaheadEntry> {
        self.0.iter().min_by_key(|entry| entry.slot()).map(|entry| entry.value().clone())
    }
}

struct LookaheadContext {
    /// Current slot of the `LookaheadProvider`
    head_slot: u64,
    /// Latest epoch of lookaheads that have been set.
    /// This ensures we only set the lookahead once per epoch.
    curr_lookahead_epoch: u64,
}

/// The lookahead provider keeps track of the lookahead, i.e. the slot -> preconfer map.
/// It builds this progressively by querying relays for preconfers for a given slot.
/// Preconf lookahead is guaranteed at epoch time. So we fetch for epoch + 1 at slot > 1 in the
/// current epoch.
pub struct LookaheadProvider {
    /// Maps a slot to the elected preconfer for that slot.
    lookahead: Lookahead,
    /// Maps a preconfer pubkey to known url.
    preconfer_registry: HashMap<BlsPublicKey, String>,
    /// List of relay URLs that support the constraints API. Preconfers will be fetched
    /// from these relays.
    relays: Vec<RelayClient>,
    context: LookaheadContext,
}

impl LookaheadProvider {
    /// Creates a new `LookaheadProvider` with the given relays.
    pub fn new(
        lookahead: Lookahead,
        relay_urls: Vec<String>,
        preconfer_registry: HashMap<BlsPublicKey, String>,
    ) -> Self {
        let relays = relay_urls
            .into_iter()
            .map(|url| {
                // TODO: read from config
                let config = RelayClientConfig::new(url, true);
                RelayClient::new(config.into())
            })
            .collect();

        Self {
            lookahead,
            preconfer_registry,
            relays,
            context: LookaheadContext { head_slot: 0, curr_lookahead_epoch: 0 },
        }
    }

    /// Runs indefinitely, subscribes to new head events.
    /// At set times, determines which preconfers have been elected for each slot in the next epoch.
    pub async fn run(mut self, mut head_event_rx: broadcast::Receiver<HeadEvent>) {
        while let Ok(head_event) = head_event_rx.recv().await {
            self.on_new_head_event(head_event).await;
        }
    }

    /// Updates the local context's slot and cleans up any out-of-date entries in the lookahead.
    /// If the slot meets the right conditions, it will fetch the lookahead for a new epoch.
    async fn on_new_head_event(&mut self, head_event: HeadEvent) {
        let curr_epoch = head_event.slot / EPOCH_SLOTS;
        let head_slot = head_event.slot;
        info!(target: "lookahead", head_slot, curr_epoch, "received new head event");

        if head_slot <= self.head_slot() {
            return;
        }
        self.set_head_slot(head_slot);

        // Clear lookahead of old slots.
        self.lookahead.0.retain(|slot, _| *slot >= head_slot);

        // Only query each epoch once.
        // if self.curr_lookahead_epoch() > curr_epoch {
        //     return;
        // }

        // Make sure we are at least 20 slots in. Often when querying duties on the epoch boundary
        // the values are incorrect, so waiting an extra slot fixes this.
        if self.head_slot() % 6 == 0 {
            let curr_epoch_start_slot = curr_epoch * EPOCH_SLOTS;
            info!(target: "lookahead", head_slot, curr_epoch_start_slot, "fetching preconfer lookahead");

            // Fetch and update the lookahead
            self.fetch_preconfer_lookahead(curr_epoch + 1).await;
        }
    }

    /// For a given epoch, fetch the elected preconfers from all relays and add results
    /// to the lookahead.
    ///
    /// Sets the `context.curr_lookahead_epoch` to `epoch` at the end.
    async fn fetch_preconfer_lookahead(&mut self, epoch: u64) {
        let epoch_start_slot = epoch * EPOCH_SLOTS;
        info!(target: "lookahead", %epoch, %epoch_start_slot, "fetching preconfer elections for epoch");

        let mut lookahead_handles = Vec::with_capacity(self.relays.len());
        for relay in self.relays.iter() {
            lookahead_handles.push(relay.get_elected_preconfers_for_epoch(epoch));
        }

        for result in join_all(lookahead_handles).await {
            match result {
                Ok(Some(preconfer_elections)) => {
                    for election in preconfer_elections {
                        self.add_elected_preconfer_to_lookahead(election).await;
                    }
                }
                Ok(None) => {
                    debug!(target: "lookahead", epoch, "no elected preconfers found");
                }
                Err(error) => {
                    debug!(?error, "failed to fetch elected preconfer");
                }
            }
        }

        self.set_curr_lookahead_epoch(epoch);
    }

    /// Adds a new election to our lookahead. Will overwrite any existing elected preconfer for that
    /// slot.
    async fn add_elected_preconfer_to_lookahead(&mut self, election: SignedPreconferElection) {
        let preconfer_url =
            self.preconfer_registry.get(&election.preconfer_pubkey()).cloned().unwrap_or_default();

        let election_slot = election.slot();
        debug!(
            target: "lookahead",
            %election_slot,
            preconf_public_key = ?election.preconfer_pubkey(),
            preconfer_url,
            "preconfer election added to lookahead",
        );

        let entry = LookaheadEntry { url: preconfer_url, election };
        self.lookahead.0.insert(election_slot, entry);
    }

    /// Returns the current head slot.
    fn head_slot(&self) -> u64 {
        self.context.head_slot
    }

    /// Sets the current head slot.
    fn set_head_slot(&mut self, slot: u64) {
        self.context.head_slot = slot;
    }

    // /// Returns the current lookahead epoch.
    // fn curr_lookahead_epoch(&self) -> u64 {
    //     self.context.curr_lookahead_epoch
    // }

    /// Sets the current lookahead epoch.
    fn set_curr_lookahead_epoch(&mut self, epoch: u64) {
        self.context.curr_lookahead_epoch = epoch;
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{common::client::MultiBeaconClient, initialize_tracing_log};

    #[tokio::test]
    async fn test_lookahead() {
        std::env::set_var("RUST_LOG", "lookahead=trace");

        initialize_tracing_log();

        let beacons = vec!["https://bn.bootnode.helder-devnets.xyz/".into()];

        let (beacon_tx, beacon_rx) = broadcast::channel(16);
        let client = MultiBeaconClient::from_endpoint_strs(&beacons);
        client.subscribe_to_head_events(beacon_tx.clone()).await;

        let lookahead = Lookahead::new();
        let relays = vec!["http://18.192.244.122:4040".into()];

        let provider = LookaheadProvider::new(lookahead, relays, HashMap::new());

        provider.run(beacon_rx).await;
    }
}