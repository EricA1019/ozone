     1|/// MCP tool: sandbox tool.
     2|use crate::OzoneMcpServer;
     3|use crate::ToolReply;
     4|use anyhow::Result;
     5|use serde_json::Value;
     6|
     7|pub fn sandbox_tool(server: &mut OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
     8|    match required_string(args, "action")?.as_str() {
     9|        "create" => server.create_sandbox(args),
    10|        "destroy" => server.destroy_sandbox(args),
    11|        other => Ok(ToolReply::error(
    12|            "Sandbox action failed".to_owned(),
    13|            json!({ "error": format!("unsupported sandbox action `{other}`") }),
    14|        )),
    15|    }
    16|}
    17|
    18|fn create_sandbox(&mut self, args: &Value) -> Result<ToolReply> {
    19|    let prefix = optional_string(args, "namePrefix").unwrap_or_else(|| "ozone-mcp".to_owned());
    20|    let sandbox_id = format!("sandbox-{}", Uuid::new_v4());
    21|    let root = env::temp_dir().join(format!(
    22|        "{}-{}",
    23|        sanitize_prefix(&prefix),
    24|        Uuid::new_v4().simple()
    25|    ));
    26|    let data_home = root.join("data");
    27|    let home = root.join("home");
    28|    let models_dir = root.join("models");
    29|    let exports_dir = root.join("exports");
    30|    fs::create_dir_all(root.join("data/ozone"))?;
    31|    fs::create_dir_all(&home)?;
    32|    fs::create_dir_all(&models_dir)?;
    33|    fs::create_dir_all(&exports_dir)?;
    34|
    35|    for model_name in optional_string_array(args, "models")? {
    36|        fs::write(models_dir.join(&model_name), [])?;
    37|    }
    38|
    39|    let mut launcher_script = None;
    40|    if optional_bool(args, "createLauncherStub").unwrap_or(false) {
    41|        let exit_code = optional_i64(args, "launcherExitCode").unwrap_or(0);
    42|        let invocation_log = root.join("launcher-invocation.txt");
    43|        let script_path = root.join("mock-launcher.sh");
    44|        fs::write(
    45|            &script_path,
    46|            format!(
    47|                "#!/bin/sh\nprintf \"%s\\n\" \"$@\" > \"{}\"\nexit {}\n",
    48|                invocation_log.display(),
    49|                exit_code
    50|            ),
    51|        )?;
    52|        #[cfg(unix)]
    53|        {
    54|            use std::os::unix::fs::PermissionsExt;
    55|            let mut permissions = fs::metadata(&script_path)?.permissions();
    56|            permissions.set_mode(0o755);
    57|            fs::set_permissions(&script_path, permissions)?;
    58|        }
    59|        launcher_script = Some(script_path);
    60|    }
    61|
    62|    if let Some(preferences) = args.get("preferences") {
    63|        let preferences_path = root.join("data/ozone/preferences.json");
    64|        let normalized_preferences = merge_json_objects(
    65|            default_preferences_json(),
    66|            normalize_preferences_json(preferences),
    67|        );
    68|        let text = serde_json::to_string_pretty(&normalized_preferences)?;
    69|        fs::write(preferences_path, format!("{text}\n"))?;
    70|    }
    71|
    72|    let sandbox = Sandbox {
    73|        id: sandbox_id.clone(),
    74|        root: root.clone(),
    75|        data_home,
    76|        home,
    77|        models_dir,
    78|        launcher_script: launcher_script.clone(),
    79|        backend: None,
    80|    };
    81|    let data = sandbox.describe();
    82|    server.sandboxes.insert(sandbox_id, sandbox);
    83|
    84|    Ok(ToolReply::success(
    85|        "Created temp-XDG sandbox".to_owned(),
    86|        data,
    87|    ))
    88|}
    89|
    90|fn destroy_sandbox(&mut self, args: &Value) -> Result<ToolReply> {
    91|    let sandbox_id = required_string(args, "sandboxId")?;
    92|    let mut sandbox = self
    93|        .sandboxes
    94|        .remove(&sandbox_id)
    95|        .ok_or_else(|| anyhow!("sandbox `{sandbox_id}` was not found"))?;
    96|    sandbox.stop_backend()?;
    97|    if sandbox.root.exists() {
    98|        fs::remove_dir_all(&sandbox.root)
    99|            .with_context(|| format!("failed to remove {}", sandbox.root.display()))?;
   100|    }
   101|    Ok(ToolReply::success(
   102|        "Destroyed sandbox".to_owned(),
   103|        json!({ "sandboxId": sandbox_id }),
   104|    ))
   105|}
   106|
   107|