//! Workflow orchestration for profiling — the `run_workflow` function.
//!
//! Extracted from `mod.rs`. Uses `super::*` to access types and
//! `pub(super)` helper functions defined in the parent module.


use anyhow::{anyhow, Result};
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

use crate::hardware;
use crate::launch_config;
#[cfg(any(feature = "analyze", feature = "bench", feature = "profiling-ui"))]
use crate::{analyze, bench};
#[cfg(any(feature = "analyze", feature = "bench", feature = "eval", feature = "profiling-ui"))]
use crate::db;
#[cfg(any(feature = "profiling-ui", feature = "sweep"))]
use crate::llamacpp;
use crate::sweep;

// Helper functions and types from the parent profiling module.
use super::{
    build_failure_report, build_success_report,
    export_llamacpp_profiles, launcher_path, llamacpp_export_dir,
    presets_path, send_completed, send_failed,
    ProfilingAction,
    ProfilingSuccessReport, WorkflowEvent, WorkflowRequest,
};


pub async fn run_workflow(
    request: WorkflowRequest,
    tx: UnboundedSender<WorkflowEvent>,
    cancel: CancellationToken,
) -> Result<()> {
    let action = request.action;
    if action == ProfilingAction::ReviewIssue {
        let report = build_failure_report(
            &request.record,
            action,
            "The selected model or launcher path is not valid enough to start profiling.".into(),
            None,
        );
        send_failed(&tx, report);
        return Ok(());
    }

    // ImportSpecs captures and saves system hardware — no model or backend needed.
    if action == ProfilingAction::ImportSpecs {
        let _ = tx.send(WorkflowEvent::Status {
            title: "Import Specs".into(),
            detail: "Capturing GPU, CPU, RAM, and CUDA info…".into(),
        });
        let profile = hardware::import_system_specs();
        let gpu_line = match (&profile.gpu_name, &profile.gpu) {
            (Some(name), Some(gpu)) => format!("{name} · {} MB", gpu.total_mb),
            (Some(name), None) => name.clone(),
            _ => "No GPU detected".into(),
        };
        let cuda_line = if profile.cuda_available {
            format!(
                "CUDA ✓ v{} · compute {} · flash-attn {}",
                profile.cuda_version.as_deref().unwrap_or("?"),
                profile.compute_capability.as_deref().unwrap_or("?"),
                if profile.flash_attn_supported {
                    "✓"
                } else {
                    "✗"
                },
            )
        } else {
            "CUDA ✗".into()
        };
        let summary = format!(
            "GPU: {gpu_line}\n{cuda_line}\nCPU: {} logical / {} physical · RAM: {} MB total / {} MB free\nSaved to system-profile.json",
            profile.cpu_logical, profile.cpu_physical, profile.ram_total_mb, profile.ram_free_mb,
        );
        let report = ProfilingSuccessReport {
            model_name: request.record.model_name.clone(),
            action,
            summary,
            benchmark_count: 0,
            ok_benchmark_count: 0,
            profile_count: 0,
            best_tokens_per_sec: None,
            recommended_profile: None,
            saved_profile_report: None,
            suggestions: vec![
                "System specs are now cached — Ozone will skip hardware polling for 24 hours."
                    .into(),
            ],
            export_detail: None,
            auto_saved_profile: None,
        };
        send_completed(&tx, report);
        return Ok(());
    }

    // ThreadSweep tests different thread counts for the selected model.
    if action == ProfilingAction::ThreadSweep {
        let backend = request.profiling_backend.resolve_backend().ok_or_else(|| {
            anyhow!(
                "Requested profiling backend unavailable: {}",
                request.profiling_backend.display_name()
            )
        })?;
        let plan = request
            .launch_plan_override
            .clone()
            .unwrap_or_else(|| launch_config::plan_profiling_launch(&request.record, &request.hardware));

        let _ = tx.send(WorkflowEvent::Status {
            title: "Thread Sweep".into(),
            detail: format!(
                "Testing thread counts 1-12 at ctx={} gpu={}…",
                plan.context_size,
                plan.gpu_layers_display(),
            ),
        });

        match bench::run_thread_sweep(
            &request.record.model_name,
            &request.record.model_path,
            &backend,
            plan.gpu_layers,
            plan.context_size,
            plan.quant_k,
            plan.quant_v,
        )
        .await
        {
            Ok(results) => {
                let best = results.iter().filter(|r| r.status == "ok").max_by(|a, b| {
                    a.tokens_per_sec
                        .partial_cmp(&b.tokens_per_sec)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                let summary = if let Some(best) = best {
                    format!(
                        "Thread sweep complete. {} configs tested. Best: {:.1} t/s",
                        results.len(),
                        best.tokens_per_sec,
                    )
                } else {
                    format!(
                        "Thread sweep complete. {} configs tested, none successful.",
                        results.len()
                    )
                };
                let report = ProfilingSuccessReport {
                    model_name: request.record.model_name.clone(),
                    action,
                    summary,
                    benchmark_count: results.len(),
                    ok_benchmark_count: results.iter().filter(|r| r.status == "ok").count(),
                    profile_count: 0,
                    best_tokens_per_sec: best.map(|r| r.tokens_per_sec),
                    recommended_profile: None,
                    saved_profile_report: None,
                    suggestions: vec![
                        "Review the benchmark history for 'thread-sweep' to compare thread counts."
                            .into(),
                    ],
                    export_detail: None,
                    auto_saved_profile: None,
                };
                send_completed(&tx, report);
            }
            Err(error) => {
                let report = build_failure_report(&request.record, action, error.to_string(), None);
                send_failed(&tx, report);
            }
        }
        return Ok(());
    }

    // ExportPresets only reads from DB and writes files — no launcher needed.
    if action == ProfilingAction::ExportPresets {
        let use_llamacpp = llamacpp::resolved_llamacpp_server_path()
            .ok()
            .map(|p| p.exists())
            .unwrap_or(false)
            || !launcher_path().exists();

        let conn = db::open()?;
        let profiles = db::get_profiles(&conn, &request.record.model_name)?;

        if use_llamacpp {
            let sh_path = llamacpp_export_dir().join("llamacpp-profiles.sh");
            let _ = tx.send(WorkflowEvent::Status {
                title: "Export".into(),
                detail: format!("Exporting llama.cpp profiles to {}…", sh_path.display()),
            });
            match export_llamacpp_profiles(&profiles) {
                Ok(out) => {
                    let action = request.action;
                    let mut report = match build_success_report(
                        &request.record,
                        request.action,
                        request.launch_profile_name.as_deref(),
                    ) {
                        Ok(r) => r,
                        Err(e) => ProfilingSuccessReport {
                            model_name: request.record.model_name.clone(),
                            action,
                            summary: format!("Export completed (report error: {e})"),
                            benchmark_count: 0,
                            ok_benchmark_count: 0,
                            profile_count: 0,
                            best_tokens_per_sec: None,
                            recommended_profile: None,
                            saved_profile_report: None,
                            suggestions: vec![],
                            export_detail: None,
                            auto_saved_profile: None,
                        },
                    };
                    report.export_detail = Some(format!("llama.cpp: {}", out.display()));
                    send_completed(&tx, report);
                }
                Err(error) => {
                    let report = build_failure_report(
                        &request.record,
                        request.action,
                        error.to_string(),
                        None,
                    );
                    send_failed(&tx, report);
                }
            }
        } else {
            let _ = tx.send(WorkflowEvent::Status {
                title: "Export".into(),
                detail: format!("Exporting saved profiles to {}…", presets_path().display()),
            });
            match analyze::export_presets_conf_quiet(
                &presets_path(),
                Some(&request.record.model_name),
            ) {
                Ok(_count) => {
                    let action = request.action;
                    let mut report = match build_success_report(
                        &request.record,
                        request.action,
                        request.launch_profile_name.as_deref(),
                    ) {
                        Ok(r) => r,
                        Err(e) => ProfilingSuccessReport {
                            model_name: request.record.model_name.clone(),
                            action,
                            summary: format!("Export completed (report error: {e})"),
                            benchmark_count: 0,
                            ok_benchmark_count: 0,
                            profile_count: 0,
                            best_tokens_per_sec: None,
                            recommended_profile: None,
                            saved_profile_report: None,
                            suggestions: vec![],
                            export_detail: None,
                            auto_saved_profile: None,
                        },
                    };
                    if let Ok(content) = std::fs::read_to_string(presets_path()) {
                        let model_lines: Vec<&str> = content
                            .lines()
                            .filter(|line| line.contains(&request.record.model_name))
                            .collect();
                        if !model_lines.is_empty() {
                            report.export_detail = Some(model_lines.join("\n"));
                        }
                    }
                    send_completed(&tx, report);
                }
                Err(error) => {
                    let report = build_failure_report(
                        &request.record,
                        request.action,
                        error.to_string(),
                        None,
                    );
                    send_failed(&tx, report);
                }
            }
        }
        return Ok(());
    }

    if !request.record.model_path.exists() {
        let report = build_failure_report(
            &request.record,
            action,
            "Profiling prerequisites are missing.".into(),
            None,
        );
        send_failed(&tx, report);
        return Ok(());
    }

    if cancel.is_cancelled() {
        let _ = tx.send(WorkflowEvent::Cancelled);
        return Ok(());
    }

    match request.action {
        ProfilingAction::QuickSweep | ProfilingAction::FullSweep => {
            let quick = matches!(request.action, ProfilingAction::QuickSweep);
            let (context_sizes, quant_kv_levels) = if quick {
                (vec![4096, 8192], vec![(1u8, 1u8)])
            } else {
                let native_max =
                    crate::gguf::read_context_length(&request.record.model_path).unwrap_or(131072);
                let ctxs = sweep::generate_context_steps(native_max);
                // Test asymmetric K/V pairs: (K, V) = (f16, f16), (q8_0, q8_0), (q8_0, q4_0)
                (ctxs, vec![(1u8, 1u8), (2u8, 2u8), (2u8, 3u8)])
            };
            let gpu_vram_budget_mb = request
                .hardware
                .gpu
                .as_ref()
                .map(|gpu| (gpu.total_mb as f64 * 0.9) as u32)
                .unwrap_or(0);
            let backend = request.profiling_backend.resolve_backend().ok_or_else(|| {
                anyhow!(
                    "Requested profiling backend unavailable: {}",
                    request.profiling_backend.display_name()
                )
            })?;
            let thread_backend = backend.clone();
            let seed_plan = launch_config::plan_profiling_launch(&request.record, &request.hardware);
            let config = sweep::SweepConfig {
                model_name: request.record.model_name.clone(),
                model_path: request.record.model_path.clone(),
                backend,
                model_size_gb: request.record.model_size_gb,
                total_layers: seed_plan.total_layers,
                context_sizes: context_sizes.clone(),
                quant_kv_levels: quant_kv_levels.clone(),
                gpu_vram_budget_mb,
            };
            let _ = tx.send(WorkflowEvent::Status {
                title: "Profiling".into(),
                detail: format!("Starting {}…", request.action.label().to_lowercase()),
            });
            let _ = tx.send(WorkflowEvent::Status {
                title: "Model".into(),
                detail: format!(
                    "{} · Max context (GGUF): {} · {} K/V pairs",
                    request.record.model_name,
                    context_sizes.last().copied().unwrap_or(0),
                    quant_kv_levels.len(),
                ),
            });
            let cancel_ref = cancel.clone();
            match sweep::run_sweep_with_progress(config, |progress| {
                if cancel_ref.is_cancelled() {
                    return;
                }
                let _ = tx.send(WorkflowEvent::Progress {
                    title: if quick {
                        "Quick sweep".into()
                    } else {
                        "Full sweep".into()
                    },
                    detail: progress.message,
                    current: progress.current,
                    total: progress.total,
                });
            })
            .await
            {
                Ok(_result) if cancel.is_cancelled() => {
                    let _ = tx.send(WorkflowEvent::Cancelled);
                }
                Ok(result) if result.configs_tested > 0 => {
                    // Auto-chain: generate profiles after sweep success
                    let _ = tx.send(WorkflowEvent::Status {
                        title: "Generating profiles".into(),
                        detail: "Creating speed/context profiles from benchmark data…".into(),
                    });
                    let _ = analyze::generate_profiles_quiet(&request.record.model_name);

                    // Auto-save the optimal profile for quick loading
                    let auto_profile = sweep::pick_optimal_profile(
                        &request.record.model_name,
                        &result.pareto_frontier,
                        None,
                    );
                    if let Some(ref optimal) = auto_profile {
                        if let Ok(mut prefs) = crate::prefs::load_prefs().await {
                            prefs.upsert_saved_launch_profile(
                                &request.record.model_name,
                                optimal.clone(),
                            );
                            prefs.set_default_saved_launch_profile(
                                &request.record.model_name,
                                "auto-optimal",
                            );
                            let _ = crate::prefs::save_prefs(&prefs).await;
                            let _ = tx.send(WorkflowEvent::Status {
                                title: "Profile saved".into(),
                                detail: format!(
                                    "Auto-saved 'auto-optimal': ctx={}, K=q{}, V=q{}",
                                    optimal.context_size, optimal.quant_k, optimal.quant_v,
                                ),
                            });
                        }
                    }

                    // Auto-chain: thread sweep on the optimal config found above
                    if let Some(ref optimal) = auto_profile {
                        let _ = tx.send(WorkflowEvent::Status {
                            title: "Thread sweep".into(),
                            detail: format!(
                                "Testing thread counts for ctx={} K=q{} V=q{}…",
                                optimal.context_size, optimal.quant_k, optimal.quant_v,
                            ),
                        });
                        match bench::run_thread_sweep(
                            &request.record.model_name,
                            &request.record.model_path,
                            &thread_backend,
                            optimal.gpu_layers,
                            optimal.context_size,
                            optimal.quant_k,
                            optimal.quant_v,
                        )
                        .await
                        {
                            Ok(thread_results) => {
                                let best_thread = thread_results
                                    .iter()
                                    .filter(|r| r.status == "ok")
                                    .max_by(|a, b| {
                                        a.tokens_per_sec
                                            .partial_cmp(&b.tokens_per_sec)
                                            .unwrap_or(std::cmp::Ordering::Equal)
                                    });
                                if let Some(best) = best_thread {
                                    // Re-load prefs to get the auto-saved profile, update thread count
                                    if let Ok(mut prefs) = crate::prefs::load_prefs().await {
                                        let model = &request.record.model_name;
                                        if let Some(mut saved) =
                                            prefs.saved_launch_profile(model, "auto-optimal")
                                        {
                                            saved.threads = best.threads;
                                            prefs.upsert_saved_launch_profile(model, saved);
                                            let _ = crate::prefs::save_prefs(&prefs).await;
                                        }
                                    }
                                    let _ = tx.send(WorkflowEvent::Status {
                                        title: "Thread result".into(),
                                        detail: format!(
                                            "Best thread count: {} ({:.1} t/s)",
                                            best.threads.unwrap_or_default(),
                                            best.tokens_per_sec,
                                        ),
                                    });
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(WorkflowEvent::Status {
                                    title: "Thread sweep".into(),
                                    detail: format!("Skipped: {e}"),
                                });
                            }
                        }
                    }

                    // Report CSV path
                    if let Some(ref csv_path) = result.csv_path {
                        let _ = tx.send(WorkflowEvent::Status {
                            title: "CSV saved".into(),
                            detail: format!("{}", csv_path.display()),
                        });
                    }

                    let action = request.action;
                    let auto_saved = auto_profile.clone();
                    let report = match build_success_report(
                        &request.record,
                        request.action,
                        request.launch_profile_name.as_deref(),
                    ) {
                        Ok(mut r) => {
                            r.auto_saved_profile = auto_saved;
                            r
                        }
                        Err(e) => ProfilingSuccessReport {
                            model_name: request.record.model_name.clone(),
                            action,
                            summary: format!("Sweep completed (report error: {e})"),
                            benchmark_count: result.configs_tested as usize,
                            ok_benchmark_count: 0,
                            profile_count: 0,
                            best_tokens_per_sec: None,
                            recommended_profile: None,
                            saved_profile_report: None,
                            suggestions: vec![],
                            export_detail: None,
                            auto_saved_profile: auto_saved,
                        },
                    };
                    send_completed(&tx, report);
                }
                Ok(_) => {
                    let report = build_failure_report(
                        &request.record,
                        request.action,
                        "Sweep completed without any successful benchmark configurations.".into(),
                        Some("oom"),
                    );
                    send_failed(&tx, report);
                }
                Err(error) => {
                    let report = build_failure_report(
                        &request.record,
                        request.action,
                        error.to_string(),
                        None,
                    );
                    send_failed(&tx, report);
                }
            }
        }
        ProfilingAction::SingleBenchmark | ProfilingAction::BenchmarkSavedProfile => {
            if cancel.is_cancelled() {
                let _ = tx.send(WorkflowEvent::Cancelled);
                return Ok(());
            }
            let backend = request.profiling_backend.resolve_backend().ok_or_else(|| {
                anyhow!(
                    "Requested profiling backend unavailable: {}",
                    request.profiling_backend.display_name()
                )
            })?;
            let plan = request.launch_plan_override.clone().unwrap_or_else(|| {
                launch_config::plan_profiling_launch(&request.record, &request.hardware)
            });
            let benchmark_label = match request.action {
                ProfilingAction::BenchmarkSavedProfile => request
                    .launch_profile_name
                    .as_deref()
                    .map(|profile_name| format!("Benchmarking saved profile '{profile_name}'"))
                    .unwrap_or_else(|| "Benchmarking saved profile".into()),
                _ => "Benchmark".into(),
            };
            let _ = tx.send(WorkflowEvent::Status {
                title: benchmark_label,
                detail: format!(
                    "Benchmarking ctx={} gpu={}/{} cpu={} K=q{} V=q{}",
                    plan.context_size,
                    plan.gpu_layers_display(),
                    plan.total_layers,
                    plan.cpu_layers,
                    plan.quant_k,
                    plan.quant_v,
                ),
            });
            match bench::run_benchmark_with_progress(
                &request.record.model_name,
                &request.record.model_path,
                &backend,
                plan.gpu_layers,
                plan.context_size,
                plan.quant_k,
                plan.quant_v,
                plan.threads,
                bench::BenchMode::Precise,
                |progress| {
                    let _ = tx.send(WorkflowEvent::Status {
                        title: "Benchmark".into(),
                        detail: progress.message,
                    });
                },
            )
            .await
            {
                Ok(_result) if cancel.is_cancelled() => {
                    let _ = tx.send(WorkflowEvent::Cancelled);
                }
                Ok(result) => {
                    let _ = bench::store_result_with_profile(
                        bench::BenchmarkStoreRequest {
                            model_name: &request.record.model_name,
                            model_size_gb: request.record.model_size_gb,
                            gpu_layers: plan.gpu_layers,
                            context_size: plan.context_size,
                            quant_k: plan.quant_k as u32,
                            quant_v: plan.quant_v as u32,
                            threads: plan.threads.unwrap_or(0),
                            launch_profile_name: request.launch_profile_name.as_deref(),
                        },
                        &result,
                    );
                    if result.status == "ok" {
                        let action = request.action;
                        let report = match build_success_report(
                            &request.record,
                            request.action,
                            request.launch_profile_name.as_deref(),
                        ) {
                            Ok(r) => r,
                            Err(e) => ProfilingSuccessReport {
                                model_name: request.record.model_name.clone(),
                                action,
                                summary: format!("Benchmark ok (report error: {e})"),
                                benchmark_count: 0,
                                ok_benchmark_count: 0,
                                profile_count: 0,
                                best_tokens_per_sec: None,
                                recommended_profile: None,
                                saved_profile_report: None,
                                suggestions: vec![],
                                export_detail: None,
                                auto_saved_profile: None,
                            },
                        };
                        send_completed(&tx, report);
                    } else {
                        let report = build_failure_report(
                            &request.record,
                            request.action,
                            format!("Benchmark ended with status '{}'.", result.status),
                            Some(&result.status),
                        );
                        send_failed(&tx, report);
                    }
                }
                Err(error) => {
                    let report = build_failure_report(
                        &request.record,
                        request.action,
                        error.to_string(),
                        None,
                    );
                    send_failed(&tx, report);
                }
            }
        }
        ProfilingAction::GenerateProfiles => {
            let _ = tx.send(WorkflowEvent::Status {
                title: "Profiles".into(),
                detail: "Generating profiles from benchmark history…".into(),
            });
            match analyze::generate_profiles_quiet(&request.record.model_name) {
                Ok(_) => {
                    let action = request.action;
                    let report = match build_success_report(
                        &request.record,
                        request.action,
                        request.launch_profile_name.as_deref(),
                    ) {
                        Ok(r) => r,
                        Err(e) => ProfilingSuccessReport {
                            model_name: request.record.model_name.clone(),
                            action,
                            summary: format!("Profiles generated (report error: {e})"),
                            benchmark_count: 0,
                            ok_benchmark_count: 0,
                            profile_count: 0,
                            best_tokens_per_sec: None,
                            recommended_profile: None,
                            saved_profile_report: None,
                            suggestions: vec![],
                            export_detail: None,
                            auto_saved_profile: None,
                        },
                    };
                    send_completed(&tx, report);
                }
                Err(error) => {
                    let report = build_failure_report(
                        &request.record,
                        request.action,
                        error.to_string(),
                        None,
                    );
                    send_failed(&tx, report);
                }
            }
        }
        ProfilingAction::LaunchRecommended
        | ProfilingAction::ReviewIssue
        | ProfilingAction::ImportSpecs
        | ProfilingAction::ThreadSweep => {}
        // ExportPresets is handled before the launcher prerequisite check above.
        ProfilingAction::ExportPresets => unreachable!("ExportPresets handled before match"),
    }

    Ok(())
}
