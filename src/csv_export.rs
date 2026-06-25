#![allow(dead_code)]
//! CSV export for eval results.
//!
//! Reads from the eval database tables and writes CSV files:
//! - ozone_models.csv
//! - ozone_run_configs.csv
//! - ozone_runs.csv
//! - ozone_task_results.csv
//! - ozone_gate_results.csv
//! - ozone_skipped.csv

use anyhow::{Context, Result};
use std::path::Path;

/// Export all eval tables to CSV files in the given directory.
pub fn export_all_csv(output_dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    let conn = crate::db::open()?;

    files.push(export_table(&conn, "eval_models",
        "id, name, family, parameter_count, model_path, model_hash, quant, file_size_bytes, notes, created_at",
        output_dir, "ozone_models.csv")?);

    files.push(export_table(&conn, "eval_run_configs",
        "id, model_id, config_hash, backend, backend_version, quant, kv_quant, context_length, batch_size, threads, gpu_layers, sampler_profile, seed, created_at",
        output_dir, "ozone_run_configs.csv")?);

    files.push(export_table(&conn, "eval_runs",
        "id, run_config_id, started_at, finished_at, status, eval_mode, min_quality_ctx, warmup_enabled, notes",
        output_dir, "ozone_runs.csv")?);

    files.push(export_table(&conn, "eval_task_results",
        "id, run_id, task_key, suite_name, lane, status, score, passed, latency_ms, prompt_tokens, completion_tokens, total_tokens, tok_per_sec, timeout_seconds, failure_type, response_path, artifact_path, attempt_index, created_at",
        output_dir, "ozone_task_results.csv")?);

    files.push(export_table(&conn, "eval_gate_results",
        "id, run_id, lane, gate_name, score, decision, required_score, reason, cache_hit, created_at",
        output_dir, "ozone_gate_results.csv")?);

    files.push(export_table(&conn, "eval_skipped",
        "id, run_id, suite_name, lane, tier, reason, failed_gate, actual_score, required_score, created_at",
        output_dir, "ozone_skipped.csv")?);

    Ok(files)
}

/// Export a single table to CSV.
fn export_table(
    conn: &rusqlite::Connection,
    table_name: &str,
    column_list: &str,
    output_dir: &Path,
    filename: &str,
) -> Result<std::path::PathBuf> {
    use std::io::Write;

    let out_path = output_dir.join(filename);
    let mut file = std::fs::File::create(&out_path)
        .with_context(|| format!("creating {filename}"))?;

    // Write header
    writeln!(file, "{}", column_list)?;

    // Write rows
    let sql = format!("SELECT {} FROM {} ORDER BY id", column_list, table_name);
    let mut stmt = conn.prepare(&sql)?;

    let rows = stmt.query_map([], |row| {
        let mut values = Vec::new();
        for i in 0..row.as_ref().column_count() {
            let val = match row.get::<_, rusqlite::types::Value>(i) {
                Ok(rusqlite::types::Value::Null) => String::new(),
                Ok(rusqlite::types::Value::Integer(n)) => n.to_string(),
                Ok(rusqlite::types::Value::Real(f)) => f.to_string(),
                Ok(rusqlite::types::Value::Text(s)) => {
                    let needs_quoting = s.contains(',') || s.contains('"') || s.contains('\n');
                    if needs_quoting {
                        format!("\"{}\"", s.replace('"', "\"\""))
                    } else {
                        s
                    }
                }
                Ok(rusqlite::types::Value::Blob(_)) => "[blob]".into(),
                Err(_) => String::new(),
            };
            values.push(val);
        }
        Ok(values)
    })?;

    for row in rows {
        let values = row?;
        writeln!(file, "{}", values.join(","))?;
    }

    Ok(out_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_export_all_csv_creates_files() {
        let tmp = std::env::temp_dir().join("ozone_csv_e2e_test");
        let _ = std::fs::create_dir_all(&tmp);
        let result = export_all_csv(&tmp);
        assert!(result.is_ok(), "export failed: {:?}", result.err());
        let files = result.unwrap();
        assert!(!files.is_empty(), "expected at least one CSV file");
        for f in &files {
            assert!(f.exists(), "file does not exist: {:?}", f);
            let meta = std::fs::metadata(f).unwrap();
            assert!(meta.len() > 0, "file is empty: {:?}", f);
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_csv_quoting() {
        let tmp = std::env::temp_dir().join("ozone_csv_quoting_test");
        let _ = std::fs::create_dir_all(&tmp);
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, val TEXT);
             INSERT INTO t VALUES (1, 'hello');
             INSERT INTO t VALUES (2, 'comma, here');
             INSERT INTO t VALUES (3, 'plain');
             ").unwrap();

        let out = tmp.join("t.csv");
        let mut f = std::fs::File::create(&out).unwrap();
        writeln!(f, "id,val").unwrap();
        let sql = "SELECT id, val FROM t ORDER BY id";
        let mut stmt = conn.prepare(sql).unwrap();
        let rows = stmt.query_map([], |row| {
            let mut vals = Vec::new();
            for i in 0..row.as_ref().column_count() {
                let v = match row.get::<_, rusqlite::types::Value>(i) {
                    Ok(rusqlite::types::Value::Null) => String::new(),
                    Ok(rusqlite::types::Value::Integer(n)) => n.to_string(),
                    Ok(rusqlite::types::Value::Real(f)) => f.to_string(),
                    Ok(rusqlite::types::Value::Text(s)) => {
                        if s.contains(',') || s.contains('"') {
                            format!("\"{}\"", s.replace('"', "\"\""))
                        } else {
                            s
                        }
                    }
                    Ok(rusqlite::types::Value::Blob(_)) => "[blob]".into(),
                    Err(_) => String::new(),
                };
                vals.push(v);
            }
            Ok(vals)
        }).unwrap();
        for row in rows {
            let vals = row.unwrap();
            writeln!(f, "{}", vals.join(",")).unwrap();
        }

        let content = std::fs::read_to_string(&out).unwrap();
        assert!(content.contains("1,hello"), "missing normal row: {}", content);
        assert!(content.contains("2,\"comma, here\""), "missing quoted: {}", content);
        assert!(content.contains("3,plain"), "missing row 3: {}", content);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
