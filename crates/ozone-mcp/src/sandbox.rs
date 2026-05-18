use std::{
    collections::BTreeMap,
    path::PathBuf,
    process::Child,
};

use anyhow::Result;
use serde_json::json;
use serde_json::Value;

#[derive(Debug)]
pub struct Sandbox {
    pub id: String,
    pub root: PathBuf,
    pub data_home: PathBuf,
    pub home: PathBuf,
    pub models_dir: PathBuf,
    pub launcher_script: Option<PathBuf>,
    pub backend: Option<ManagedBackend>,
}

impl Sandbox {
    pub fn describe(&self) -> Value {
        json!({
            "sandboxId": self.id,
            "root": self.root,
            "dataHome": self.data_home,
            "home": self.home,
            "modelsDir": self.models_dir,
            "launcherScript": self.launcher_script,
            "backend": self.backend.as_ref().map(ManagedBackend::describe)
        })
    }

    pub fn env_overrides(&self) -> BTreeMap<String, String> {
        let mut env_map = BTreeMap::new();
        env_map.insert(
            "XDG_DATA_HOME".to_owned(),
            self.data_home.display().to_string(),
        );
        env_map.insert("HOME".to_owned(), self.home.display().to_string());
        env_map.insert(
            "OZONE_MODELS_DIR".to_owned(),
            self.models_dir.display().to_string(),
        );
        if let Some(path) = &self.launcher_script {
            env_map.insert(
                "OZONE_KOBOLDCPP_LAUNCHER".to_owned(),
                path.display().to_string(),
            );
        }
        env_map
    }

    pub fn command_env(&self) -> BTreeMap<String, String> {
        use std::env;

        let mut env_map = self.env_overrides();
        if let Ok(value) = env::var("CARGO_HOME") {
            env_map.insert("CARGO_HOME".to_owned(), value);
        } else if let Some(value) = super::host_toolchain_dir(".cargo") {
            env_map.insert("CARGO_HOME".to_owned(), value);
        }
        if let Ok(value) = env::var("RUSTUP_HOME") {
            env_map.insert("RUSTUP_HOME".to_owned(), value);
        } else if let Some(value) = super::host_toolchain_dir(".rustup") {
            env_map.insert("RUSTUP_HOME".to_owned(), value);
        }
        if let Some(backend) = &self.backend {
            env_map.insert("OZONE__BACKEND__TYPE".to_owned(), "koboldcpp".to_owned());
            env_map.insert("OZONE__BACKEND__URL".to_owned(), backend.base_url.clone());
        }
        // Carry the host's Python user-site path so pyte/Pillow remain findable
        // even when HOME is overridden to the sandbox home dir.
        if let Ok(pythonpath) = env::var("PYTHONPATH") {
            env_map.insert("PYTHONPATH".to_owned(), pythonpath);
        } else {
            // Derive it from the real HOME before the sandbox override takes effect.
            let real_home = env::var("HOME").unwrap_or_default();
            if !real_home.is_empty() {
                // Mirror Python's default user-site path: $HOME/.local/lib/pythonX.Y/site-packages
                // We don't know the exact X.Y so we glob the known prefix pattern.
                let user_site_base = format!("{real_home}/.local/lib");
                if let Ok(entries) = std::fs::read_dir(&user_site_base) {
                    let paths: Vec<String> = entries
                        .filter_map(|e| e.ok())
                        .filter(|e| e.file_name().to_string_lossy().starts_with("python"))
                        .map(|e| format!("{}/site-packages", e.path().display()))
                        .filter(|p| std::path::Path::new(p).exists())
                        .collect();
                    if !paths.is_empty() {
                        env_map.insert("PYTHONPATH".to_owned(), paths.join(":"));
                    }
                }
            }
        }
        env_map
    }

    pub fn stop_backend(&mut self) -> Result<bool> {
        let Some(mut backend) = self.backend.take() else {
            return Ok(false);
        };
        let _ = backend.child.kill();
        let _ = backend.child.wait();
        Ok(true)
    }
}

#[derive(Debug)]
pub struct ManagedBackend {
    pub child: Child,
    pub port: u16,
    pub model_name: String,
    pub base_url: String,
    pub log_path: PathBuf,
}

impl ManagedBackend {
    pub fn describe(&self) -> Value {
        json!({
            "pid": self.child.id(),
            "port": self.port,
            "modelName": self.model_name,
            "baseUrl": self.base_url,
            "logPath": self.log_path
        })
    }
}