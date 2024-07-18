use reqwest::StatusCode;

#[derive(Debug, thiserror::Error)]
pub enum RelayClientError {
    #[error("Reqwest error: {0}")]
    ReqwestError(#[from] reqwest::Error),

    #[error("Relay responded with an error. Code: {status_code:?}, Error: {error}")]
    RelayError { status_code: StatusCode, error: String },
}
