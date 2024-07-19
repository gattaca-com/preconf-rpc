use serde::{Deserialize, Serialize};

pub(crate) mod client;
pub(crate) mod error;
pub(crate) mod types;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BeaconClientConfig {
    pub beacon_client_addresses: Vec<String>,
    pub core: Option<usize>,
}
