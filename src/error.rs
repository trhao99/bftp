use thiserror::Error;

#[derive(Debug, Error)]
pub enum BftpError {
    #[error("API error: errno={errno}, msg={errmsg:?}")]
    Api { errno: i32, errmsg: Option<String> },
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Auth error: {0}")]
    Auth(String),
    #[error("File not found: {0}")]
    FileNotFound(String),
    #[error("{0}")]
    Other(String),
}

impl From<anyhow::Error> for BftpError {
    fn from(e: anyhow::Error) -> Self {
        BftpError::Other(e.to_string())
    }
}
