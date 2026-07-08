//! CLI handler for `oz creative-write` — run creative writing evaluation.
//! Extracted from `src/lib.rs` inline dispatch.

use anyhow::Result;

#[cfg(feature = "eval")]
pub async fn cmd_creative_write(
    model: String,
    base_url: String,
) -> Result<()> {
    let root = crate::eval::resolve_project_root()?;
    let prompt_bank = crate::creative_writing::load_prompt_bank(&root)?;
    if prompt_bank.is_empty() {
        anyhow::bail!("No prompts found in creative writing prompt bank");
    }

    let artifacts_dir = root.join("results").join("creative_writing");
    let csv_path = crate::creative_writing::run_creative_writing_eval(
        &model,
        &prompt_bank,
        &base_url,
        &artifacts_dir,
    )
    .await?;

    // Build and write markdown report
    let report_md = crate::creative_writing::build_creative_report(&csv_path)?;
    let report_path = csv_path.with_extension("md");
    std::fs::write(&report_path, &report_md)?;

    ozone_core::cli::success(&format!("Creative writing eval complete for '{}'", model));
    ozone_core::cli::field("CSV:", &csv_path.display());
    ozone_core::cli::field("Report:", &report_path.display());
    Ok(())
}
