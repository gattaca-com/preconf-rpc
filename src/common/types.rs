use std::collections::HashMap;

use alloy::rpc::types::beacon::BlsPublicKey;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(bound = "T: Serialize + serde::de::DeserializeOwned")]
#[serde(untagged)]
pub enum ApiResult<T: Serialize + DeserializeOwned> {
    Ok(T),
    Err(String),
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(bound = "T: Serialize + serde::de::DeserializeOwned")]
pub struct BeaconResponse<T: Serialize + DeserializeOwned> {
    pub data: T,
    #[serde(flatten)]
    pub meta: HashMap<String, serde_json::Value>,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct SyncStatus {
    #[serde_as(as = "DisplayFromStr")]
    pub head_slot: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub sync_distance: usize,
    pub is_syncing: bool,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ProposerDuty {
    #[serde_as(as = "DisplayFromStr")]
    #[serde(rename = "pubkey")]
    pub public_key: BlsPublicKey,
    #[serde_as(as = "DisplayFromStr")]
    pub validator_index: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub slot: u64,
}

#[cfg(test)]
mod tests {
    use alloy::{primitives::hex::FromHex, rpc::types::beacon::BlsPublicKey};

    use super::{BeaconResponse, ProposerDuty};

    #[test]
    fn test_beacon_proposer() {
        let data = r#"{
            "dependent_root": "0x5fd8a9bc4111be67ad969970ad3bc9ccc1a398cc8ea033650b61f58803b0a847",
            "execution_optimistic": false,
            "data": [
                {
                    "pubkey": "0xab7f3ed5f4f9d6136b90c22eeae38faa892036971e1a0245a5472da57ae7fcf6ba29d55dd4d162301fb256822e46261c",
                    "validator_index": "467380",
                    "slot": "9079424"
                },
                {
                    "pubkey": "0xa9e4b8c958f25df42fcbccdc7547b101d9ad4c31081438479234f7f2e01a0b726dd91b9394b32efd03336794344981a9",
                    "validator_index": "1291439",
                    "slot": "9079425"
                }
            ]
        }"#;

        let decoded = serde_json::from_str::<BeaconResponse<Vec<ProposerDuty>>>(data).unwrap();

        assert!(decoded.meta.contains_key("dependent_root"));

        let duties = decoded.data;

        assert_eq!(
            duties[0].public_key,
            BlsPublicKey::from_hex("0xab7f3ed5f4f9d6136b90c22eeae38faa892036971e1a0245a5472da57ae7fcf6ba29d55dd4d162301fb256822e46261c").unwrap()
        );
        assert_eq!(duties[0].validator_index, 467380);
        assert_eq!(duties[0].slot, 9079424);
        assert_eq!(
            duties[1].public_key,
            BlsPublicKey::from_hex("0xa9e4b8c958f25df42fcbccdc7547b101d9ad4c31081438479234f7f2e01a0b726dd91b9394b32efd03336794344981a9").unwrap()
        );
        assert_eq!(duties[1].validator_index, 1291439);
        assert_eq!(duties[1].slot, 9079425);
    }
}
