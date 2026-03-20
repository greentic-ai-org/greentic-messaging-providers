use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::Value;

use crate::bindings::greentic::secrets_store::secrets_store;
use crate::bindings::greentic::state::state_store;
use webchat_directline_core::directline::store::{SecretStore, StateStore};

/// Host-backed state store implementation using the WIT state-store interface.
pub struct HostStateStore;

impl StateStore for HostStateStore {
    fn read(&mut self, key: &str) -> Result<Option<Vec<u8>>, String> {
        match state_store::read(key, None) {
            Ok(bytes) => {
                if bytes.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(bytes))
                }
            }
            Err(err) => {
                if err.code == "not_found" {
                    Ok(None)
                } else {
                    Err(format!("state read error: {} - {}", err.code, err.message))
                }
            }
        }
    }

    fn write(&mut self, key: &str, value: &[u8]) -> Result<(), String> {
        state_store::write(key, value, None)
            .map(|_ack| ())
            .map_err(|err| format!("state write error: {} - {}", err.code, err.message))
    }
}

/// Config-aware secret store that first checks injected config for secrets
/// before falling back to the host secrets_store interface.
///
/// The host injects secrets as base64-encoded values with `{key}_b64` suffix
/// in the request config. This allows the component to work in provider_core_only
/// mode where direct secrets_store access is denied.
pub struct ConfigAwareSecretStore {
    config: Option<Value>,
}

impl ConfigAwareSecretStore {
    pub fn new(config: Option<Value>) -> Self {
        Self { config }
    }

    /// Try to get secret from injected config first.
    /// The host injects secrets as `{key}_b64` with base64-encoded value.
    fn get_from_config(&self, key: &str) -> Option<Vec<u8>> {
        let config = self.config.as_ref()?;
        let config_obj = config.as_object()?;
        let key_b64 = format!("{}_b64", key);
        let encoded = config_obj.get(&key_b64)?.as_str()?;
        STANDARD.decode(encoded).ok()
    }
}

impl SecretStore for ConfigAwareSecretStore {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
        // First check injected config
        if let Some(bytes) = self.get_from_config(key) {
            return Ok(Some(bytes));
        }

        // Fallback to host secrets_store interface
        match secrets_store::get(key) {
            Ok(opt) => Ok(opt),
            Err(err) => Err(format!("secret error: {} - {}", err.name(), err.message())),
        }
    }
}
