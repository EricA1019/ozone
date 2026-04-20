use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::{self, BufRead, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

const ENV_SKIP_INSTALL_UPDATE_PROMPT: &str = "OZONE_SKIP_INSTALL_UPDATE_PROMPT";
const ENV_INSTALL_UPDATE_PROMPTED: &str = "OZONE_INSTALL_UPDATE_PROMPTED";
const ENV_DEBUG_INSTALL_CHECK: &str = "OZONE_DEBUG_INSTALL_CHECK";

fn debug(msg: &str) {
    if env_truthy(ENV_DEBUG_INSTALL_CHECK) {
        eprintln!("[install-check] {msg}");
    }
}

struct PendingInstallUpdate {
    repo_root: PathBuf,
    sync_script: PathBuf,
    source_artifact: PathBuf,
}

pub fn maybe_prompt_for_local_install_update(binary_name: &str) -> io::Result<bool> {
    debug(&format!("checking for binary={binary_name}"));
    if env_truthy(ENV_SKIP_INSTALL_UPDATE_PROMPT) {
        debug("skipped: OZONE_SKIP_INSTALL_UPDATE_PROMPT set");
        return Ok(false);
    }
    if env_truthy(ENV_INSTALL_UPDATE_PROMPTED) {
        debug("skipped: already prompted this process tree");
        return Ok(false);
    }
    env::set_var(ENV_INSTALL_UPDATE_PROMPTED, "1");

    if !io::stdin().is_terminal() {
        debug("skipped: stdin is not a terminal");
        return Ok(false);
    }
    if !io::stdout().is_terminal() {
        debug("skipped: stdout is not a terminal");
        return Ok(false);
    }

    let Some(update) = pending_install_update(binary_name)? else {
        debug("no pending update found");
        return Ok(false);
    };

    // Show the first 8 hex chars of the source artifact's SHA-256 as a build ID.
    let build_id = sha256_file(&update.source_artifact)
        .map(|digest| {
            digest[..4]
                .iter()
                .fold(String::with_capacity(8), |mut s, b| {
                    use std::fmt::Write;
                    let _ = write!(s, "{b:02x}");
                    s
                })
        })
        .unwrap_or_else(|_| "????????".to_string());

    print!(
        "A newer local build is ready (build-id: {build_id}). Update installed binaries now? [Y/n] "
    );
    io::stdout().flush()?;

    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer)?;
    if answer_is_yes(&answer) {
        let status = Command::new(&update.sync_script)
            .arg("--no-build")
            .current_dir(&update.repo_root)
            .status()?;
        if !status.success() {
            return Err(io::Error::other(format!(
                "local install sync failed with status {status}"
            )));
        }
        return Ok(true);
    }

    println!("Skipped. Continuing with installed version.\n");
    Ok(false)
}

pub fn relaunch_current_process() -> io::Result<()> {
    let relaunch_target = env::args_os()
        .next()
        .filter(|arg0| !arg0.is_empty())
        .unwrap_or_else(|| {
            env::current_exe()
                .map(|path| path.into_os_string())
                .unwrap_or_else(|_| "ozone".into())
        });
    let status = Command::new(relaunch_target)
        .args(env::args_os().skip(1))
        .status()?;
    std::process::exit(status.code().unwrap_or(1));
}

fn pending_install_update(binary_name: &str) -> io::Result<Option<PendingInstallUpdate>> {
    let current_exe = env::current_exe()?;
    debug(&format!("current_exe={}", current_exe.display()));
    let home_dir = dirs::home_dir().ok_or_else(|| io::Error::other("HOME directory not found"))?;
    if !is_managed_install_path(&current_exe, &home_dir) {
        debug(&format!(
            "skipped: {} is not under ~/.cargo/bin or ~/.local/bin",
            current_exe.display()
        ));
        return Ok(None);
    }

    let Some(repo_root) = read_install_source_root()? else {
        debug("skipped: no repo root found (no marker file and cwd not in repo)");
        return Ok(None);
    };
    debug(&format!("repo_root={}", repo_root.display()));
    let sync_script = repo_root.join("contrib/sync-local-install.sh");
    if !sync_script.is_file() {
        debug(&format!(
            "skipped: sync script not found at {}",
            sync_script.display()
        ));
        return Ok(None);
    }

    let Some(source_artifact) =
        stale_release_artifact(&current_exe, &repo_root, binary_name, &home_dir)?
    else {
        debug("skipped: installed binary matches release artifact (checksums equal)");
        return Ok(None);
    };
    debug(&format!(
        "stale! source_artifact={}",
        source_artifact.display()
    ));

    Ok(Some(PendingInstallUpdate {
        repo_root,
        sync_script,
        source_artifact,
    }))
}

fn read_install_source_root() -> io::Result<Option<PathBuf>> {
    let Some(path) = crate::paths::install_source_root_path() else {
        return discover_repo_root_from_cwd();
    };
    if !path.is_file() {
        return discover_repo_root_from_cwd();
    }

    let contents = fs::read_to_string(path)?;
    let trimmed = contents.trim();
    if trimmed.is_empty() {
        return discover_repo_root_from_cwd();
    }
    Ok(Some(PathBuf::from(trimmed)))
}

fn discover_repo_root_from_cwd() -> io::Result<Option<PathBuf>> {
    let current_dir = env::current_dir()?;
    for candidate in current_dir.ancestors() {
        if candidate.join("Cargo.toml").is_file()
            && candidate.join("contrib/sync-local-install.sh").is_file()
        {
            return Ok(Some(candidate.to_path_buf()));
        }
    }
    Ok(None)
}

fn stale_release_artifact(
    current_exe: &Path,
    repo_root: &Path,
    binary_name: &str,
    home_dir: &Path,
) -> io::Result<Option<PathBuf>> {
    if !is_managed_install_path(current_exe, home_dir) {
        return Ok(None);
    }

    let source_artifact = repo_root.join("target/release").join(binary_name);
    if !source_artifact.is_file() {
        return Ok(None);
    }

    let current_checksum = sha256_file(current_exe)?;
    let source_checksum = sha256_file(&source_artifact)?;
    if current_checksum == source_checksum {
        return Ok(None);
    }

    Ok(Some(source_artifact))
}

fn is_managed_install_path(current_exe: &Path, home_dir: &Path) -> bool {
    current_exe.starts_with(home_dir.join(".cargo/bin"))
        || current_exe.starts_with(home_dir.join(".local/bin"))
}

fn sha256_file(path: &Path) -> io::Result<[u8; 32]> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    let mut digest = [0u8; 32];
    digest.copy_from_slice(&hasher.finalize());
    Ok(digest)
}

fn env_truthy(name: &str) -> bool {
    matches!(
    env::var(name),
    Ok(value)
        if value == "1"
            || value.eq_ignore_ascii_case("true")
            || value.eq_ignore_ascii_case("yes")
    )
}

fn answer_is_yes(answer: &str) -> bool {
    let trimmed = answer.trim();
    trimmed.is_empty() || trimmed.eq_ignore_ascii_case("y") || trimmed.eq_ignore_ascii_case("yes")
}

#[cfg(test)]
mod tests {
    use super::{is_managed_install_path, stale_release_artifact};
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("ozone-install-{name}-{nonce}"))
    }

    fn write_file(path: &Path, contents: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn managed_install_detection_matches_local_bin_dirs() {
        let home = PathBuf::from("/tmp/ozone-home");
        assert!(is_managed_install_path(
            &home.join(".cargo/bin/ozone"),
            &home
        ));
        assert!(is_managed_install_path(
            &home.join(".local/bin/ozone-plus"),
            &home
        ));
        assert!(!is_managed_install_path(
            &PathBuf::from("/tmp/elsewhere/ozone"),
            &home
        ));
    }

    #[test]
    fn stale_release_artifact_detects_checksum_drift() {
        let root = unique_temp_dir("stale");
        let home = root.join("home");
        let repo = root.join("repo");
        let current = home.join(".local/bin/ozone");
        let release = repo.join("target/release/ozone");

        write_file(&current, b"installed-old");
        write_file(&release, b"built-new");

        let detected = stale_release_artifact(&current, &repo, "ozone", &home).unwrap();
        assert_eq!(detected, Some(release));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn stale_release_artifact_skips_matching_binary() {
        let root = unique_temp_dir("match");
        let home = root.join("home");
        let repo = root.join("repo");
        let current = home.join(".cargo/bin/ozone-plus");
        let release = repo.join("target/release/ozone-plus");

        write_file(&current, b"same-binary");
        write_file(&release, b"same-binary");

        let detected = stale_release_artifact(&current, &repo, "ozone-plus", &home).unwrap();
        assert!(detected.is_none());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn stale_release_artifact_skips_non_installed_paths() {
        let root = unique_temp_dir("skip");
        let home = root.join("home");
        let repo = root.join("repo");
        let current = repo.join("target/debug/ozone");
        let release = repo.join("target/release/ozone");

        write_file(&current, b"debug-build");
        write_file(&release, b"release-build");

        let detected = stale_release_artifact(&current, &repo, "ozone", &home).unwrap();
        assert!(detected.is_none());

        let _ = fs::remove_dir_all(root);
    }
}
