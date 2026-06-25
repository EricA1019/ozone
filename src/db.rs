use anyhow::{Context, Result};
use ozone_core::paths;
use rusqlite::Connection;

/// Benchmark result row — one run of a specific configuration.
#[derive(Debug, Clone)]
pub struct BenchmarkRow {
    pub model_name: String,
    #[cfg(any(feature = "bench", feature = "profiling-ui"))]
    pub model_size_gb: f64,
    pub gpu_layers: i32,
    pub context_size: u32,
    pub quant_k: u32,
    /// V-cache quantization (defaults to quant_k when reading legacy rows)
    pub quant_v: u32,
    #[cfg(any(feature = "bench", feature = "profiling-ui"))]
    pub threads: u32,
    pub tokens_per_sec: f64,
    pub time_to_first_token_ms: u32,
    pub vram_peak_mb: u32,
    #[cfg(any(feature = "bench", feature = "profiling-ui"))]
    pub ram_peak_mb: u32,
    #[cfg(any(feature = "bench", feature = "profiling-ui"))]
    pub total_tokens: u32,
    #[cfg(any(feature = "bench", feature = "profiling-ui"))]
    pub total_time_ms: u32,
    pub status: String,
    #[cfg(any(feature = "bench", feature = "profiling-ui"))]
    pub gpu_name: String,
    #[cfg(any(feature = "bench", feature = "profiling-ui"))]
    pub gpu_vram_mb: u32,
    #[cfg(any(feature = "bench", feature = "profiling-ui"))]
    pub ram_total_mb: u32,
    #[cfg(any(feature = "bench", feature = "profiling-ui"))]
    pub timestamp: String,
    #[cfg(any(feature = "bench", feature = "profiling-ui"))]
    pub notes: String,
    #[cfg(any(feature = "bench", feature = "profiling-ui"))]
    pub launch_profile_name: Option<String>,
}

/// Auto-generated preset from benchmark data.
#[cfg(any(feature = "analyze", feature = "profiling-ui"))]
#[derive(Debug, Clone)]
pub struct ProfileRow {
    pub model_name: String,
    pub profile_name: String,
    pub gpu_layers: i32,
    pub context_size: u32,
    pub quant_k: u32,
    pub quant_v: u32,
    pub tokens_per_sec: f64,
    pub vram_mb: u32,
    pub source: String,
    pub created_at: String,
}

pub fn open() -> Result<Connection> {
    let path = paths::benchmarks_db_path()
        .context("Could not determine ozone data directory for benchmarks DB")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create benchmarks DB directory {}", parent.display())
        })?;
    }
    let conn = Connection::open(&path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    init_tables(&conn)?;
    Ok(conn)
}

fn init_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS benchmarks (
            id                    INTEGER PRIMARY KEY,
            model_name            TEXT NOT NULL,
            model_size_gb         REAL,
            gpu_layers            INTEGER,
            context_size          INTEGER,
            quant_k               INTEGER,
            quant_v               INTEGER,
            threads               INTEGER,
            tokens_per_sec        REAL,
            time_to_first_token_ms INTEGER,
            vram_peak_mb          INTEGER,
            ram_peak_mb           INTEGER,
            total_tokens          INTEGER,
            total_time_ms         INTEGER,
            status                TEXT,
            gpu_name              TEXT,
            gpu_vram_mb           INTEGER,
            ram_total_mb          INTEGER,
            timestamp             TEXT,
            notes                 TEXT,
            launch_profile_name   TEXT
        );

        CREATE TABLE IF NOT EXISTS profiles (
            id            INTEGER PRIMARY KEY,
            model_name    TEXT NOT NULL,
            profile_name  TEXT,
            gpu_layers    INTEGER,
            context_size  INTEGER,
            quant_k       INTEGER,
            quant_v       INTEGER DEFAULT 1,
            tokens_per_sec REAL,
            vram_mb       INTEGER,
            source        TEXT,
            created_at    TEXT
         );",
    )?;
    // Migration: rename quant_kv → quant_k, add quant_v column (older DBs).
    // All migration errors are non-fatal — the schema might not support RENAME
    // COLUMN (SQLite < 3.25.0), or the columns may already exist. The queries
    // use COALESCE(quant_v, quant_k) to handle any missing columns gracefully.
    let _ = conn.execute("ALTER TABLE benchmarks RENAME COLUMN quant_kv TO quant_k", []);
    let _ = conn.execute("ALTER TABLE benchmarks ADD COLUMN quant_v INTEGER DEFAULT 1", []);
    let _ = conn.execute("ALTER TABLE profiles RENAME COLUMN quant_kv TO quant_k", []);
    let _ = conn.execute("ALTER TABLE profiles ADD COLUMN quant_v INTEGER DEFAULT 1", []);
    let _ = conn.execute(
        "ALTER TABLE benchmarks ADD COLUMN launch_profile_name TEXT",
        [],
    );
    Ok(())
}

/// Insert a benchmark result. Returns the row id.
#[cfg(any(feature = "bench", feature = "profiling-ui"))]
pub fn insert_benchmark(conn: &Connection, row: &BenchmarkRow) -> Result<i64> {
    conn.execute(
        "INSERT INTO benchmarks (
            model_name, model_size_gb, gpu_layers, context_size, quant_k, quant_v, threads,
            tokens_per_sec, time_to_first_token_ms, vram_peak_mb, ram_peak_mb,
            total_tokens, total_time_ms, status, gpu_name, gpu_vram_mb, ram_total_mb,
            timestamp, notes, launch_profile_name
        ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",
        rusqlite::params![
            row.model_name,
            row.model_size_gb,
            row.gpu_layers,
            row.context_size,
            row.quant_k,
            row.quant_v,
            row.threads,
            row.tokens_per_sec,
            row.time_to_first_token_ms,
            row.vram_peak_mb,
            row.ram_peak_mb,
            row.total_tokens,
            row.total_time_ms,
            row.status,
            row.gpu_name,
            row.gpu_vram_mb,
            row.ram_total_mb,
            row.timestamp,
            row.notes,
            row.launch_profile_name,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Insert a profile preset.
#[cfg(any(feature = "analyze", feature = "profiling-ui"))]
pub fn insert_profile(conn: &Connection, row: &ProfileRow) -> Result<i64> {
    conn.execute(
        "INSERT INTO profiles (
            model_name, profile_name, gpu_layers, context_size, quant_k, quant_v,
            tokens_per_sec, vram_mb, source, created_at
        ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        rusqlite::params![
            row.model_name,
            row.profile_name,
            row.gpu_layers,
            row.context_size,
            row.quant_k,
            row.quant_v,
            row.tokens_per_sec,
            row.vram_mb,
            row.source,
            row.created_at,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Get all benchmarks for a model, ordered by timestamp desc.
#[cfg(any(feature = "analyze", feature = "profiling-ui", test))]
pub fn get_benchmarks(conn: &Connection, model_name: &str) -> Result<Vec<BenchmarkRow>> {
    let mut stmt = conn.prepare(
        "SELECT model_name, model_size_gb, gpu_layers, context_size, quant_k, COALESCE(quant_v, quant_k) as quant_v, threads,
                tokens_per_sec, time_to_first_token_ms, vram_peak_mb, ram_peak_mb,
                total_tokens, total_time_ms, status, gpu_name, gpu_vram_mb, ram_total_mb,
                timestamp, notes, launch_profile_name
         FROM benchmarks WHERE model_name = ?1 ORDER BY timestamp DESC",
    )?;
    let rows = stmt.query_map([model_name], |r| {
        Ok(BenchmarkRow {
            model_name: r.get(0)?,
            #[cfg(any(feature = "bench", feature = "profiling-ui"))]
            model_size_gb: r.get(1)?,
            gpu_layers: r.get(2)?,
            context_size: r.get(3)?,
            quant_k: r.get(4)?,
            quant_v: r.get(5)?,
            #[cfg(any(feature = "bench", feature = "profiling-ui"))]
            threads: r.get(6)?,
            tokens_per_sec: r.get(7)?,
            time_to_first_token_ms: r.get(8)?,
            vram_peak_mb: r.get(9)?,
            #[cfg(any(feature = "bench", feature = "profiling-ui"))]
            ram_peak_mb: r.get(10)?,
            #[cfg(any(feature = "bench", feature = "profiling-ui"))]
            total_tokens: r.get(11)?,
            #[cfg(any(feature = "bench", feature = "profiling-ui"))]
            total_time_ms: r.get(12)?,
            status: r.get(13)?,
            #[cfg(any(feature = "bench", feature = "profiling-ui"))]
            gpu_name: r.get(14)?,
            #[cfg(any(feature = "bench", feature = "profiling-ui"))]
            gpu_vram_mb: r.get(15)?,
            #[cfg(any(feature = "bench", feature = "profiling-ui"))]
            ram_total_mb: r.get(16)?,
            #[cfg(any(feature = "bench", feature = "profiling-ui"))]
            timestamp: r.get(17)?,
            #[cfg(any(feature = "bench", feature = "profiling-ui"))]
            notes: r.get(18)?,
            #[cfg(any(feature = "bench", feature = "profiling-ui"))]
            launch_profile_name: r.get(19)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Get all benchmarks across all models.
#[cfg(any(feature = "analyze", feature = "profiling-ui"))]
pub fn get_all_benchmarks(conn: &Connection) -> Result<Vec<BenchmarkRow>> {
    let mut stmt = conn.prepare(
        "SELECT model_name, model_size_gb, gpu_layers, context_size, quant_k, COALESCE(quant_v, quant_k) as quant_v, threads,
                tokens_per_sec, time_to_first_token_ms, vram_peak_mb, ram_peak_mb,
                total_tokens, total_time_ms, status, gpu_name, gpu_vram_mb, ram_total_mb,
                timestamp, notes, launch_profile_name
         FROM benchmarks ORDER BY timestamp DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(BenchmarkRow {
            model_name: r.get(0)?,
            #[cfg(any(feature = "bench", feature = "profiling-ui"))]
            model_size_gb: r.get(1)?,
            gpu_layers: r.get(2)?,
            context_size: r.get(3)?,
            quant_k: r.get(4)?,
            quant_v: r.get(5)?,
            #[cfg(any(feature = "bench", feature = "profiling-ui"))]
            threads: r.get(6)?,
            tokens_per_sec: r.get(7)?,
            time_to_first_token_ms: r.get(8)?,
            vram_peak_mb: r.get(9)?,
            #[cfg(any(feature = "bench", feature = "profiling-ui"))]
            ram_peak_mb: r.get(10)?,
            #[cfg(any(feature = "bench", feature = "profiling-ui"))]
            total_tokens: r.get(11)?,
            #[cfg(any(feature = "bench", feature = "profiling-ui"))]
            total_time_ms: r.get(12)?,
            status: r.get(13)?,
            #[cfg(any(feature = "bench", feature = "profiling-ui"))]
            gpu_name: r.get(14)?,
            #[cfg(any(feature = "bench", feature = "profiling-ui"))]
            gpu_vram_mb: r.get(15)?,
            #[cfg(any(feature = "bench", feature = "profiling-ui"))]
            ram_total_mb: r.get(16)?,
            #[cfg(any(feature = "bench", feature = "profiling-ui"))]
            timestamp: r.get(17)?,
            #[cfg(any(feature = "bench", feature = "profiling-ui"))]
            notes: r.get(18)?,
            #[cfg(any(feature = "bench", feature = "profiling-ui"))]
            launch_profile_name: r.get(19)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Get profiles for a model, ordered by profile name.
#[cfg(any(feature = "analyze", feature = "profiling-ui"))]
pub fn get_profiles(conn: &Connection, model_name: &str) -> Result<Vec<ProfileRow>> {
    let mut stmt = conn.prepare(
        "SELECT model_name, profile_name, gpu_layers, context_size, quant_k, COALESCE(quant_v, quant_k) as quant_v,
                tokens_per_sec, vram_mb, source, created_at
         FROM profiles WHERE model_name = ?1 ORDER BY profile_name",
    )?;
    let rows = stmt.query_map([model_name], |r| {
        Ok(ProfileRow {
            model_name: r.get(0)?,
            profile_name: r.get(1)?,
            gpu_layers: r.get(2)?,
            context_size: r.get(3)?,
            quant_k: r.get(4)?,
            quant_v: r.get(5)?,
            tokens_per_sec: r.get(6)?,
            vram_mb: r.get(7)?,
            source: r.get(8)?,
            created_at: r.get(9)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Delete all profiles for a model (before regenerating).
#[cfg(any(feature = "analyze", feature = "profiling-ui"))]
pub fn clear_profiles(conn: &Connection, model_name: &str) -> Result<()> {
    conn.execute("DELETE FROM profiles WHERE model_name = ?1", [model_name])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_benchmarks_schema_without_launch_profile_name(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE benchmarks (
                id                    INTEGER PRIMARY KEY,
                model_name            TEXT NOT NULL,
                model_size_gb         REAL,
                gpu_layers            INTEGER,
                context_size          INTEGER,
                quant_k               INTEGER,
            quant_v               INTEGER DEFAULT 1,
                threads               INTEGER,
                tokens_per_sec        REAL,
                time_to_first_token_ms INTEGER,
                vram_peak_mb          INTEGER,
                ram_peak_mb           INTEGER,
                total_tokens          INTEGER,
                total_time_ms         INTEGER,
                status                TEXT,
                gpu_name              TEXT,
                gpu_vram_mb           INTEGER,
                ram_total_mb          INTEGER,
                timestamp             TEXT,
                notes                 TEXT
            );

            CREATE TABLE profiles (
                id            INTEGER PRIMARY KEY,
                model_name    TEXT NOT NULL,
                profile_name  TEXT,
                gpu_layers    INTEGER,
                context_size  INTEGER,
                quant_kv      INTEGER,
            quant_v       INTEGER DEFAULT 1,
                tokens_per_sec REAL,
                vram_mb       INTEGER,
                source        TEXT,
                created_at    TEXT
             );",
        )
        .expect("create legacy schema");
    }

    #[cfg(any(feature = "bench", feature = "profiling-ui"))]
    #[test]
    fn benchmark_round_trip_preserves_launch_profile_name() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        init_tables(&conn).expect("init tables");

        let row = BenchmarkRow {
            model_name: "sample.gguf".into(),
            model_size_gb: 7.0,
            gpu_layers: 20,
            context_size: 16384,
            quant_k: 1,
            quant_v: 1,
            threads: 8,
            tokens_per_sec: 12.5,
            time_to_first_token_ms: 420,
            vram_peak_mb: 7800,
            ram_peak_mb: 6200,
            total_tokens: 100,
            total_time_ms: 8000,
            status: "ok".into(),
            gpu_name: "Test GPU".into(),
            gpu_vram_mb: 12000,
            ram_total_mb: 32000,
            timestamp: "2026-04-21T00:00:00+00:00".into(),
            notes: String::new(),
            launch_profile_name: Some("custom-1".into()),
        };

        insert_benchmark(&conn, &row).expect("insert benchmark");
        let rows = get_benchmarks(&conn, "sample.gguf").expect("get benchmarks");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].launch_profile_name.as_deref(), Some("custom-1"));
    }

    #[test]
    fn init_tables_is_reentrant_for_existing_schema() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");

        init_tables(&conn).expect("first init");
        init_tables(&conn).expect("second init");

        let column_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('benchmarks') WHERE name = 'launch_profile_name'",
                [],
                |row| row.get(0),
            )
            .expect("count launch_profile_name column");

        assert_eq!(column_count, 1);
    }

    #[test]
    fn init_tables_upgrades_legacy_schema_with_launch_profile_name() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        legacy_benchmarks_schema_without_launch_profile_name(&conn);

        init_tables(&conn).expect("upgrade legacy schema");

        let column_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('benchmarks') WHERE name = 'launch_profile_name'",
                [],
                |row| row.get(0),
            )
            .expect("count launch_profile_name column");

        assert_eq!(column_count, 1);
    }
}
