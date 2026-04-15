//! `ozone-core` — shared foundations for the Ozone product family.
//!
//! Provides product metadata, data/config path helpers (with env var overrides),
//! session identifiers, and engine domain types reused by all Ozone crates.

pub mod cli;
pub mod engine;
pub mod session;

pub mod product {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ProductTier {
        Ozonelite,
        Ozone,
        OzonePlus,
    }

    impl ProductTier {
        pub const fn display_name(self) -> &'static str {
            match self {
                Self::Ozonelite => "ozonelite",
                Self::Ozone => "ozone",
                Self::OzonePlus => "ozone+",
            }
        }

        pub const fn slug(self) -> &'static str {
            match self {
                Self::Ozonelite => "ozonelite",
                Self::Ozone => "ozone",
                Self::OzonePlus => "ozone-plus",
            }
        }

        pub const fn status_label(self) -> &'static str {
            match self {
                Self::Ozonelite => "Planned",
                Self::Ozone => "v0.4.0-alpha",
                Self::OzonePlus => "v0.4.0-alpha",
            }
        }
    }

    pub const OZONE_PLUS_DOC_PATH: &str = "ozone+/README.md";
    pub const OZONE_PLUS_DESIGN_DOC_PATH: &str = "ozone+/ozone_v0.4_design.md";
}

pub mod paths {
    use directories::ProjectDirs;
    use std::path::PathBuf;

    const GLOBAL_DB_FILE_NAME: &str = "global.db";
    const SESSIONS_DIR_NAME: &str = "sessions";
    const SESSION_DB_FILE_NAME: &str = "session.db";
    const SESSION_CONFIG_FILE_NAME: &str = "config.toml";
    const SESSION_DRAFT_FILE_NAME: &str = "draft.txt";

    const ENV_MODELS_DIR: &str = "OZONE_MODELS_DIR";
    const ENV_KOBOLDCPP_LAUNCHER: &str = "OZONE_KOBOLDCPP_LAUNCHER";
    const DEFAULT_KOBOLDCPP_PORT: u16 = 5001;

    fn project_dirs() -> Option<ProjectDirs> {
        ProjectDirs::from("", "", "ozone")
    }

    pub fn data_dir() -> Option<PathBuf> {
        project_dirs().map(|dirs| dirs.data_dir().to_path_buf())
    }

    /// Returns the model directory. Respects `OZONE_MODELS_DIR` env var,
    /// falls back to `~/models/`.
    pub fn models_dir() -> PathBuf {
        if let Ok(val) = std::env::var(ENV_MODELS_DIR) {
            return PathBuf::from(val);
        }
        dirs::home_dir()
            .map(|h| h.join("models"))
            .unwrap_or_else(|| PathBuf::from("models"))
    }

    /// Returns the preset file path inside the models directory.
    pub fn presets_path() -> PathBuf {
        models_dir().join("koboldcpp-presets.conf")
    }

    /// Returns the launch wrapper path. Respects `OZONE_KOBOLDCPP_LAUNCHER`,
    /// falls back to `~/models/launch-koboldcpp.sh`.
    pub fn launcher_path() -> PathBuf {
        if let Ok(val) = std::env::var(ENV_KOBOLDCPP_LAUNCHER) {
            return PathBuf::from(val);
        }
        models_dir().join("launch-koboldcpp.sh")
    }

    /// The default KoboldCpp API base URL.
    pub fn koboldcpp_base_url() -> String {
        format!("http://127.0.0.1:{DEFAULT_KOBOLDCPP_PORT}")
    }

    /// The default KoboldCpp ready-check endpoint.
    pub fn koboldcpp_ready_url() -> String {
        format!("http://127.0.0.1:{DEFAULT_KOBOLDCPP_PORT}/api/v1/model")
    }

    /// The default KoboldCpp perf endpoint.
    pub fn koboldcpp_perf_url() -> String {
        format!("http://127.0.0.1:{DEFAULT_KOBOLDCPP_PORT}/api/extra/perf")
    }

    /// The default KoboldCpp generate endpoint.
    pub fn koboldcpp_generate_url() -> String {
        format!("http://127.0.0.1:{DEFAULT_KOBOLDCPP_PORT}/api/v1/generate")
    }

    pub fn preferences_path() -> Option<PathBuf> {
        data_dir().map(|path| path.join("preferences.json"))
    }

    pub fn benchmarks_db_path() -> Option<PathBuf> {
        data_dir().map(|path| path.join("benchmarks.db"))
    }

    pub fn kobold_log_path() -> Option<PathBuf> {
        data_dir().map(|path| path.join("koboldcpp.log"))
    }

    pub fn global_db_path() -> Option<PathBuf> {
        data_dir().map(|path| path.join(GLOBAL_DB_FILE_NAME))
    }

    pub fn sessions_dir() -> Option<PathBuf> {
        data_dir().map(|path| path.join(SESSIONS_DIR_NAME))
    }

    pub fn session_dir(session_id: impl AsRef<str>) -> Option<PathBuf> {
        sessions_dir().map(|path| path.join(session_id.as_ref()))
    }

    pub fn session_db_path(session_id: impl AsRef<str>) -> Option<PathBuf> {
        let session_id = session_id.as_ref();
        session_dir(session_id).map(|path| path.join(SESSION_DB_FILE_NAME))
    }

    pub fn session_config_path(session_id: impl AsRef<str>) -> Option<PathBuf> {
        let session_id = session_id.as_ref();
        session_dir(session_id).map(|path| path.join(SESSION_CONFIG_FILE_NAME))
    }

    pub fn session_draft_path(session_id: impl AsRef<str>) -> Option<PathBuf> {
        let session_id = session_id.as_ref();
        session_dir(session_id).map(|path| path.join(SESSION_DRAFT_FILE_NAME))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::{
        paths,
        product::{ProductTier, OZONE_PLUS_DESIGN_DOC_PATH, OZONE_PLUS_DOC_PATH},
        session::SessionId,
    };

    #[test]
    fn product_tiers_expose_stable_metadata() {
        let cases = [
            (ProductTier::Ozonelite, "ozonelite", "ozonelite", "Planned"),
            (ProductTier::Ozone, "ozone", "ozone", "v0.4.0-alpha"),
            (
                ProductTier::OzonePlus,
                "ozone+",
                "ozone-plus",
                "v0.4.0-alpha",
            ),
        ];

        for (tier, display_name, slug, status_label) in cases {
            assert_eq!(tier.display_name(), display_name);
            assert_eq!(tier.slug(), slug);
            assert_eq!(tier.status_label(), status_label);
        }

        assert_eq!(OZONE_PLUS_DOC_PATH, "ozone+/README.md");
        assert_eq!(OZONE_PLUS_DESIGN_DOC_PATH, "ozone+/ozone_v0.4_design.md");
    }

    #[test]
    fn path_helpers_append_stable_suffixes() {
        let data_dir = paths::data_dir();
        let session_id = SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap();

        if let Some(path) = data_dir.as_ref() {
            assert!(path.ends_with(Path::new("ozone")));
        }

        assert_eq!(
            paths::preferences_path(),
            data_dir.clone().map(|path| path.join("preferences.json"))
        );
        assert_eq!(
            paths::benchmarks_db_path(),
            data_dir.clone().map(|path| path.join("benchmarks.db"))
        );
        assert_eq!(
            paths::kobold_log_path(),
            data_dir.clone().map(|path| path.join("koboldcpp.log"))
        );
        assert_eq!(
            paths::global_db_path(),
            data_dir.clone().map(|path| path.join("global.db"))
        );

        let expected_sessions_dir = data_dir.clone().map(|path| path.join("sessions"));

        assert_eq!(paths::sessions_dir(), expected_sessions_dir.clone());
        assert_eq!(
            paths::session_dir(&session_id),
            expected_sessions_dir
                .clone()
                .map(|path| path.join(session_id.as_str()))
        );
        assert_eq!(
            paths::session_db_path(&session_id),
            expected_sessions_dir
                .clone()
                .map(|path| path.join(session_id.as_str()).join("session.db"))
        );
        assert_eq!(
            paths::session_config_path(&session_id),
            expected_sessions_dir
                .clone()
                .map(|path| path.join(session_id.as_str()).join("config.toml"))
        );
        assert_eq!(
            paths::session_draft_path(&session_id),
            expected_sessions_dir.map(|path| path.join(session_id.as_str()).join("draft.txt"))
        );
    }
}
