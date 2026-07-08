//! CLI handler for `oz model` — manage GGUF model files.
//! Extracted from `src/lib.rs` inline dispatch.

use anyhow::Result;

#[cfg(feature = "model-mgmt")]
pub async fn cmd_model(command: crate::model::ModelCommand) -> Result<()> {
    match crate::model::run(command).await {
        Ok(()) => Ok(()),
        Err(e) => {
            ozone_core::cli::error(&format!("{e}"));
            std::process::exit(1);
        }
    }
}
