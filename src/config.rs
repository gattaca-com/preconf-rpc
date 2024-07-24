use std::{fs, path::Path};

use alloy::rpc::types::beacon::BlsPublicKey;
use eyre::{Result, WrapErr};
use hashbrown::HashMap;
use serde::{Deserialize, Deserializer};
use url::Url;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Provider {
    Lookahead,
    Registry,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(rename = "lookahead")]
    pub lookaheads: Vec<Lookahead>,
    #[serde(rename = "beacon-nodes")]
    pub beacon_nodes: Vec<String>,
}

#[derive(Debug)]
pub struct Lookahead {
    pub chain_id: u16,
    pub relays: Vec<String>,
    pub registry: Option<HashMap<BlsPublicKey, Url>>,
    pub provider: Provider,
}

impl<'de> Deserialize<'de> for Lookahead {
    fn deserialize<D>(deserializer: D) -> Result<Lookahead, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "kebab-case")]
        struct LookaheadHelper {
            chain_id: u16,
            relays: Vec<String>,
            registry: Option<HashMap<BlsPublicKey, Url>>,
            url_provider: Provider,
        }

        let helper = LookaheadHelper::deserialize(deserializer)?;

        if matches!(helper.url_provider, Provider::Registry) && helper.registry.is_none() {
            return Err(serde::de::Error::custom(
                "registry map is mandatory when url-provider is set to registry",
            ));
        }

        Ok(Lookahead {
            chain_id: helper.chain_id,
            relays: helper.relays,
            registry: helper.registry,
            provider: helper.url_provider,
        })
    }
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
            provider: Provider::Lookahead,
        };

        let _expected_config = Config {
            lookaheads: vec![expected_lookahead],
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
            provider: Provider::Lookahead,
        };

        let _expected_config = Config {
            lookaheads: vec![expected_lookahead],
            beacon_nodes: vec!["node1".to_string(), "node2".to_string()],
        };

        let config: Config = toml::from_str(data).unwrap();
        assert!(matches!(config, _expected_config));
    }

    #[test]
    fn test_fail_if_wrong_registry_combination() {
        let data = r#"
        beacon-nodes = ["node1", "node2"]
        [[lookahead]]
        chain-id = 1
        url-provider = "registry"
        relays = ["relay1", "relay2"]
        "#;
        let config: Result<Config> = toml::from_str(data).wrap_err("error parsing config");
        assert!(config.is_err());
    }
}
