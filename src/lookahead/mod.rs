use std::sync::Arc;

use dashmap::DashMap;

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
}

#[derive(Debug, Clone)]
pub enum Lookahead {
    Multi(Arc<DashMap<u64, LookaheadEntry>>),
}

impl Lookahead {
    pub fn clear_slots(&mut self, head_slot: u64) {
        match self {
            Lookahead::Multi(m) => m.retain(|slot, _| *slot >= head_slot),
        }
    }
    pub fn insert(&mut self, election_slot: u64, slot: LookaheadEntry) {
        match self {
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
            Lookahead::Multi(m) => {
                m.iter().min_by_key(|entry| entry.slot()).map(|entry| entry.value().clone())
            }
        }
    }
}