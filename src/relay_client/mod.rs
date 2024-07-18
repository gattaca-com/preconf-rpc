mod client;
pub(crate) mod error;
pub(crate) use client::RelayClient;

/// Handles communication to a single relay.
#[derive(Clone, Debug)]
pub struct RelayClientConfig {
    url: String,
    /// True if the relay supports fetching all elected preconfers in 1 call by
    /// leaving out the `slot` query parameter.
    get_lookahead_enabled: bool,
}

impl RelayClientConfig {
    pub fn new(url: String, get_lookahead_enabled: bool) -> Self {
        Self { url, get_lookahead_enabled }
    }
}
