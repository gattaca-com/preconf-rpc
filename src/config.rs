use std::{fs, path::Path};

use eyre::{Result, WrapErr};
use hashbrown::HashMap;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(rename = "lookahead-providers-relays")]
    pub lookahead_providers_relays: Vec<LookaheadProvider>,
}

#[derive(Debug, Deserialize)]
pub struct LookaheadProvider {
    #[serde(rename = "chain-id")]
    pub chain_id: u16,
    #[serde(rename = "relay-urls")]
    pub relay_urls: Vec<String>,
}

impl Config {
    pub fn from_file(filepath: &Path) -> Result<Self> {
        let toml_str = fs::read_to_string(filepath)?;
        toml::from_str(&toml_str).wrap_err("could not parse configuration file")
    }
}
