use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Common error type providers can reuse to surface failures.
#[derive(Debug, Error, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProviderError {
    #[error("validation error: {0}")]
    Validation(String),
    #[error("transport error: {0}")]
    Transport(String),
    #[error("unknown provider error: {0}")]
    Other(String),
}

impl ProviderError {
    pub fn validation(msg: impl Into<String>) -> Self {
        ProviderError::Validation(msg.into())
    }

    pub fn transport(msg: impl Into<String>) -> Self {
        ProviderError::Transport(msg.into())
    }

    pub fn other(msg: impl Into<String>) -> Self {
        ProviderError::Other(msg.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_validation_error() {
        let err = ProviderError::validation("missing token");
        assert_eq!(err, ProviderError::Validation("missing token".into()));
        assert_eq!(err.to_string(), "validation error: missing token");
    }
}
