use anyhow::Result;
use rusqlite::Connection;
use std::path::PathBuf;

/// Benchmark result row — one run of a specific configuration.
#[derive(Debug, Clone)]
pub struct BenchmarkRow {
    #[allow(dead_code)]
    pub id: Option<i64>,
    pub model_name: String,
    pub model_size_gb: f64,
    pub gpu_layers: i32,
    pub context_size: u32,
    pub quant_kv: u32,
    pub threads: u32,
    pub tokens_per_sec: f64,
    pub time_to_first_token_ms: u32,
    pub vram_peak_mb: u32,
    pub ram_peak_mb: u32,
    pub total_tokens: u32,
    pub total_time_ms: u32,
    pub status: String,
    pub gpu_name: String,
    pub gpu_vram_mb: u32,
    pub ram_total_mb: u32,
    pub timestamp: String,
    pub notes: String,
}

/// Auto-generated preset from benchmark data.
#[derive(Debug, Clone)]
pub struct ProfileRow {
    #[allow(dead_code)]
    pub id: Option<i64>,
    pub model_name: String,
    pub profile_name: String,
    pub gpu_layers: i32,
    pub context_size: u32,
    pub quant_kv: u32,
    pub tokens_per_sec: f64,
    pub vram_mb: u32,
    pub source: String,
    pub created_at: String,
}

fn db_path() -> PathBuf {
    let proj = directories::ProjectDirs::from("", "", "ozone")
        .expect("could not determine data directory");
    let dir = proj.data_dir();
    std::fs::create_dir_all(dir).ok();
    dir.join("benchmarks.db")
}

pub fn open() -> Result<Connection> {
    let path = db_path();
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
            quant_kv              INTEGER,
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

        CREATE TABLE IF NOT EXISTS profiles (
            id            INTEGER PRIMARY KEY,
            model_name    TEXT NOT NULL,
            profile_name  TEXT,
            gpu_layers    INTEGER,
            context_size  INTEGER,
            quant_kv      INTEGER,
            tokens_per_sec REAL,
            vram_mb       INTEGER,
            source        TEXT,
            created_at    TEXT
        );",
    )?;
    Ok(())
}

/// Insert a benchmark result. Returns the row id.
pub fn insert_benchmark(conn: &Connection, row: &BenchmarkRow) -> Result<i64> {
    conn.execute(
        "INSERT INTO benchmarks (
            model_name, model_size_gb, gpu_layers, context_size, quant_kv, threads,
            tokens_per_sec, time_to_first_token_ms, vram_peak_mb, ram_peak_mb,
            total_tokens, total_time_ms, status, gpu_name, gpu_vram_mb, ram_total_mb,
            timestamp, notes
        ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
        rusqlite::params![
            row.model_name, row.model_size_gb, row.gpu_layers, row.context_size,
            row.quant_kv, row.threads, row.tokens_per_sec, row.time_to_first_token_ms,
            row.vram_peak_mb, row.ram_peak_mb, row.total_tokens, row.total_time_ms,
            row.status, row.gpu_name, row.gpu_vram_mb, row.ram_total_mb,
            row.timestamp, row.notes,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Insert a profile preset.
pub fn insert_profile(conn: &Connection, row: &ProfileRow) -> Result<i64> {
    conn.execute(
        "INSERT INTO profiles (
            model_name, profile_name, gpu_layers, context_size, quant_kv,
            tokens_per_sec, vram_mb, source, created_at
        ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        rusqlite::params![
            row.model_name, row.profile_name, row.gpu_layers, row.context_size,
            row.quant_kv, row.tokens_per_sec, row.vram_mb, row.source, row.created_at,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Get all benchmarks for a model, ordered by timestamp desc.
pub fn get_benchmarks(conn: &Connection, model_name: &str) -> Result<Vec<BenchmarkRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, model_name, model_size_gb, gpu_layers, context_size, quant_kv, threads,
                tokens_per_sec, time_to_first_token_ms, vram_peak_mb, ram_peak_mb,
                total_tokens, total_time_ms, status, gpu_name, gpu_vram_mb, ram_total_mb,
                timestamp, notes
         FROM benchmarks WHERE model_name = ?1 ORDER BY timestamp DESC",
    )?;
    let rows = stmt.query_map([model_name], |r| {
        Ok(BenchmarkRow {
            id: Some(r.get(0)?),
            model_name: r.get(1)?,
            model_size_gb: r.get(2)?,
            gpu_layers: r.get(3)?,
            context_size: r.get(4)?,
            quant_kv: r.get(5)?,
            threads: r.get(6)?,
            tokens_per_sec: r.get(7)?,
            time_to_first_token_ms: r.get(8)?,
            vram_peak_mb: r.get(9)?,
            ram_peak_mb: r.get(10)?,
            total_tokens: r.get(11)?,
            total_time_ms: r.get(12)?,
            status: r.get(13)?,
            gpu_name: r.get(14)?,
            gpu_vram_mb: r.get(15)?,
            ram_total_mb: r.get(16)?,
            timestamp: r.get(17)?,
            notes: r.get(18)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Get all benchmarks across all models.
pub fn get_all_benchmarks(conn: &Connection) -> Result<Vec<BenchmarkRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, model_name, model_size_gb, gpu_layers, context_size, quant_kv, threads,
                tokens_per_sec, time_to_first_token_ms, vram_peak_mb, ram_peak_mb,
                total_tokens, total_time_ms, status, gpu_name, gpu_vram_mb, ram_total_mb,
                timestamp, notes
         FROM benchmarks ORDER BY timestamp DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(BenchmarkRow {
            id: Some(r.get(0)?),
            model_name: r.get(1)?,
            model_size_gb: r.get(2)?,
            gpu_layers: r.get(3)?,
            context_size: r.get(4)?,
            quant_kv: r.get(5)?,
            threads: r.get(6)?,
            tokens_per_sec: r.get(7)?,
            time_to_first_token_ms: r.get(8)?,
            vram_peak_mb: r.get(9)?,
            ram_peak_mb: r.get(10)?,
            total_tokens: r.get(11)?,
            total_time_ms: r.get(12)?,
            status: r.get(13)?,
            gpu_name: r.get(14)?,
            gpu_vram_mb: r.get(15)?,
            ram_total_mb: r.get(16)?,
            timestamp: r.get(17)?,
            notes: r.get(18)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Get profiles for a model, ordered by profile name.
pub fn get_profiles(conn: &Connection, model_name: &str) -> Result<Vec<ProfileRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, model_name, profile_name, gpu_layers, context_size, quant_kv,
                tokens_per_sec, vram_mb, source, created_at
         FROM profiles WHERE model_name = ?1 ORDER BY profile_name",
    )?;
    let rows = stmt.query_map([model_name], |r| {
        Ok(ProfileRow {
            id: Some(r.get(0)?),
            model_name: r.get(1)?,
            profile_name: r.get(2)?,
            gpu_layers: r.get(3)?,
            context_size: r.get(4)?,
            quant_kv: r.get(5)?,
            tokens_per_sec: r.get(6)?,
            vram_mb: r.get(7)?,
            source: r.get(8)?,
            created_at: r.get(9)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Delete all profiles for a model (before regenerating).
pub fn clear_profiles(conn: &Connection, model_name: &str) -> Result<()> {
    conn.execute("DELETE FROM profiles WHERE model_name = ?1", [model_name])?;
    Ok(())
}
