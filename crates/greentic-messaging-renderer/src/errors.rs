use thiserror::Error;

/// Generic renderer error wrapper.
#[derive(Debug, Error)]
#[error("{0}")]
pub struct RendererError(pub String);

impl From<String> for RendererError {
    fn from(value: String) -> Self {
        RendererError(value)
    }
}

impl From<&str> for RendererError {
    fn from(value: &str) -> Self {
        RendererError(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_error_formats_string_and_str_sources() {
        let from_string = RendererError::from("render failed".to_string());
        let from_str = RendererError::from("bad card");

        assert_eq!(from_string.to_string(), "render failed");
        assert_eq!(from_str.to_string(), "bad card");
    }
}
