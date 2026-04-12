use anyhow::{bail, Result};
use std::collections::BTreeMap;

use crate::db::{self, BenchmarkRow, ProfileRow};

/// Show benchmark results for one model or all models.
pub fn show_benchmarks(model: Option<&str>) -> Result<()> {
    let conn = db::open()?;

    match model {
        Some(name) => {
            let rows = db::get_benchmarks(&conn, name)?;
            if rows.is_empty() {
                println!();
                println!("  No benchmarks found for '{name}'.");
                println!("  Run `ozone bench {name}` or `ozone sweep` first.");
                println!();
                return Ok(());
            }
            print_benchmark_table(name, &rows);
        }
        None => {
            let rows = db::get_all_benchmarks(&conn)?;
            if rows.is_empty() {
                println!();
                println!("  No benchmarks found.");
                println!("  Run `ozone bench <model>` or `ozone sweep` first.");
                println!();
                return Ok(());
            }
            // Group by model name
            let mut by_model: BTreeMap<String, Vec<&BenchmarkRow>> = BTreeMap::new();
            for r in &rows {
                by_model.entry(r.model_name.clone()).or_default().push(r);
            }
            println!();
            println!("  ⬡ Benchmark Summary — All Models");
            println!("  ─────────────────────────────────────────────────────────────────────");
            println!("  Model{0:1$}│ Benchmarks │ Best tok/s", "", 40);
            println!("  ─────────────────────────────────────────────────────────────────────");
            for (name, benches) in &by_model {
                let count = benches.len();
                let best = benches
                    .iter()
                    .filter(|b| b.status == "ok")
                    .map(|b| b.tokens_per_sec)
                    .fold(0.0_f64, f64::max);
                let display_name = if name.len() > 44 {
                    format!("{}…", &name[..43])
                } else {
                    name.clone()
                };
                let pad = 46_usize.saturating_sub(display_name.len());
                if best > 0.0 {
                    println!("  {display_name}{0:1$}│ {count:>10} │ {best:>10.2}", "", pad);
                } else {
                    println!("  {display_name}{0:1$}│ {count:>10} │        n/a", "", pad);
                }
            }
            println!("  ─────────────────────────────────────────────────────────────────────");
            println!("  {} models, {} benchmarks total", by_model.len(), rows.len());
            println!();
        }
    }
    Ok(())
}

fn print_benchmark_table(model_name: &str, rows: &[BenchmarkRow]) {
    println!();
    println!("  ⬡ Benchmark History — {model_name}");
    println!("  ─────────────────────────────────────────────────────────────────────");
    println!("  #  │ Layers │ Context │ QKV │ Tokens/s │ TTFT    │ VRAM    │ Status");
    println!("  ─────────────────────────────────────────────────────────────────────");
    for (i, r) in rows.iter().enumerate() {
        let ttft = format!("{} ms", r.time_to_first_token_ms);
        let vram = format!("{} MB", r.vram_peak_mb);
        println!(
            "  {:<2} │ {:<6} │ {:<7} │ {:<3} │ {:<8.2} │ {:<7} │ {:<7} │ {}",
            i + 1,
            r.gpu_layers,
            r.context_size,
            r.quant_kv,
            r.tokens_per_sec,
            ttft,
            vram,
            r.status,
        );
    }
    println!("  ─────────────────────────────────────────────────────────────────────");
    println!("  {} benchmarks total", rows.len());
    println!();
}

/// A point on the Pareto frontier.
#[derive(Debug, Clone)]
struct ParetoPoint {
    context_size: u32,
    gpu_layers: i32,
    quant_kv: u32,
    tokens_per_sec: f64,
    vram_peak_mb: u32,
}

/// Compute the Pareto frontier from benchmark rows.
/// Dimensions: maximize tokens_per_sec, maximize context_size.
fn compute_pareto(rows: &[BenchmarkRow]) -> Vec<ParetoPoint> {
    // Filter to ok benchmarks only
    let ok_rows: Vec<&BenchmarkRow> = rows.iter().filter(|r| r.status == "ok").collect();
    if ok_rows.is_empty() {
        return Vec::new();
    }

    // For each context level, keep the fastest config
    let mut best_per_context: BTreeMap<u32, ParetoPoint> = BTreeMap::new();
    for r in &ok_rows {
        let entry = best_per_context.entry(r.context_size).or_insert(ParetoPoint {
            context_size: r.context_size,
            gpu_layers: r.gpu_layers,
            quant_kv: r.quant_kv,
            tokens_per_sec: r.tokens_per_sec,
            vram_peak_mb: r.vram_peak_mb,
        });
        if r.tokens_per_sec > entry.tokens_per_sec {
            *entry = ParetoPoint {
                context_size: r.context_size,
                gpu_layers: r.gpu_layers,
                quant_kv: r.quant_kv,
                tokens_per_sec: r.tokens_per_sec,
                vram_peak_mb: r.vram_peak_mb,
            };
        }
    }

    let mut candidates: Vec<ParetoPoint> = best_per_context.into_values().collect();

    // Remove dominated points.
    // A point is dominated if another point is >= on both (speed, context) and > on at least one.
    let snapshot = candidates.clone();
    candidates.retain(|p| {
        !snapshot.iter().any(|q| {
            (q.tokens_per_sec >= p.tokens_per_sec && q.context_size >= p.context_size)
                && (q.tokens_per_sec > p.tokens_per_sec || q.context_size > p.context_size)
        })
    });

    // Sort by context ascending for display
    candidates.sort_by_key(|p| p.context_size);
    candidates
}

/// Compute and display Pareto frontier for a model.
pub fn show_pareto(model_name: &str) -> Result<()> {
    let conn = db::open()?;
    let rows = db::get_benchmarks(&conn, model_name)?;
    let ok_count = rows.iter().filter(|r| r.status == "ok").count();

    if ok_count < 2 {
        println!("  Need at least 2 successful benchmarks to compute Pareto frontier.");
        println!("  Run `ozone bench` or `ozone sweep` with different configurations.");
        println!();
        return Ok(());
    }

    let frontier = compute_pareto(&rows);
    if frontier.is_empty() {
        println!("  No Pareto frontier could be computed.");
        return Ok(());
    }

    // Determine profile labels
    let labels = assign_profile_labels(&frontier);

    println!("  ⬡ Pareto Frontier — {model_name}");
    println!("  ─────────────────────────────────────────────────────────");
    println!("  Context  │ Layers │ QKV │ Tokens/s │ VRAM    │ Profile");
    println!("  ─────────────────────────────────────────────────────────");
    for (i, p) in frontier.iter().enumerate() {
        let vram = format!("{} MB", p.vram_peak_mb);
        let label = &labels[i];
        let profile_str = if label.is_empty() {
            String::new()
        } else {
            format!("★ {label}")
        };
        println!(
            "  {:<8} │ {:<6} │ {:<3} │ {:<8.2} │ {:<7} │ {}",
            p.context_size, p.gpu_layers, p.quant_kv, p.tokens_per_sec, vram, profile_str,
        );
    }
    println!("  ─────────────────────────────────────────────────────────");
    println!();

    Ok(())
}

/// Assign profile labels to Pareto frontier points.
fn assign_profile_labels(frontier: &[ParetoPoint]) -> Vec<String> {
    let mut labels = vec![String::new(); frontier.len()];
    if frontier.is_empty() {
        return labels;
    }
    if frontier.len() == 1 {
        labels[0] = "speed".to_string();
        return labels;
    }

    // Speed: highest tokens_per_sec
    let speed_idx = frontier
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.tokens_per_sec.partial_cmp(&b.tokens_per_sec).unwrap())
        .map(|(i, _)| i)
        .unwrap();

    // Context: largest context_size (frontier is sorted by context asc, so last)
    let context_idx = frontier.len() - 1;

    labels[speed_idx] = "speed".to_string();
    if context_idx != speed_idx {
        labels[context_idx] = "context".to_string();
    }

    // Balanced: median of frontier (excluding already-labeled if possible)
    if frontier.len() > 2 {
        let mid = frontier.len() / 2;
        if labels[mid].is_empty() {
            labels[mid] = "balanced".to_string();
        }
    }

    labels
}

/// Auto-generate profiles for a model from benchmark data.
pub fn generate_profiles(model_name: &str) -> Result<()> {
    let conn = db::open()?;
    let rows = db::get_benchmarks(&conn, model_name)?;
    let ok_count = rows.iter().filter(|r| r.status == "ok").count();

    if ok_count == 0 {
        bail!(
            "No successful benchmarks for '{model_name}'. Run `ozone bench` or `ozone sweep` first."
        );
    }

    let frontier = compute_pareto(&rows);
    if frontier.is_empty() {
        bail!("Could not compute Pareto frontier for '{model_name}'.");
    }

    let labels = assign_profile_labels(&frontier);
    let now = chrono::Local::now().to_rfc3339();

    // Clear existing auto-generated profiles
    db::clear_profiles(&conn, model_name)?;

    let mut generated = 0;
    for (i, p) in frontier.iter().enumerate() {
        if labels[i].is_empty() {
            continue;
        }
        let profile = ProfileRow {
            id: None,
            model_name: model_name.to_string(),
            profile_name: labels[i].clone(),
            gpu_layers: p.gpu_layers,
            context_size: p.context_size,
            quant_kv: p.quant_kv,
            tokens_per_sec: p.tokens_per_sec,
            vram_mb: p.vram_peak_mb,
            source: "auto".to_string(),
            created_at: now.clone(),
        };
        db::insert_profile(&conn, &profile)?;
        generated += 1;
    }

    println!();
    println!("  ⬡ Generated {generated} profile(s) for {model_name}");
    println!();

    Ok(())
}

/// Show stored profiles.
pub fn show_profiles(model: Option<&str>) -> Result<()> {
    let conn = db::open()?;

    match model {
        Some(name) => {
            let profiles = db::get_profiles(&conn, name)?;
            if profiles.is_empty() {
                println!();
                println!("  No profiles for '{name}'.");
                println!("  Run `ozone analyze --generate {name}` to auto-generate from benchmarks.");
                println!();
                return Ok(());
            }
            print_profiles_table(name, &profiles);
        }
        None => {
            // Show profiles for all models — fetch all benchmarks to get model names
            let rows = db::get_all_benchmarks(&conn)?;
            let mut model_names: Vec<String> = rows.iter().map(|r| r.model_name.clone()).collect();
            model_names.sort();
            model_names.dedup();

            if model_names.is_empty() {
                println!();
                println!("  No benchmarks found. Run `ozone bench` or `ozone sweep` first.");
                println!();
                return Ok(());
            }

            let mut any = false;
            for name in &model_names {
                let profiles = db::get_profiles(&conn, name)?;
                if !profiles.is_empty() {
                    print_profiles_table(name, &profiles);
                    any = true;
                }
            }
            if !any {
                println!();
                println!("  No profiles generated yet.");
                println!("  Run `ozone analyze --generate <model>` to auto-generate from benchmarks.");
                println!();
            }
        }
    }
    Ok(())
}

fn print_profiles_table(model_name: &str, profiles: &[ProfileRow]) {
    println!();
    println!("  ⬡ Profiles — {model_name}");
    println!("  ─────────────────────────────────────────────────────────────────");
    println!("  Name     │ Layers │ Context │ QKV │ Tokens/s │ VRAM    │ Source");
    println!("  ─────────────────────────────────────────────────────────────────");
    for p in profiles {
        let vram = format!("{} MB", p.vram_mb);
        println!(
            "  {:<8} │ {:<6} │ {:<7} │ {:<3} │ {:<8.2} │ {:<7} │ {}",
            p.profile_name, p.gpu_layers, p.context_size, p.quant_kv, p.tokens_per_sec, vram,
            p.source,
        );
    }
    println!("  ─────────────────────────────────────────────────────────────────");
    println!();
}
