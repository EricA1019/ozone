//! Model file and run configuration hashing.
//!
//! Provides SHA-256 based identity for model files (fast: first 64KB + size)
//! and run configurations (stable format string).
//!
//! # Usage
//!
//! - `hash_model_file` — identity for model caching. Same file = same hash.
//! - `hash_run_config` — **diagnostic only**. The hash is written into eval
//!   artifact logs for traceability. It is NOT used as a DB cache key or for
//!   deduplication. The `eval_run_configs` table schema includes a
//!   `config_hash` column but nothing writes to or reads from it.
//!
//!   Because the hash is diagnostic-only, the call sites in `runner.rs` pass
//!   some fixed default values (`batch_size: 512`, `seed: 42`) that are not
//!   currently configurable. This is harmless — if those fields become
//!   user-configurable in the future, the hashed values should be updated to
//!   match.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

/// Number of bytes to read from the beginning of a model file for hashing.
const HASH_READ_SIZE: u64 = 65_536;

/// Hash a model file's first 64KB + file size for fast identity.
///
/// Reads the first 65536 bytes and appends the file size as big-endian u64,
/// then SHA-256 hashes the combined data. Returns a hex string.
///
/// Same file = same hash regardless of path. Different sizes = different hashes.
/// This is NOT cryptographically collision-resistant for adversarial inputs,
/// but sufficient for cache-key identity.
pub fn hash_model_file(path: &Path) -> Result<String> {
    let metadata = std::fs::metadata(path)
        .with_context(|| format!("failed to read metadata for {}", path.display()))?;
    let file_size = metadata.len();

    let mut file =
        std::fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;

    // Read up to HASH_READ_SIZE bytes
    let mut buffer = vec![0u8; HASH_READ_SIZE as usize];
    let bytes_read = file
        .read(&mut buffer)
        .with_context(|| format!("failed to read from {}", path.display()))?;
    buffer.truncate(bytes_read);

    let mut hasher = Sha256::new();
    hasher.update(&buffer);
    hasher.update(file_size.to_be_bytes());

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

/// Hash a run configuration tuple into a stable 12-character hex string.
///
/// The hash is computed from a canonical format string of all parameters,
/// making it deterministic and order-independent. Two configs with identical
/// parameters will always produce the same hash regardless of how the
/// parameters were provided.
#[derive(Debug, Clone, Copy)]
pub struct RunConfigIdentity<'a> {
    pub model_hash: &'a str,
    pub backend: &'a str,
    pub quant: &'a str,
    pub kv_quant: &'a str,
    pub context_length: u32,
    pub batch_size: u32,
    pub gpu_layers: u32,
    pub threads: u32,
    pub sampler_profile: &'a str,
    pub seed: u64,
}

pub fn hash_run_config(config: &RunConfigIdentity<'_>) -> String {
    let canonical = format!(
        "model_hash={}|backend={}|quant={}|kv_quant={}|ctx={}|batch={}|gpu={}|threads={}|sampler={}|seed={}",
        config.model_hash, config.backend, config.quant, config.kv_quant,
        config.context_length, config.batch_size, config.gpu_layers, config.threads,
        config.sampler_profile, config.seed,
    );

    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    let result = hasher.finalize();

    // First 12 hex chars — sufficient for uniqueness in local use
    format!("{:x}", result)[..12].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_hash_model_file_deterministic() {
        let dir = std::env::temp_dir().join("ozone_hash_test");
        let _ = std::fs::create_dir_all(&dir);

        let path = dir.join("test_model.gguf");
        let content = vec![0xABu8; 100_000]; // Larger than HASH_READ_SIZE
        {
            let mut f = std::fs::File::create(&path).expect("create test file");
            f.write_all(&content).expect("write test file");
        }

        let hash1 = hash_model_file(&path).expect("first hash");
        let hash2 = hash_model_file(&path).expect("second hash");
        assert_eq!(hash1, hash2, "same file must produce same hash");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_hash_model_file_different_sizes_differ() {
        let dir = std::env::temp_dir().join("ozone_hash_test2");
        let _ = std::fs::create_dir_all(&dir);

        let path1 = dir.join("a.gguf");
        let path2 = dir.join("b.gguf");
        std::fs::write(&path1, vec![0xABu8; 1000]).expect("write a");
        std::fs::write(&path2, vec![0xABu8; 2000]).expect("write b");

        let hash_a = hash_model_file(&path1).expect("hash a");
        let hash_b = hash_model_file(&path2).expect("hash b");
        assert_ne!(
            hash_a, hash_b,
            "different sizes must produce different hashes"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_hash_run_config_deterministic() {
        let config = RunConfigIdentity {
            model_hash: "abc123",
            backend: "llama.cpp",
            quant: "Q4_K_M",
            kv_quant: "q8_0",
            context_length: 16384,
            batch_size: 512,
            gpu_layers: 35,
            threads: 12,
            sampler_profile: "coding_low_temp",
            seed: 42,
        };
        let hash1 = hash_run_config(&config);
        let hash2 = hash_run_config(&config);
        assert_eq!(hash1, hash2, "same config must produce same hash");
        assert_eq!(hash1.len(), 12, "hash must be 12 hex chars");
    }

    #[test]
    fn test_hash_run_config_different_params_differ() {
        let base = RunConfigIdentity {
            model_hash: "abc",
            backend: "llama.cpp",
            quant: "Q4_K_M",
            kv_quant: "q8_0",
            context_length: 16384,
            batch_size: 512,
            gpu_layers: 35,
            threads: 12,
            sampler_profile: "default",
            seed: 42,
        };
        let changed = RunConfigIdentity {
            context_length: 32768,
            ..base
        };
        let hash1 = hash_run_config(&base);
        let hash2 = hash_run_config(&changed);
        assert_ne!(
            hash1, hash2,
            "different context must produce different hash"
        );
    }
}
