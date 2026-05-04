     1|/// MCP tool: launcher smoke tool.
     2|use crate::OzoneMcpServer;
     3|use crate::ToolReply;
     4|use anyhow::Result;
     5|use serde_json::Value;
     6|
     7|pub fn launcher_smoke_tool(server: &mut OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
     8|    let sandbox_id = required_string(args, "sandboxId")?;
     9|    let live_refresh_model_name = optional_string(args, "liveRefreshModelName");
    10|    let enter_count = optional_u64(args, "enterCount").unwrap_or(4);
    11|    let sandbox = self
    12|        .sandboxes
    13|        .get(&sandbox_id)
    14|        .ok_or_else(|| anyhow!("sandbox `{sandbox_id}` was not found"))?;
    15|    let refresh_model_path = live_refresh_model_name
    16|        .as_ref()
    17|        .map(|name| sandbox.models_dir.join(name));
    18|    let runner_spec = LauncherSmokeRunnerSpec {
    19|        repo_root: server.repo_root.to_string_lossy().into_owned(),
    20|        live_refresh_path: refresh_model_path.map(|path| path.to_string_lossy().into_owned()),
    21|        enter_count,
    22|        capture: PtyVteCaptureConfig::sandbox_artifacts(sandbox, "launcher-smoke-final"),
    23|    };
    24|    let script_body = r###"def run():
    25|master, proc = open_pty_process(
    26|    ["cargo", "run", "--quiet", "--", "--mode", "base", "--frontend", "ozonePlus", "--no-browser"],
    27|    SPEC["repoRoot"],
    28|)
    29|live_refresh_path = SPEC.get("liveRefreshPath")
    30|live_refresh_name = os.path.basename(live_refresh_path) if live_refresh_path else None
    31|saw_live_refresh_model = False
    32|
    33|pump(master, proc, 8.0)
    34|if live_refresh_path:
    35|    open(live_refresh_path, "ab").close()
    36|    pump(master, proc, 2.5)
    37|    saw_live_refresh_model = screen_contains(live_refresh_name)
    38|
    39|for index in range(int(SPEC["enterCount"])):
    40|    send_key(master, "enter")
    41|    pump(master, proc, 4.0 if index + 1 == int(SPEC["enterCount"]) else 1.0)
    42|    if live_refresh_name and not saw_live_refresh_model:
    43|        saw_live_refresh_model = screen_contains(live_refresh_name)
    44|
    45|process_state = stop_process(proc)
    46|final_capture = capture_screen()
    47|return {
    48|    "ok": True,
    49|    "bufferTail": final_capture["tailText"],
    50|    "sawLiveRefreshModel": saw_live_refresh_model,
    51|    "screen": summarize_capture(final_capture),
    52|    "processExitedBeforeStop": process_state["processExitedBeforeStop"],
    53|    "exitCode": process_state["exitCode"],
    54|}
    55|"###;
    56|    let output = server.run_python_vte_helper(
    57|        sandbox,
    58|        &runner_spec,
    59|        script_body,
    60|        "failed to run launcher smoke helper",
    61|    )?;
    62|    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    63|    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    64|    let pty_data = serde_json::from_str::<Value>(&stdout).unwrap_or_else(
    65|        |_| json!({ "bufferTail": stdout.trim(), "sawLiveRefreshModel": false }),
    66|    );
    67|    let launcher_invocation_log = sandbox.root.join("launcher-invocation.txt");
    68|    let sessions = server.with_repo(Some(&sandbox_id), |repo| {
    69|        Ok(repo
    70|            .list_sessions()?
    71|            .iter()
    72|            .map(session_summary_json)
    73|            .collect::<Vec<_>>())
    74|    })?;
    75|    let launcher_session = sessions.iter().find(|session| {
    76|        session
    77|            .get("name")
    78|            .and_then(Value::as_str)
    79|            .is_some_and(|name| name == "Launcher Session")
    80|    });
    81|    let launcher_session_count = sessions
    82|        .iter()
    83|        .filter(|session| {
    84|            session
    85|                .get("name")
    86|                .and_then(Value::as_str)
    87|                .is_some_and(|name| name == "Launcher Session")
    88|        })
    89|        .count();
    90|    let data = json!({
    91|        "commandOk": output.status.success(),
    92|        "exitCode": output.status.code(),
    93|        "pty": pty_data,
    94|        "stderr": stderr,
    95|        "launcherInvoked": launcher_invocation_log.exists(),
    96|        "handoffOk": launcher_session.is_some(),
    97|        "launcherArgs": if launcher_invocation_log.exists() {
    98|            fs::read_to_string(&launcher_invocation_log)
    99|                .ok()
   100|                .map(|text| text.lines().map(str::to_owned).collect::<Vec<_>>())
   101|        } else {
   102|            None
   103|        },
   104|        "sessions": sessions,
   105|        "launcherSession": launcher_session.cloned(),
   106|        "launcherSessionCount": launcher_session_count,
   107|    });
   108|    let status_msg = if launcher_session_count > 1 {
   109|        format!(
   110|            "Completed launcher handoff smoke (warning: {} duplicate Launcher Session rows)",
   111|            launcher_session_count
   112|        )
   113|    } else {
   114|        "Completed launcher handoff smoke".to_owned()
   115|    };
   116|    Ok(if output.status.success() {
   117|        ToolReply::success(status_msg, data)
   118|    } else {
   119|        ToolReply::error("Launcher handoff smoke failed".to_owned(), data)
   120|    })
   121|}
   122|
   123|