use alloy::rpc::types::beacon::{BlsPublicKey, BlsSignature};
use serde::{Deserialize, Serialize};
use tree_hash_derive::TreeHash;

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignedPreconferElection {
    pub message: PreconferElection,
    /// Signature over `message`. Must be signed by the proposer for `slot`.
    pub signature: BlsSignature,
}

impl SignedPreconferElection {
    pub fn preconfer_pubkey(&self) -> BlsPublicKey {
        self.message.preconfer_pubkey
    }

    pub fn slot(&self) -> u64 {
        self.message.slot_number
    }

    pub fn chain_id(&self) -> u64 {
        self.message.chain_id
    }

    pub fn gas_limit(&self) -> u64 {
        self.message.gas_limit
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize, TreeHash)]
pub struct PreconferElection {
    /// Public key of the preconfer for `slot`.
    pub preconfer_pubkey: BlsPublicKey,
    /// Slot this delegation is valid for.
    pub slot_number: u64,
    /// Chain ID this election is valid for. For example `1` for Mainnet.
    pub chain_id: u64,
    /// Maximum gas used by all pre-confirmations.
    pub gas_limit: u64, /* TODO: this should be optional but still need to figure out how to
                         * TreeHash */
}
