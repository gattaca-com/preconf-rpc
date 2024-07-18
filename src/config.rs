use std::{collections::HashMap, fs, path::Path, str::FromStr};

use reqwest::Url;
use serde::Deserialize;

#[derive(Deserialize)]
struct RawConfig {
    forward: ForwardServiceConfig,
    redirection_urls: HashMap<String, String>,
}

pub(crate) struct PreconfRpcConfig {
    pub forward: ForwardServiceConfig,
    pub redirection_urls: hashbrown::HashMap<u16, Url>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct ForwardServiceConfig {
    address: String,
    port: u16,
}

impl<P: AsRef<Path>> From<P> for PreconfRpcConfig {
    fn from(value: P) -> Self {
        let toml_content = fs::read_to_string(value).expect("unable to open file");
        let config: RawConfig =
            toml::from_str(&toml_content).expect("could not parse  configuration");
        PreconfRpcConfig {
            forward: config.forward,
            redirection_urls: config
                .redirection_urls
                .into_iter()
                .map(|(v, k)| {
                    (v.parse::<u16>().expect("invalid u8"), Url::from_str(&k).expect("invalid url"))
                })
                .collect(),
        }
    }
}

impl ForwardServiceConfig {
    pub fn listening_addr(&self) -> String {
        format!("{}:{}", self.address, self.port)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn test_preconf_rpc_config_from_valid_toml() {
        let toml_content = r#"
        [forward]
        address = "localhost"
        port = 10053

        [redirection_urls]
        "1" = "https://postman-echo.com/post"
        "#;

        let mut temp_file = NamedTempFile::new().expect("failed to create temp file");
        temp_file.write_all(toml_content.as_bytes()).expect("failed to write to temp file");

        let config = PreconfRpcConfig::from(temp_file.path());

        assert_eq!(config.forward.address, "localhost");
        assert_eq!(config.forward.port, 10053);
        assert_eq!(config.redirection_urls.len(), 1);
        assert!(config.redirection_urls.contains_key(&1u16));
        assert_eq!(
            config.redirection_urls.get(&1u16).unwrap().as_str(),
            "https://postman-echo.com/post"
        );
    }

    #[test]
    #[should_panic(expected = "unable to open file")]
    fn test_preconf_rpc_config_from_nonexistent_file() {
        PreconfRpcConfig::from("nonexistent_file.toml");
    }

    #[test]
    #[should_panic(expected = "invalid u8")]
    fn test_preconf_rpc_config_from_invalid_key() {
        let toml_content = r#"
        [forward]
        address = "localhost"
        port = 10053

        [redirection_urls]
        "invalid" = "https://postman-echo.com/post"
        "#;

        let mut temp_file = NamedTempFile::new().expect("failed to create temp file");
        temp_file.write_all(toml_content.as_bytes()).expect("failed to write to temp file");

        PreconfRpcConfig::from(temp_file.path());
    }

    #[test]
    #[should_panic(expected = "invalid url")]
    fn test_preconf_rpc_config_from_invalid_url() {
        let toml_content = r#"
        [forward]
        address = "localhost"
        port = 10053

        [redirection_urls]
        "1" = "invalid_url"
        "#;

        let mut temp_file = NamedTempFile::new().expect("failed to create temp file");
        temp_file.write_all(toml_content.as_bytes()).expect("failed to write to temp file");

        PreconfRpcConfig::from(temp_file.path());
    }
}
