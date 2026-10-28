use serde::Serialize;

/// Unified error type for the engine.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("invalid url: {0}")]
    Url(#[from] url::ParseError),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("download not found")]
    NotFound,

    #[error("invalid download state: {0}")]
    InvalidState(String),

    #[error("download failed: {0}")]
    Failed(String),

    #[error("server does not support range requests")]
    RangeUnsupported,

    #[error("operation cancelled")]
    Cancelled,

    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn failed(msg: impl Into<String>) -> Self {
        Self::Failed(msg.into())
    }
}

impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
