use std::{
    fs,
    io::{ErrorKind, Write},
    path::Path,
    time::Duration,
};

use rusqlite::Connection;

use crate::Result;

pub(super) fn open_connection(path: &Path) -> Result<(Connection, bool)> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        secure_path(parent, 0o700)?;
    }

    let existed_before_open = path
        .metadata()
        .map(|metadata| metadata.len() > 0)
        .unwrap_or(false);
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "foreign_keys", 1_i64)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.busy_timeout(Duration::from_secs(5))?;
    secure_path(path, 0o600)?;
    Ok((conn, existed_before_open))
}

pub(super) fn ensure_file_with_contents(path: &Path, contents: &str) -> Result<()> {
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            file.write_all(contents.as_bytes())?;
        }
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }

    secure_path(path, 0o600)?;
    Ok(())
}

#[cfg(unix)]
pub(super) fn secure_path(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if path.exists() {
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }

    Ok(())
}

#[cfg(not(unix))]
pub(super) fn secure_path(path: &Path, _mode: u32) -> Result<()> {
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::{ensure_file_with_contents, open_connection};

    static FS_HELPERS_COUNTER: AtomicU64 = AtomicU64::new(1);

    struct FsTestSandbox {
        root: PathBuf,
    }

    impl FsTestSandbox {
        fn new(prefix: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "ozone-persist-fs-{prefix}-{}-{}",
                std::process::id(),
                FS_HELPERS_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));

            if root.exists() {
                fs::remove_dir_all(&root).unwrap();
            }

            fs::create_dir_all(&root).unwrap();
            Self { root }
        }
    }

    impl Drop for FsTestSandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn fs_helpers_open_connection_initializes_sqlite_settings() {
        let sandbox = FsTestSandbox::new("open-connection");
        let db_path = sandbox.root.join("session.db");

        let (conn, existed_before_open) = open_connection(&db_path).expect("open sqlite db");
        assert!(!existed_before_open);

        let foreign_keys: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .expect("read foreign_keys");
        assert_eq!(foreign_keys, 1);

        let journal_mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("read journal_mode");
        assert_eq!(journal_mode.to_lowercase(), "wal");

        drop(conn);

        let (_, existed_before_open) = open_connection(&db_path).expect("reopen sqlite db");
        assert!(existed_before_open);
    }

    #[test]
    fn fs_helpers_ensure_file_with_contents_preserves_existing_text() {
        let sandbox = FsTestSandbox::new("ensure-file");
        let config_path = sandbox.root.join("config.toml");

        ensure_file_with_contents(&config_path, "alpha = 1\n").expect("create config");
        assert_eq!(fs::read_to_string(&config_path).unwrap(), "alpha = 1\n");

        ensure_file_with_contents(&config_path, "beta = 2\n").expect("preserve config");
        assert_eq!(fs::read_to_string(&config_path).unwrap(), "alpha = 1\n");
    }
}