use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ProviderConfigOut {
    pub(crate) enabled: bool,
}

pub(crate) fn default_config_out() -> ProviderConfigOut {
    ProviderConfigOut { enabled: true }
}

pub(crate) fn validate_config_out(_config: &ProviderConfigOut) -> Result<(), String> {
    Ok(())
}
