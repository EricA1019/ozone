use anyhow::{bail, Context, Result};
use clap::Subcommand;
use std::fs;
use std::io::{self, BufRead, Write};
use std::os::unix::fs as unix_fs;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum ModelCommand {
    /// List all local model files
    List,
    /// Add a model from HuggingFace, Ollama, or a local path
    Add {
        /// Download from HuggingFace Hub (repo id, e.g. TheBloke/model-GGUF)
        #[arg(long, value_name = "REPO")]
        hf: Option<String>,

        /// Link an Ollama model blob into ~/models/
        #[arg(long, value_name = "MODEL")]
        ollama: Option<String>,

        /// Symlink an existing file into ~/models/
        #[arg(long, value_name = "PATH")]
        link: Option<String>,

        /// Optional filename (used with --hf)
        #[arg(value_name = "FILENAME")]
        filename: Option<String>,
    },
    /// Remove a model from ~/models/
    Remove {
        /// Model filename to remove
        name: String,
    },
    /// Show detailed info about a model
    Info {
        /// Model filename to inspect
        name: String,
    },
}

/// Execute the model subcommand.
pub async fn run(command: ModelCommand) -> Result<()> {
    match command {
        ModelCommand::List => cmd_list().await,
        ModelCommand::Add {
            hf,
            ollama,
            link,
            filename,
        } => cmd_add(hf, ollama, link, filename).await,
        ModelCommand::Remove { name } => cmd_remove(&name),
        ModelCommand::Info { name } => cmd_info(&name).await,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn models_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    PathBuf::from(home).join("models")
}

fn human_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

/// Extract quant level from a GGUF filename stem.
/// Looks for patterns like Q4_K_M, Q5_K_S, Q8_0, IQ4_XS, Q2_K, Q6_K, etc.
fn parse_quant(stem: &str) -> Option<String> {
    let upper = stem.to_uppercase();

    // Try multi-token patterns: Q4_K_M, IQ4_XS, Q5_K_S, Q8_0, Q2_K, Q6_K, etc.
    // Strategy: scan for Qn or IQn then greedily consume _LETTER sequences
    let bytes = upper.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        // Find start of a quant token (Q or IQ followed by digit)
        let start = i;
        let is_iq = i + 2 < len && bytes[i] == b'I' && bytes[i + 1] == b'Q';
        let is_q = bytes[i] == b'Q' && !is_iq;

        if is_iq {
            i += 2;
        } else if is_q {
            i += 1;
        } else {
            i += 1;
            continue;
        }

        // Must be preceded by a delimiter or start-of-string
        if start > 0 {
            let prev = bytes[start - 1];
            if prev != b'-' && prev != b'_' && prev != b'.' && prev != b' ' {
                continue;
            }
        }

        // Must have a digit next
        if i >= len || !bytes[i].is_ascii_digit() {
            continue;
        }

        // Consume digits
        while i < len && bytes[i].is_ascii_digit() {
            i += 1;
        }

        // Consume underscore-separated suffix tokens (K, M, S, XS, L, 0, etc.)
        loop {
            if i >= len || bytes[i] != b'_' {
                break;
            }
            let peek = i + 1;
            if peek >= len {
                break;
            }
            // The suffix token should be alphanumeric and short
            let mut j = peek;
            while j < len && (bytes[j].is_ascii_alphanumeric()) {
                j += 1;
            }
            let token_len = j - peek;
            if token_len == 0 || token_len > 3 {
                break;
            }
            // Must end at a delimiter or end-of-string
            if j < len
                && bytes[j] != b'-'
                && bytes[j] != b'_'
                && bytes[j] != b'.'
                && bytes[j] != b' '
            {
                // Part of a longer token — not a quant suffix
                break;
            }
            i = j;
        }

        // Must end at a delimiter or end-of-string
        if i < len && bytes[i] != b'-' && bytes[i] != b'_' && bytes[i] != b'.' && bytes[i] != b' ' {
            continue;
        }

        let quant = &upper[start..i];
        // Validate it looks reasonable (at least Q + digit)
        if quant.len() >= 2 {
            // Return original-case by slicing the original stem
            return Some(stem[start..i].to_uppercase());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// list
// ---------------------------------------------------------------------------

struct ModelEntry {
    name: String,
    size: u64,
    is_symlink: bool,
    link_target: Option<PathBuf>,
    quant: Option<String>,
}

async fn scan_models(dir: &Path) -> Result<Vec<ModelEntry>> {
    let mut entries = Vec::new();
    let mut rd = tokio::fs::read_dir(dir)
        .await
        .with_context(|| format!("Cannot read {}", dir.display()))?;
    while let Some(entry) = rd.next_entry().await? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".gguf") {
            continue;
        }
        let path = entry.path();
        let sym_meta = tokio::fs::symlink_metadata(&path).await?;
        let is_symlink = sym_meta.file_type().is_symlink();
        let link_target = if is_symlink {
            tokio::fs::read_link(&path).await.ok()
        } else {
            None
        };
        // Follow symlinks for real size
        let size = match tokio::fs::metadata(&path).await {
            Ok(m) => m.len(),
            Err(_) => 0, // broken symlink
        };
        let stem = name.trim_end_matches(".gguf");
        let quant = parse_quant(stem);
        entries.push(ModelEntry {
            name,
            size,
            is_symlink,
            link_target,
            quant,
        });
    }
    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(entries)
}

async fn cmd_list() -> Result<()> {
    let dir = models_dir();
    let entries = scan_models(&dir).await?;

    if entries.is_empty() {
        println!("No .gguf models found in {}", dir.display());
        return Ok(());
    }

    let max_name = entries.iter().map(|e| e.name.len()).max().unwrap_or(20);
    let col_name = max_name.max(4);

    println!("Models in ~/models/");
    println!(
        "  {:<col_name$}  {:>8}  {:<8}  TYPE",
        "NAME", "SIZE", "QUANT"
    );
    for e in &entries {
        let size = human_size(e.size);
        let quant = e.quant.as_deref().unwrap_or("—");
        let kind = if e.is_symlink {
            if let Some(ref target) = e.link_target {
                format!("symlink → {}", target.display())
            } else {
                "symlink".into()
            }
        } else {
            "file".into()
        };
        println!(
            "  {:<col_name$}  {:>8}  {:<8}  {}",
            e.name, size, quant, kind
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// add
// ---------------------------------------------------------------------------

async fn cmd_add(
    hf: Option<String>,
    ollama: Option<String>,
    link: Option<String>,
    filename: Option<String>,
) -> Result<()> {
    let set_count = [hf.is_some(), ollama.is_some(), link.is_some()]
        .iter()
        .filter(|&&v| v)
        .count();
    if set_count != 1 {
        bail!("Exactly one of --hf, --ollama, or --link must be provided.");
    }

    if let Some(repo) = hf {
        add_hf(&repo, filename.as_deref()).await
    } else if let Some(model) = ollama {
        add_ollama(&model)
    } else if let Some(path) = link {
        add_link(&path)
    } else {
        unreachable!()
    }
}

async fn add_hf(repo: &str, filename: Option<&str>) -> Result<()> {
    let dir = models_dir();
    fs::create_dir_all(&dir)?;

    let hf_bin = PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/bin/hf");
    if !hf_bin.exists() {
        bail!(
            "HuggingFace CLI not found at {}. Install with: pip install huggingface_hub[cli]",
            hf_bin.display()
        );
    }

    let mut cmd = std::process::Command::new(&hf_bin);
    cmd.arg("download").arg(repo);
    if let Some(fname) = filename {
        cmd.arg(fname);
    } else {
        cmd.args(["--include", "*.gguf"]);
    }
    cmd.arg("--local-dir").arg(&dir);

    println!("Running: {} {}", hf_bin.display(), {
        let mut args = vec!["download".to_string(), repo.to_string()];
        if let Some(f) = filename {
            args.push(f.to_string());
        } else {
            args.push("--include".into());
            args.push("*.gguf".into());
        }
        args.push("--local-dir".into());
        args.push(dir.display().to_string());
        args.join(" ")
    });

    let status = cmd.status().context("Failed to run hf CLI")?;
    if !status.success() {
        bail!("hf download exited with {}", status);
    }
    println!("✓ Download complete.");
    Ok(())
}

fn add_ollama(model: &str) -> Result<()> {
    let home = std::env::var("HOME").unwrap_or_default();
    let model_dir = models_dir();
    fs::create_dir_all(&model_dir)?;

    // Parse model name: may include tag like "deepseek-coder:6.7b"
    let (name, tag) = if let Some(pos) = model.find(':') {
        (&model[..pos], &model[pos + 1..])
    } else {
        (model, "latest")
    };

    let manifest_path = PathBuf::from(&home)
        .join(".ollama/models/manifests/registry.ollama.ai/library")
        .join(name)
        .join(tag);

    if !manifest_path.exists() {
        bail!(
            "Ollama manifest not found at {}. Is '{}' pulled?",
            manifest_path.display(),
            model
        );
    }

    let manifest_text =
        fs::read_to_string(&manifest_path).context("Failed to read Ollama manifest")?;
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).context("Invalid manifest JSON")?;

    // Find the model layer (mediaType containing "model")
    let layers = manifest["layers"]
        .as_array()
        .context("Manifest has no layers array")?;
    let model_layer = layers
        .iter()
        .find(|l| {
            l["mediaType"]
                .as_str()
                .map(|m| m.contains("model"))
                .unwrap_or(false)
        })
        .context("No model layer found in manifest")?;

    let digest = model_layer["digest"]
        .as_str()
        .context("No digest in model layer")?;

    // Digest looks like "sha256:<hex>"
    let blob_name = digest.replace(':', "-");
    let blob_path = PathBuf::from(&home)
        .join(".ollama/models/blobs")
        .join(&blob_name);

    if !blob_path.exists() {
        bail!("Ollama blob not found at {}", blob_path.display());
    }

    let link_name = format!("{}.gguf", model.replace(':', "-"));
    let link_path = model_dir.join(&link_name);

    if link_path.exists() {
        bail!("{} already exists in ~/models/", link_name);
    }

    unix_fs::symlink(&blob_path, &link_path)
        .with_context(|| format!("Failed to create symlink {}", link_path.display()))?;
    println!("✓ Linked {} → {}", link_name, blob_path.display());
    Ok(())
}

fn add_link(path: &str) -> Result<()> {
    let source = PathBuf::from(path)
        .canonicalize()
        .with_context(|| format!("Path not found: {path}"))?;
    if !source.is_file() {
        bail!("{} is not a file", source.display());
    }

    let model_dir = models_dir();
    fs::create_dir_all(&model_dir)?;

    let fname = source.file_name().context("Cannot determine filename")?;
    let link_path = model_dir.join(fname);

    if link_path.exists() {
        bail!("{} already exists in ~/models/", fname.to_string_lossy());
    }

    unix_fs::symlink(&source, &link_path)
        .with_context(|| format!("Failed to create symlink {}", link_path.display()))?;
    println!(
        "✓ Linked {} → {}",
        fname.to_string_lossy(),
        source.display()
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// remove
// ---------------------------------------------------------------------------

fn cmd_remove(name: &str) -> Result<()> {
    let path = models_dir().join(name);
    if !path.exists() {
        bail!("Model not found: {}", path.display());
    }

    let sym_meta = fs::symlink_metadata(&path)?;
    let is_symlink = sym_meta.file_type().is_symlink();

    if is_symlink {
        fs::remove_file(&path)?;
        println!("✓ Removed symlink {name}");
    } else {
        let size = sym_meta.len();
        print!(
            "Remove {name} ({})? This cannot be undone. [y/N] ",
            human_size(size)
        );
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().lock().read_line(&mut answer)?;
        if answer.trim().eq_ignore_ascii_case("y") {
            fs::remove_file(&path)?;
            println!("✓ Removed {name}");
        } else {
            println!("Skipped.");
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// info
// ---------------------------------------------------------------------------

async fn cmd_info(name: &str) -> Result<()> {
    let path = models_dir().join(name);
    if !path.exists() {
        bail!("Model not found: {}", path.display());
    }

    let sym_meta = fs::symlink_metadata(&path)?;
    let is_symlink = sym_meta.file_type().is_symlink();
    let link_target = if is_symlink {
        fs::read_link(&path).ok()
    } else {
        None
    };

    // Follow symlink for real size
    let real_meta = fs::metadata(&path).ok();
    let size = real_meta.as_ref().map(|m| m.len()).unwrap_or(0);

    let created = real_meta
        .as_ref()
        .and_then(|m| m.created().ok())
        .map(|t| {
            let dt: chrono::DateTime<chrono::Local> = t.into();
            dt.format("%Y-%m-%d %H:%M").to_string()
        })
        .unwrap_or_else(|| "unknown".into());

    let stem = name.trim_end_matches(".gguf");
    let quant = parse_quant(stem).unwrap_or_else(|| "—".into());

    println!();
    println!("  Model:    {name}");
    println!("  Path:     {}", path.display());
    println!("  Size:     {}", human_size(size));
    println!("  Created:  {created}");
    println!("  Quant:    {quant}");
    if is_symlink {
        if let Some(ref target) = link_target {
            println!("  Type:     symlink → {}", target.display());
        } else {
            println!("  Type:     symlink (broken)");
        }
    } else {
        println!("  Type:     file");
    }

    // Check KoboldCpp
    match check_koboldcpp_model(name).await {
        Some(true) => println!("  KoboldCpp: loaded ✓"),
        Some(false) => println!("  KoboldCpp: running (different model)"),
        None => {}
    }

    println!();
    Ok(())
}

/// Check if KoboldCpp is running on port 5001 and if this model is loaded.
/// Returns Some(true) if loaded, Some(false) if running but different model, None if not running.
async fn check_koboldcpp_model(model_name: &str) -> Option<bool> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .ok()?;
    let resp = client
        .get("http://localhost:5001/api/v1/model")
        .send()
        .await
        .ok()?;
    let body: serde_json::Value = resp.json().await.ok()?;
    let loaded = body["result"].as_str().unwrap_or_default();
    Some(loaded.contains(model_name.trim_end_matches(".gguf")))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quant_parsing() {
        assert_eq!(parse_quant("Harbinger-24B-Q4_K_M"), Some("Q4_K_M".into()));
        assert_eq!(
            parse_quant("Deepseek-Coder-V2-16B-Q5_K_S"),
            Some("Q5_K_S".into())
        );
        assert_eq!(parse_quant("some-model-Q8_0"), Some("Q8_0".into()));
        assert_eq!(parse_quant("model-IQ4_XS"), Some("IQ4_XS".into()));
        assert_eq!(parse_quant("model-Q2_K"), Some("Q2_K".into()));
        assert_eq!(parse_quant("model-Q6_K"), Some("Q6_K".into()));
        assert_eq!(parse_quant("mn-12b-mag-mell-r1"), None);
    }

    #[test]
    fn human_size_formatting() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1024), "1.0 KB");
        assert_eq!(human_size(1_073_741_824), "1.0 GB");
        assert_eq!(human_size(7_843_348_480), "7.3 GB");
        assert_eq!(human_size(536_870_912), "512.0 MB");
    }
}
