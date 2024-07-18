#[derive(Debug, thiserror::Error)]
pub enum RelayClientError {
    #[error("Reqwest error: {0}")]
    ReqwestError(#[from] reqwest::Error),
}
