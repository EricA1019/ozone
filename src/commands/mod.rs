//! CLI command dispatch helpers — extracted from `lib.rs`.
//!
//! Each `cmd_*` function handles one CLI command by calling into the
//! appropriate crate module. `lib.rs::run()` calls these directly.

use anyhow::Result;

pub async fn cmd_clear() -> Result<()> {
    let killed = crate::processes::clear_gpu_backends().await?;
    if killed.is_empty() {
        ozone_core::cli::info("No GPU backends running.");
    } else {
        for k in &killed {
            ozone_core::cli::success(&format!("Stopped: {k}"));
        }
    }
    Ok(())
}

pub async fn cmd_purge_last_model() -> Result<()> {
    let killed = crate::processes::purge_last_model().await?;
    if killed.is_empty() {
        ozone_core::cli::info("No managed llama.cpp model was running.");
    } else {
        for pid in killed {
            ozone_core::cli::success(&format!("Stopped managed llama.cpp pid {pid}"));
        }
    }
    Ok(())
}

pub async fn cmd_import_specs() -> Result<()> {
    ozone_core::cli::header("Import System Specs");
    ozone_core::cli::info("Capturing GPU, CPU, RAM, and CUDA info\u{2026}");
    let profile = crate::hardware::import_system_specs();
    if let Some(ref name) = profile.gpu_name {
        ozone_core::cli::field("GPU:", name);
    }
    if let Some(ref gpu) = profile.gpu {
        ozone_core::cli::field("VRAM:", &format!("{} MB", gpu.total_mb));
    }
    ozone_core::cli::field("CUDA:", &if profile.cuda_available { "\u{2713}" } else { "\u{2717}" });
    if let Some(ref cap) = profile.compute_capability {
        ozone_core::cli::field("Compute Cap:", cap);
    }
    ozone_core::cli::field("Flash Attn:", &if profile.flash_attn_supported { "\u{2713}" } else { "\u{2717}" });
    ozone_core::cli::field("CPU:", &format!("{} logical / {} physical", profile.cpu_logical, profile.cpu_physical));
    ozone_core::cli::field("RAM:", &format!("{} MB total", profile.ram_total_mb));
    ozone_core::cli::success("Saved to system-profile.json");
    Ok(())
}

pub async fn cmd_profiles() -> Result<()> {
    let prefs = crate::prefs::load_prefs().await?;
    let profs = &prefs.saved_launch_profiles;
    if profs.is_empty() {
        ozone_core::cli::info("No saved launch profiles found.");
    } else {
        println!("Saved launch profiles:");
        for (model, profiles) in profs {
            for p in profiles {
                let default_marker = prefs
                    .default_saved_launch_profile_name_for(model)
                    .filter(|d| d == &p.profile_name)
                    .map(|_| " [default]")
                    .unwrap_or("");
                println!(
                    "  {:<20}  {:>7} ctx  {:>3} gpu  K=q{} V=q{}  threads={}{}",
                    p.profile_name,
                    p.context_size,
                    p.gpu_layers,
                    p.quant_k,
                    p.quant_v,
                    p.threads.map(|t| t.to_string()).unwrap_or_else(|| "auto".into()),
                    default_marker,
                );
            }
        }
    }
    Ok(())
}

#[cfg(feature = "eval")]
pub async fn cmd_eval_list() -> Result<()> {
    println!("{:<20} {:<50} KIND", "NAME", "DESCRIPTION");
    for task in crate::eval::EVAL_TASKS {
        let kind_label = match task.kind {
            crate::eval::EvalTaskKind::LmEval { .. } => "lm-eval",
            crate::eval::EvalTaskKind::EvalPlus { .. } => "evalplus",
            crate::eval::EvalTaskKind::CreativeWriting => "creative-writing",
        };
        println!("{:<20} {:<50} {}", task.cli_name, task.description, kind_label);
    }
    Ok(())
}
