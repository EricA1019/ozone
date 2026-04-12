use std::{
    fs,
    path::{Path, PathBuf},
};

use rusqlite::{params, Connection, Transaction};

use crate::{PersistError, Result};

pub struct Migration {
    pub version: u32,
    pub description: &'static str,
    pub apply: fn(&Transaction<'_>) -> rusqlite::Result<()>,
}

pub struct Migrator {
    pub migrations: &'static [Migration],
}

impl Migrator {
    pub fn latest_version(&self) -> u32 {
        self.migrations
            .last()
            .map(|migration| migration.version)
            .unwrap_or(0)
    }

    pub fn migrate(
        &self,
        conn: &mut Connection,
        path: &Path,
        existed_before_open: bool,
        applied_at: i64,
    ) -> Result<u32> {
        let current_version = current_version(conn)?;
        let latest_version = self.latest_version();

        if current_version > latest_version {
            return Err(PersistError::UnsupportedSchemaVersion(current_version));
        }

        if existed_before_open && current_version < latest_version {
            create_backup(path, current_version)?;
        }

        let mut previous_version = current_version;

        for migration in self
            .migrations
            .iter()
            .filter(|migration| migration.version > current_version)
        {
            let tx = conn.transaction()?;

            if let Err(error) = (migration.apply)(&tx) {
                return Err(PersistError::MigrationFailed {
                    version: previous_version,
                    target: migration.version,
                    reason: error.to_string(),
                });
            }

            tx.execute(
                "INSERT INTO schema_version (version, applied_at, description) VALUES (?1, ?2, ?3)",
                params![migration.version, applied_at, migration.description],
            )?;
            tx.commit()?;
            previous_version = migration.version;
        }

        Ok(latest_version)
    }
}

pub(crate) fn backup_path(path: &Path, version: u32) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("session.db");

    path.with_file_name(format!("{file_name}.bak.{version}"))
}

fn create_backup(path: &Path, version: u32) -> Result<()> {
    let backup_path = backup_path(path, version);

    if backup_path.exists() {
        fs::remove_file(&backup_path)?;
    }

    fs::copy(path, &backup_path)?;
    Ok(())
}

fn current_version(conn: &Connection) -> Result<u32> {
    if !table_exists(conn, "schema_version")? {
        return Ok(0);
    }

    let version = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get::<_, i64>(0),
    )?;

    u32::try_from(version)
        .map_err(|_| PersistError::InvalidData(format!("schema version {version} is invalid")))
}

fn table_exists(conn: &Connection, name: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [name],
        |row| row.get::<_, i64>(0),
    )
    .map(|exists| exists != 0)
}
