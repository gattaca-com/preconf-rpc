use alloy::rpc::types::beacon::BlsSignature;
use serde::Serialize;
use ssz_types::VariableList;
use tree_hash_derive::TreeHash;

use super::commitments::InclusionRequest;
use crate::ssz::{MaxTransactionsPerPayload, SszTransaction};

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SignedConstraints {
    pub message: ConstraintsMessage,
    /// Signature over `message`. Must be signed by the key relating to the elected
    /// preconfer for `message.slot`.
    pub signature: BlsSignature,
}

/// Specifies inclusion constraints for a `slot`. This message is received by relays and is
/// sent only once. All constraints in a single `constraints` list must be included in order.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, TreeHash)]
pub struct ConstraintsMessage {
    /// Slot these constraints are valid for.
    pub slot: u64,
    /// All transaction constraints
    /// TODO: we should enum InclusionConstraint
    pub constraints: VariableList<
        VariableList<InclusionConstraint, MaxTransactionsPerPayload>,
        MaxTransactionsPerPayload,
    >,
}

/// Constraint representing a transaction that must be *included* in a block.
#[derive(Debug, Clone, Default, PartialEq, Serialize, TreeHash)]
pub struct InclusionConstraint {
    #[serde(with = "ssz_types::serde_utils::hex_var_list")]
    pub tx: SszTransaction,
}

impl From<InclusionRequest> for InclusionConstraint {
    fn from(value: InclusionRequest) -> Self {
        let mut encoded_tx = Vec::new();
        value.tx.encode_enveloped(&mut encoded_tx);
        Self { tx: encoded_tx.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inclusion_constraint_from_inclusion_request() {
        let constraint = SszTransaction::new(vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap();
        let list = VariableList::new(vec![InclusionConstraint { tx: constraint.clone() }]).unwrap();
        let constraints = VariableList::new(vec![list]).unwrap();

        let singed_constraints = SignedConstraints {
            message: ConstraintsMessage { slot: 50, constraints },
            signature: BlsSignature::random(),
        };

        let s = serde_json::to_string(&singed_constraints).unwrap();
        let _encode = alloy::primitives::hex::encode(s);
    }
}
