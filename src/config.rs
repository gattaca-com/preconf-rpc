use std::{fs, path::Path};

use alloy::rpc::types::beacon::BlsPublicKey;
use eyre::{Result, WrapErr};
use hashbrown::HashMap;
use serde::Deserialize;
use url::Url;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UrlProvider {
    Lookahead,
    Registry,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(rename = "lookahead")]
    pub lookahead_providers_relays: Vec<Lookahead>,
    #[serde(rename = "beacon-nodes")]
    pub beacon_nodes: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Lookahead {
    #[serde(rename = "chain-id")]
    pub chain_id: u16,
    #[serde(rename = "relays")]
    pub relays: Vec<String>,
    #[serde(rename = "registry")]
    pub registry: Option<HashMap<BlsPublicKey, Url>>,
    #[serde(rename = "url-provider")]
    pub url_provider: UrlProvider,
}

impl Config {
    pub fn from_file(filepath: &Path) -> Result<Self> {
        let toml_str = fs::read_to_string(filepath)?;
        toml::from_str(&toml_str).wrap_err("could not parse configuration file")
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_deserialize_config() {
        let data = r#"
        beacon-nodes = ["node1", "node2"]
        [[lookahead]]
        chain-id = 1
        url-provider = "lookahead"
        relays = ["relay1", "relay2"]
        [lookahead.registry]
        "0x8248efd1f054fcccd090879c4011ed91ee9f9d0db5ad125ae1af74fdd33de809ddc882400d99b5184ca065d4570df8cc" = "localhost:21009"
        "#;

        let expected_registry = {
            let mut registry = HashMap::new();
            registry.insert(BlsPublicKey::from_str("0x8248efd1f054fcccd090879c4011ed91ee9f9d0db5ad125ae1af74fdd33de809ddc882400d99b5184ca065d4570df8cc").unwrap(), Url::from_str("localhost:21009").unwrap());
            registry
        };

        let expected_lookahead = Lookahead {
            chain_id: 1,
            relays: vec!["relay1".to_string(), "relay2".to_string()],
            registry: Some(expected_registry),
            url_provider: UrlProvider::Lookahead,
        };

        let _expected_config = Config {
            lookahead_providers_relays: vec![expected_lookahead],
            beacon_nodes: vec!["node1".to_string(), "node2".to_string()],
        };

        let config: Config = toml::from_str(data).unwrap();
        assert!(matches!(config, _expected_config));
    }

    #[test]
    fn test_deserialize_config_no_lookahead_registry() {
        let data = r#"
        beacon-nodes = ["node1", "node2"]
        [[lookahead]]
        chain-id = 1
        url-provider = "lookahead"
        relays = ["relay1", "relay2"]
        "#;

        let expected_lookahead = Lookahead {
            chain_id: 1,
            relays: vec!["relay1".to_string(), "relay2".to_string()],
            registry: None,
            url_provider: UrlProvider::Lookahead,
        };

        let _expected_config = Config {
            lookahead_providers_relays: vec![expected_lookahead],
            beacon_nodes: vec!["node1".to_string(), "node2".to_string()],
        };

        let config: Config = toml::from_str(data).unwrap();
        assert!(matches!(config, _expected_config));
    }
}
