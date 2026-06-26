use crate::optional_string;
use crate::optional_u64;
use crate::required_string;
use crate::session_summary_json;
use crate::LauncherSmokeRunnerSpec;
/// MCP tool: launcher smoke test.
use crate::OzoneMcpServer;
use crate::PtyVteCaptureConfig;
use crate::ToolReply;
use anyhow::anyhow;
use serde_json::json;
use serde_json::Value;
use std::fs;

pub fn launcher_smoke_tool(
    server: &mut OzoneMcpServer,
    args: &serde_json::Value,
) -> anyhow::Result<ToolReply> {
    let sandbox_id = required_string(args, "sandboxId")?;
    let live_refresh_model_name = optional_string(args, "liveRefreshModelName");
    let enter_count = optional_u64(args, "enterCount").unwrap_or(4);
    let sandbox = server
        .sandboxes
        .get(&sandbox_id)
        .ok_or_else(|| anyhow!("sandbox `{sandbox_id}` was not found"))?;
    let refresh_model_path = live_refresh_model_name
        .as_ref()
        .map(|name| sandbox.models_dir.join(name));
    let runner_spec = LauncherSmokeRunnerSpec {
        repo_root: server.repo_root.to_string_lossy().into_owned(),
        live_refresh_path: refresh_model_path.map(|path| path.to_string_lossy().into_owned()),
        enter_count,
        capture: PtyVteCaptureConfig::sandbox_artifacts(sandbox, "launcher-smoke-final"),
    };
    let script_body = r###"def run():
    master, proc = open_pty_process(
        ["cargo", "run", "--quiet", "--", "--mode", "base", "--frontend", "ozone-plus", "--no-browser"],
        SPEC["repoRoot"],
    )
    live_refresh_path = SPEC.get("liveRefreshPath")
    live_refresh_name = os.path.basename(live_refresh_path) if live_refresh_path else None
    saw_live_refresh_model = False

    pump(master, proc, 8.0)
    if live_refresh_path:
        open(live_refresh_path, "ab").close()
        pump(master, proc, 2.5)
        saw_live_refresh_model = screen_contains(live_refresh_name)

    for index in range(int(SPEC["enterCount"])):
        send_key(master, "enter")
        pump(master, proc, 4.0 if index + 1 == int(SPEC["enterCount"]) else 1.0)
        if live_refresh_name and not saw_live_refresh_model:
            saw_live_refresh_model = screen_contains(live_refresh_name)

    process_state = stop_process(proc)
    final_capture = capture_screen()
    return {
        "ok": True,
        "bufferTail": final_capture["tailText"],
        "sawLiveRefreshModel": saw_live_refresh_model,
        "screen": summarize_capture(final_capture),
        "processExitedBeforeStop": process_state["processExitedBeforeStop"],
        "exitCode": process_state["exitCode"],
    }
"###;
    let output = server.run_python_vte_helper(
        sandbox,
        &runner_spec,
        script_body,
        "failed to run launcher smoke helper",
    )?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let pty_data = serde_json::from_str::<Value>(&stdout)
        .unwrap_or_else(|_| json!({ "bufferTail": stdout.trim(), "sawLiveRefreshModel": false }));
    let launcher_invocation_log = sandbox.root.join("launcher-invocation.txt");
    let sessions = server.with_repo(Some(&sandbox_id), |repo| {
        Ok(repo
            .list_sessions()?
            .iter()
            .map(session_summary_json)
            .collect::<Vec<_>>())
    })?;
    let launcher_session = sessions.iter().find(|session| {
        session
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|name| name == "Launcher Session")
    });
    let launcher_session_count = sessions
        .iter()
        .filter(|session| {
            session
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| name == "Launcher Session")
        })
        .count();
    let data = json!({
        "commandOk": output.status.success(),
        "exitCode": output.status.code(),
        "pty": pty_data,
        "stderr": stderr,
        "launcherInvoked": launcher_invocation_log.exists(),
        "handoffOk": launcher_session.is_some(),
        "launcherArgs": if launcher_invocation_log.exists() {
            fs::read_to_string(&launcher_invocation_log)
                .ok()
                .map(|text| text.lines().map(str::to_owned).collect::<Vec<_>>())
        } else {
            None
        },
        "sessions": sessions,
        "launcherSession": launcher_session.cloned(),
        "launcherSessionCount": launcher_session_count,
    });
    let status_msg = if launcher_session_count > 1 {
        format!(
            "Completed launcher handoff smoke (warning: {} duplicate Launcher Session rows)",
            launcher_session_count
        )
    } else {
        "Completed launcher handoff smoke".to_owned()
    };
    Ok(if output.status.success() {
        ToolReply::success(status_msg, data)
    } else {
        ToolReply::error("Launcher handoff smoke failed".to_owned(), data)
    })
}
