use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::http_mock::{
    HttpMode, HttpResponseQueue, new_response_queue, queue_mock_response, queue_mock_response_for,
};

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct Values {
    #[serde(default)]
    pub config: Map<String, Value>,
    #[serde(default)]
    pub secrets: Map<String, Value>,
    #[serde(default)]
    pub to: Map<String, Value>,
    #[serde(default)]
    pub http: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub state: Map<String, Value>,
    // Ordered canned responses for mock HTTP, one popped per outbound call.
    // Lets multi-leg providers (e.g. Teams: OAuth token then Graph send) be
    // exercised in mock mode without a live backend.
    #[serde(default)]
    pub responses: Vec<MockResponseSpec>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct MockResponseSpec {
    #[serde(default = "default_status")]
    pub status: u16,
    #[serde(default)]
    pub body: Value,
    // Optional URL substring this response is bound to (e.g. "/oauth2/v2.0/token").
    // When omitted, responses are served in declaration order.
    #[serde(default)]
    pub url: Option<String>,
}

fn default_status() -> u16 {
    200
}

impl Values {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let bytes = fs::read(&path)
            .with_context(|| format!("failed to read values file {}", path.as_ref().display()))?;
        let values: Values = serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to parse {}", path.as_ref().display()))?;
        Ok(values)
    }

    // Build a mock response queue from `responses`, or None to fall back to
    // the generic canned body. A string body is sent verbatim; any other JSON
    // value is serialized.
    pub fn response_queue(&self) -> Option<HttpResponseQueue> {
        if self.responses.is_empty() {
            return None;
        }
        let queue = new_response_queue();
        for spec in &self.responses {
            let body = match &spec.body {
                Value::String(s) => s.clone().into_bytes(),
                other => serde_json::to_vec(other).unwrap_or_default(),
            };
            match &spec.url {
                Some(url) => queue_mock_response_for(&queue, spec.status, body, url.clone()),
                None => queue_mock_response(&queue, spec.status, body),
            }
        }
        Some(queue)
    }

    pub fn http_mode(&self) -> HttpMode {
        match self
            .http
            .as_deref()
            .unwrap_or("mock")
            .to_ascii_lowercase()
            .as_str()
        {
            "real" => HttpMode::Real,
            _ => HttpMode::Mock,
        }
    }

    pub fn secret_bytes(&self) -> HashMap<String, Vec<u8>> {
        self.secrets
            .iter()
            .map(|(key, value)| {
                let bytes = match value {
                    Value::String(s) => s.as_bytes().to_vec(),
                    other => serde_json::to_string(other)
                        .unwrap_or_default()
                        .into_bytes(),
                };
                (key.clone(), bytes)
            })
            .collect()
    }

    pub fn to_metadata(&self) -> HashMap<String, String> {
        self.to
            .iter()
            .filter_map(|(key, value)| {
                value
                    .as_str()
                    .map(|s| (key.clone(), s.to_owned()))
                    .or_else(|| serde_json::to_string(value).ok().map(|s| (key.clone(), s)))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_mock_http() {
        let values = Values {
            config: Map::new(),
            secrets: Map::new(),
            to: Map::new(),
            http: None,
            state: Map::new(),
            responses: Vec::new(),
        };
        assert!(matches!(values.http_mode(), HttpMode::Mock));
    }

    #[test]
    fn secret_bytes_handles_non_strings() {
        let mut secrets = Map::new();
        secrets.insert("TEXT".to_string(), Value::String("text".to_string()));
        secrets.insert("MAP".to_string(), Value::Object(Map::new()));
        let values = Values {
            config: Map::new(),
            secrets,
            to: Map::new(),
            http: None,
            state: Map::new(),
            responses: Vec::new(),
        };
        let bytes = values.secret_bytes();
        assert_eq!(
            bytes.get("TEXT").map(|v| v.as_slice()),
            Some(b"text" as &[u8])
        );
        assert!(bytes.contains_key("MAP"));
    }
}
