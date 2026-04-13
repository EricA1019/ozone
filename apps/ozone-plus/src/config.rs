//! ozone-plus application configuration.

use serde::{Deserialize, Serialize};

/// Top-level ozone-plus configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OzonePlusConfig {
    #[serde(default)]
    pub hooks: crate::hooks::HooksConfig,
}
