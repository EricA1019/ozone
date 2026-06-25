//! Model file and run configuration hashing.
//!
//! Provides SHA-256 based identity for model files (fast: first 64KB + size)
//! and run configurations (stable format string). Foundation for gate caching
//! and result deduplication in the eval pipeline.

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

    let mut file = std::fs::File::open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;

    // Read up to HASH_READ_SIZE bytes
    let mut buffer = vec![0u8; HASH_READ_SIZE as usize];
    let bytes_read = file
        .read(&mut buffer)
        .with_context(|| format!("failed to read from {}", path.display()))?;
    buffer.truncate(bytes_read);

    let mut hasher = Sha256::new();
    hasher.update(&buffer);
    hasher.update(&file_size.to_be_bytes());

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

/// Hash a run configuration tuple into a stable 12-character hex string.
///
/// The hash is computed from a canonical format string of all parameters,
/// making it deterministic and order-independent. Two configs with identical
/// parameters will always produce the same hash regardless of how the
/// parameters were provided.
pub fn hash_run_config(
    model_hash: &str,
    backend: &str,
    quant: &str,
    kv_quant: &str,
    context_length: u32,
    batch_size: u32,
    gpu_layers: u32,
    threads: u32,
    sampler_profile: &str,
    seed: u64,
) -> String {
    let canonical = format!(
        "model_hash={}|backend={}|quant={}|kv_quant={}|ctx={}|batch={}|gpu={}|threads={}|sampler={}|seed={}",
        model_hash, backend, quant, kv_quant,
        context_length, batch_size, gpu_layers, threads,
        sampler_profile, seed,
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
        assert_ne!(hash_a, hash_b, "different sizes must produce different hashes");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_hash_run_config_deterministic() {
        let hash1 = hash_run_config(
            "abc123", "llama.cpp", "Q4_K_M", "q8_0",
            16384, 512, 35, 12, "coding_low_temp", 42,
        );
        let hash2 = hash_run_config(
            "abc123", "llama.cpp", "Q4_K_M", "q8_0",
            16384, 512, 35, 12, "coding_low_temp", 42,
        );
        assert_eq!(hash1, hash2, "same config must produce same hash");
        assert_eq!(hash1.len(), 12, "hash must be 12 hex chars");
    }

    #[test]
    fn test_hash_run_config_different_params_differ() {
        let hash1 = hash_run_config(
            "abc", "llama.cpp", "Q4_K_M", "q8_0",
            16384, 512, 35, 12, "default", 42,
        );
        let hash2 = hash_run_config(
            "abc", "llama.cpp", "Q4_K_M", "q8_0",
            32768, 512, 35, 12, "default", 42,
        );
        assert_ne!(hash1, hash2, "different context must produce different hash");
    }
}
