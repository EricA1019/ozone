use anyhow::{bail, Context, Result};
#[cfg(feature = "model-mgmt")]
use directories::BaseDirs;
use std::env;
use std::path::PathBuf;

#[cfg(feature = "model-mgmt")]
use std::path::Path;

#[cfg(feature = "model-mgmt")]
pub const ENV_LLAMACPP_CLI: &str = "OZONE_LLAMACPP_CLI";
pub const ENV_LLAMACPP_SERVER: &str = "OZONE_LLAMACPP_SERVER";

#[cfg(feature = "model-mgmt")]
pub fn discover_llama_cli_binary() -> Result<PathBuf> {
    discover_binary(ENV_LLAMACPP_CLI, &["llama-cli"])
}

pub fn discover_llama_server_binary() -> Result<PathBuf> {
    discover_binary(ENV_LLAMACPP_SERVER, &["llama-server"])
}

fn discover_binary(override_env: &str, candidates: &[&str]) -> Result<PathBuf> {
    if let Ok(value) = env::var(override_env) {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            bail!("{override_env} is set but empty.");
        }
        return resolve_binary(trimmed).with_context(|| {
            format!("{override_env} points to '{trimmed}', but that executable was not found")
        });
    }

    for candidate in candidates {
        if let Ok(path) = resolve_binary(candidate) {
            return Ok(path);
        }
    }

    bail!(
        "{} not found on PATH. Set {}=/path/to/{} to use a local install.",
        candidates.join(" / "),
        override_env,
        candidates[0]
    );
}

fn resolve_binary(candidate: &str) -> Result<PathBuf> {
    let path = PathBuf::from(candidate);
    if path.components().count() > 1 || path.is_absolute() {
        if path.is_file() {
            return Ok(path);
        }
        bail!("{} does not exist", path.display());
    }

    let path_env = env::var_os("PATH").context("PATH is not set")?;
    for dir in env::split_paths(&path_env) {
        let full = dir.join(candidate);
        if full.is_file() {
            return Ok(full);
        }
    }

    bail!("'{candidate}' was not found on PATH");
}

#[cfg(feature = "model-mgmt")]
pub fn hugging_face_cache_root() -> PathBuf {
    if let Some(path) = env_path("HF_HUB_CACHE") {
        return path;
    }
    if let Some(path) = env_path("HUGGINGFACE_HUB_CACHE") {
        return path;
    }
    if let Some(path) = env_path("HF_HOME") {
        return path.join("hub");
    }
    BaseDirs::new()
        .map(|dirs| dirs.cache_dir().join("huggingface").join("hub"))
        .unwrap_or_else(|| PathBuf::from(".cache/huggingface/hub"))
}

#[cfg(feature = "model-mgmt")]
pub fn hugging_face_repo_cache_dir(repo: &str) -> PathBuf {
    let repo_slug = repo.replace('/', "--");
    hugging_face_cache_root().join(format!("models--{repo_slug}"))
}

#[cfg(feature = "model-mgmt")]
pub fn model_library_target(source: &Path, preferred_name: Option<&str>) -> Result<PathBuf> {
    let filename = match preferred_name {
        Some(name) => PathBuf::from(name),
        None => source
            .file_name()
            .map(PathBuf::from)
            .context("cannot determine model filename from source path")?,
    };
    Ok(ozone_core::paths::models_dir().join(filename))
}

#[cfg(feature = "model-mgmt")]
fn env_path(name: &str) -> Option<PathBuf> {
    let value = env::var_os(name)?;
    if value.is_empty() {
        return None;
    }
    Some(PathBuf::from(value))
}

#[cfg(test)]
#[cfg(feature = "model-mgmt")]
mod tests {
    use super::*;

    #[test]
    fn repo_cache_dir_matches_hf_layout() {
        let path = hugging_face_repo_cache_dir("ggml-org/gemma-3-1b-it-GGUF");
        assert!(path.ends_with("models--ggml-org--gemma-3-1b-it-GGUF"));
    }

    #[test]
    fn model_library_target_uses_preferred_name_when_given() {
        let target =
            model_library_target(Path::new("/tmp/model.gguf"), Some("alias.gguf")).unwrap();
        assert!(target.ends_with("alias.gguf"));
    }

    #[test]
    fn model_library_target_falls_back_to_source_filename() {
        let target = model_library_target(Path::new("/tmp/model.gguf"), None).unwrap();
        assert!(target.ends_with("model.gguf"));
    }
}
