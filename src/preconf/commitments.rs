use std::str::FromStr;

use alloy::{primitives::Signature, rpc::types::beacon::BlsSignature};
use reth_primitives::TransactionSigned;
use serde::{de, Deserialize, Deserializer, Serialize};

/// Request to include a transaction at a specific slot
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InclusionRequest {
    /// The consensus slot number at which the transaction should be included.
    pub slot: u64,
    /// The transaction to be included.
    #[serde(deserialize_with = "deserialize_tx_signed", serialize_with = "serialize_tx_signed")]
    pub tx: TransactionSigned,
    /// The signature over the "slot" and "tx" fields by the user.
    /// A valid signature is the only proof that the user actually requested
    /// this specific commitment to be included at the given slot.
    #[serde(deserialize_with = "deserialize_from_str", serialize_with = "signature_as_str")]
    pub signature: Signature,
}

fn deserialize_tx_signed<'de, D>(deserializer: D) -> Result<TransactionSigned, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    let data = alloy::hex::decode(s.trim_start_matches("0x")).map_err(de::Error::custom)?;
    TransactionSigned::decode_enveloped(&mut data.as_slice()).map_err(de::Error::custom)
}

fn serialize_tx_signed<S>(tx: &TransactionSigned, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut data = Vec::new();
    tx.encode_enveloped(&mut data);
    serializer.serialize_str(&format!("0x{}", alloy::hex::encode(&data)))
}

fn deserialize_from_str<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: std::fmt::Display,
{
    let s = String::deserialize(deserializer)?;
    T::from_str(s.trim_start_matches("0x")).map_err(de::Error::custom)
}

fn signature_as_str<S: serde::Serializer>(
    sig: &Signature,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let parity = sig.v();
    // As bytes encodes the parity as 27/28, need to change that.
    let mut bytes = sig.as_bytes();
    bytes[bytes.len() - 1] = if parity.y_parity() { 1 } else { 0 };
    serializer.serialize_str(&format!("0x{}", alloy::hex::encode(bytes)))
}

// TODO
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InclusionReponse {
    pub signature: BlsSignature,
    pub message: InclusionRequest,
}
