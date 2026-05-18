use std::path::{Path, PathBuf};

use ozone_core::{paths as core_paths, session::SessionId};

use crate::{PersistError, Result};

pub(super) const DEFAULT_SESSION_CONFIG: &str = "[meta]\nconfig_version = 1\n";
pub(super) const DEFAULT_SESSION_DRAFT: &str = "";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistencePaths {
    data_dir: PathBuf,
}

impl PersistencePaths {
    pub fn from_data_dir(path: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: path.into(),
        }
    }

    pub fn from_xdg() -> Result<Self> {
        let data_dir = core_paths::data_dir().ok_or(PersistError::DataDirUnavailable)?;
        Ok(Self::from_data_dir(data_dir))
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn global_db_path(&self) -> PathBuf {
        self.data_dir.join("global.db")
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.data_dir.join("sessions")
    }

    pub fn session_dir(&self, session_id: &SessionId) -> PathBuf {
        self.sessions_dir().join(session_id.as_str())
    }

    pub fn session_db_path(&self, session_id: &SessionId) -> PathBuf {
        self.session_dir(session_id).join("session.db")
    }

    pub fn session_config_path(&self, session_id: &SessionId) -> PathBuf {
        self.session_dir(session_id).join("config.toml")
    }

    pub fn session_draft_path(&self, session_id: &SessionId) -> PathBuf {
        self.session_dir(session_id).join("draft.txt")
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ozone_core::session::SessionId;

    use super::{PersistencePaths, DEFAULT_SESSION_CONFIG, DEFAULT_SESSION_DRAFT};

    #[test]
    fn persistence_paths_build_expected_session_layout() {
        let paths = PersistencePaths::from_data_dir("/tmp/ozone-persist-test");
        let session_id = SessionId::parse("123e4567-e89b-12d3-a456-426614174000")
            .expect("valid session id");

        assert_eq!(paths.data_dir(), PathBuf::from("/tmp/ozone-persist-test").as_path());
        assert_eq!(
            paths.global_db_path(),
            PathBuf::from("/tmp/ozone-persist-test/global.db")
        );
        assert_eq!(
            paths.sessions_dir(),
            PathBuf::from("/tmp/ozone-persist-test/sessions")
        );
        assert_eq!(
            paths.session_dir(&session_id),
            PathBuf::from("/tmp/ozone-persist-test/sessions/123e4567-e89b-12d3-a456-426614174000")
        );
        assert_eq!(
            paths.session_db_path(&session_id),
            PathBuf::from(
                "/tmp/ozone-persist-test/sessions/123e4567-e89b-12d3-a456-426614174000/session.db"
            )
        );
        assert_eq!(
            paths.session_config_path(&session_id),
            PathBuf::from(
                "/tmp/ozone-persist-test/sessions/123e4567-e89b-12d3-a456-426614174000/config.toml"
            )
        );
        assert_eq!(
            paths.session_draft_path(&session_id),
            PathBuf::from(
                "/tmp/ozone-persist-test/sessions/123e4567-e89b-12d3-a456-426614174000/draft.txt"
            )
        );
    }

    #[test]
    fn persistence_paths_default_session_artifacts_are_stable() {
        assert_eq!(DEFAULT_SESSION_CONFIG, "[meta]\nconfig_version = 1\n");
        assert_eq!(DEFAULT_SESSION_DRAFT, "");
    }
}