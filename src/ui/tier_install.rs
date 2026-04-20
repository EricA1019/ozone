//! Tier install flow — offered when user picks Base or Plus and the binary is not found.

use std::path::PathBuf;

/// Returns the binary name for a given tier (the executable that should be on PATH).
pub fn binary_name_for_tier(tier: crate::prefs::Tier) -> &'static str {
    match tier {
        crate::prefs::Tier::Lite => "ozone-lite",
        crate::prefs::Tier::Base => "ozone",
        crate::prefs::Tier::Plus => "ozone-plus",
    }
}

/// Returns true if the named binary is accessible on PATH.
pub fn is_tier_installed(binary: &str) -> bool {
    let Ok(path_var) = std::env::var("PATH") else {
        return false;
    };
    std::env::split_paths(&path_var).any(|dir| dir.join(binary).is_file())
}

/// Download and install a tier binary from GitHub releases.
/// Returns `Ok(installed_path)` or `Err(description)`.
pub fn install_tier_from_github(tier_binary_name: &str) -> Result<PathBuf, String> {
    let install_dir = install_target_dir()?;
    let asset_name = github_asset_name(tier_binary_name)?;

    let release_url = "https://api.github.com/repos/EricA1019/ozone/releases/latest";

    let response = ureq::get(release_url)
        .set("User-Agent", "ozone/0.4")
        .call()
        .map_err(|e| format!("Failed to fetch release info: {e}"))?;

    let json: serde_json::Value = response
        .into_json()
        .map_err(|e| format!("Failed to parse release JSON: {e}"))?;

    let assets = json["assets"]
        .as_array()
        .ok_or_else(|| "No assets found in the latest release".to_string())?;

    let asset_url = assets
        .iter()
        .find(|a| a["name"].as_str() == Some(&asset_name))
        .and_then(|a| a["browser_download_url"].as_str())
        .ok_or_else(|| {
            format!(
                "Asset '{asset_name}' not found in latest release. \
                 Install manually: https://github.com/EricA1019/ozone/releases"
            )
        })?
        .to_string();

    let response = ureq::get(&asset_url)
        .set("User-Agent", "ozone/0.4")
        .call()
        .map_err(|e| format!("Download failed: {e}"))?;

    let dest_path = install_dir.join(tier_binary_name);
    let mut dest_file = std::fs::File::create(&dest_path)
        .map_err(|e| format!("Cannot write to {}: {e}", dest_path.display()))?;

    std::io::copy(&mut response.into_reader(), &mut dest_file)
        .map_err(|e| format!("Write failed: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("chmod failed: {e}"))?;
    }

    Ok(dest_path)
}

fn install_target_dir() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").map_err(|_| "HOME not set".to_string())?;
    let local_bin = PathBuf::from(&home).join(".local/bin");
    if local_bin.exists() {
        return Ok(local_bin);
    }
    let cargo_bin = PathBuf::from(&home).join(".cargo/bin");
    if cargo_bin.exists() {
        return Ok(cargo_bin);
    }
    // Create ~/.local/bin if neither exists
    std::fs::create_dir_all(&local_bin)
        .map_err(|e| format!("Failed to create ~/.local/bin: {e}"))?;
    Ok(local_bin)
}

fn github_asset_name(tier_binary_name: &str) -> Result<String, String> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let platform = match (os, arch) {
        ("linux", "x86_64") => "linux-x86_64",
        ("linux", "aarch64") => "linux-aarch64",
        ("macos", "x86_64") => "macos-x86_64",
        ("macos", "aarch64") => "macos-aarch64",
        _ => {
            return Err(format!(
                "Unsupported platform: {os}-{arch}. \
                 Install manually: https://github.com/EricA1019/ozone/releases"
            ))
        }
    };
    Ok(format!("{tier_binary_name}-{platform}"))
}
