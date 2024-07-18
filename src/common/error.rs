#[derive(Debug, thiserror::Error)]
pub enum BeaconClientError {
    #[error("Reqwest error: {0}")]
    ReqwestError(#[from] reqwest::Error),

    #[error("URL parse error: {0}")]
    UrlError(#[from] url::ParseError),

    #[error("JSON serialization/deserialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("error from API: {0}")]
    Api(String),

    #[error("missing expected data in response: {0}")]
    MissingExpectedData(String),

    #[error("beacon node unavailable")]
    BeaconNodeUnavailable,

    #[error("block validation failed")]
    BlockValidationFailed,

    #[error("block integration failed")]
    BlockIntegrationFailed,

    #[error("beacon node syncing")]
    BeaconNodeSyncing,

    #[error("channel error")]
    ChannelError,
}
