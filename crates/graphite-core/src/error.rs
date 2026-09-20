use thiserror::Error;

#[derive(Debug, Error)]
pub enum GraphiteError {
    #[error("{0}")]
    Message(String),
    #[error("unsupported database type: {0}")]
    UnsupportedType(String),
    #[error("not connected")]
    NotConnected,
    #[error("invalid identifier: {0}")]
    InvalidIdentifier(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl GraphiteError {
    pub fn msg(text: impl Into<String>) -> Self {
        Self::Message(text.into())
    }
}

pub type Result<T> = std::result::Result<T, GraphiteError>;
