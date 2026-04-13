//! Shell extensibility: custom commands, pre/post hooks, theme loading stubs.

use std::path::PathBuf;
use std::process::Command;
use serde::{Deserialize, Serialize};

/// Hook configuration (stored in ozone+ config)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HooksConfig {
    /// Script to run before each generation
    pub pre_generation: Option<PathBuf>,
    /// Script to run after each generation
    pub post_generation: Option<PathBuf>,
}

/// A custom slash command discovered from the commands directory
#[derive(Debug, Clone)]
pub struct CustomCommand {
    pub name: String,
    pub path: PathBuf,
    pub description: Option<String>,
}

/// Result of hook or command execution
#[derive(Debug)]
pub enum HookResult {
    Success { stdout: String },
    Skipped,
    Failed { error: String },
}

impl HooksConfig {
    pub fn run_pre_generation(&self, session_id: &str) -> HookResult {
        run_script(self.pre_generation.as_ref(), "pre_generation", session_id, "")
    }

    pub fn run_post_generation(&self, session_id: &str, response: &str) -> HookResult {
        run_script(self.post_generation.as_ref(), "post_generation", session_id, response)
    }
}

fn run_script(path: Option<&PathBuf>, name: &str, session_id: &str, response: &str) -> HookResult {
    let Some(script) = path else {
        return HookResult::Skipped;
    };
    if !script.exists() {
        return HookResult::Failed {
            error: format!("{} not found: {}", name, script.display()),
        };
    }
    match Command::new(script)
        .env("OZONE_SESSION_ID", session_id)
        .env("OZONE_RESPONSE", response)
        .output()
    {
        Ok(out) if out.status.success() => HookResult::Success {
            stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        },
        Ok(out) => HookResult::Failed {
            error: String::from_utf8_lossy(&out.stderr).to_string(),
        },
        Err(e) => HookResult::Failed { error: e.to_string() },
    }
}

/// Discover custom commands from `$XDG_CONFIG_HOME/ozone/commands/`
pub fn discover_commands() -> Vec<CustomCommand> {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()))
        .join("ozone")
        .join("commands");

    if !dir.exists() {
        return Vec::new();
    }

    let mut commands = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    let description = std::fs::read_to_string(&path).ok().and_then(|c| {
                        c.lines().next().and_then(|l| {
                            if l.starts_with('#') || l.starts_with("//") {
                                Some(l.trim_start_matches(['#', '/', ' ']).to_string())
                            } else {
                                None
                            }
                        })
                    });
                    commands.push(CustomCommand {
                        name: format!("/{}", stem),
                        path,
                        description,
                    });
                }
            }
        }
    }
    commands.sort_by(|a, b| a.name.cmp(&b.name));
    commands
}

/// Execute a custom command script
pub fn run_custom_command(cmd: &CustomCommand, args: &str, session_id: &str) -> HookResult {
    match Command::new(&cmd.path)
        .arg(args)
        .env("OZONE_SESSION_ID", session_id)
        .output()
    {
        Ok(out) if out.status.success() => HookResult::Success {
            stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        },
        Ok(out) => HookResult::Failed {
            error: String::from_utf8_lossy(&out.stderr).to_string(),
        },
        Err(e) => HookResult::Failed { error: e.to_string() },
    }
}

/// Stub: load a custom theme by name from `$XDG_CONFIG_HOME/ozone/themes/`
pub fn load_custom_theme(name: &str) -> Option<String> {
    let path = dirs::config_dir()?
        .join("ozone")
        .join("themes")
        .join(format!("{}.toml", name));
    std::fs::read_to_string(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hooks_default_skips() {
        let cfg = HooksConfig::default();
        assert!(matches!(cfg.run_pre_generation("s1"), HookResult::Skipped));
        assert!(matches!(cfg.run_post_generation("s1", "resp"), HookResult::Skipped));
    }

    #[test]
    fn hooks_config_roundtrip() {
        let cfg = HooksConfig {
            pre_generation: Some(PathBuf::from("/tmp/pre.sh")),
            post_generation: None,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: HooksConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.pre_generation, cfg.pre_generation);
        assert!(parsed.post_generation.is_none());
    }

    #[test]
    fn discover_commands_no_panic_on_missing_dir() {
        let _ = discover_commands();
    }

    #[test]
    fn load_theme_returns_none_when_missing() {
        assert!(load_custom_theme("nonexistent_theme_xyz").is_none());
    }
}
