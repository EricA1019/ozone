use crate::OzoneMcpServer;
use crate::ToolReply;
use crate::Sandbox;
use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use std::fs;
use uuid::Uuid;

/// Creates a temporary XDG sandbox environment for testing.
pub fn sandbox_tool(server: &mut OzoneMcpServer, args: &Value) -> Result<ToolReply> {
    let prefix = super::optional_string(args, "namePrefix")
        .unwrap_or_else(|| "ozone-mcp".to_owned());
    let sandbox_id = format!("sandbox-{}", Uuid::new_v4());
    let root = std::env::temp_dir().join(format!(
        "{}-{}",
        sanitize_prefix(&prefix),
        Uuid::new_v4().simple()
    ));
    let data_home = root.join("data");
    let home = root.join("home");
    let models_dir = root.join("models");
    let exports_dir = root.join("exports");
    fs::create_dir_all(root.join("data/ozone"))?;
    fs::create_dir_all(&home)?;
    fs::create_dir_all(&models_dir)?;
    fs::create_dir_all(&exports_dir)?;

    for model_name in super::optional_string_array(args, "models")? {
        fs::write(models_dir.join(&model_name), [])?;
    }

    let mut launcher_script = None;
    if super::optional_bool(args, "createLauncherStub").unwrap_or(false) {
        let exit_code = super::optional_i64(args, "launcherExitCode").unwrap_or(0);
        let invocation_log = root.join("launcher-invocation.txt");
        let script_path = root.join("mock-launcher.sh");
        fs::write(
            &script_path,
            format!(
                "#!/bin/sh\necho \"$@\" >> {}\nexit {}\n",
                invocation_log.display(),
                exit_code
            ),
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&script_path)?.permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&script_path, permissions)?;
        }
        launcher_script = Some(script_path);
    }

    if let Some(preferences) = args.get("preferences") {
        let preferences_path = root.join("data/ozone/preferences.json");
        let normalized_preferences = merge_json_objects(
            default_preferences_json(),
            normalize_preferences_json(preferences),
        );
        let text = serde_json::to_string_pretty(&normalized_preferences)?;
        fs::write(preferences_path, format!("{text}\n"))?;
    }

    let sandbox = Sandbox {
        id: sandbox_id.clone(),
        root: root.clone(),
        data_home,
        home,
        models_dir,
        launcher_script: launcher_script.clone(),
        backend: None,
    };
    let data = sandbox.describe();
    server.sandboxes.insert(sandbox_id.clone(), sandbox);

    Ok(ToolReply::success(
        "Created temp-XDG sandbox".to_owned(),
        data,
    ))
}
