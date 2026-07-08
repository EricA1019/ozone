//! OzoneMcpServer — MCP protocol server implementation.
//!
//! Contains the core server struct, tool/journey/sandbox method impls,
//! and Drop cleanup. Extracted from `lib.rs`.

use std::{
    collections::BTreeMap,
    env,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::sandbox::Sandbox;
use crate::testing;
use crate::testing::{
    CapturableScreenJourneyDefinition, MockUserJourneySpec, MockUserRunnerSpec, PreparedSandbox,
    PtyVteCaptureConfig,
};
use crate::types::CommandOutput;
use crate::tools;
use crate::{
    mock_user_capture_settings,
    EnvOverrideGuard, JsonRpcRequest, ToolReply,
};


pub(crate) struct OzoneMcpServer {
    pub(crate) repo_root: PathBuf,
    pub(crate) sandboxes: BTreeMap<String, Sandbox>,
}

impl OzoneMcpServer {
    pub(crate) fn new() -> Result<Self> {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .ok_or_else(|| anyhow!("failed to resolve repo root from crate manifest path"))?;
        Ok(Self {
            repo_root,
            sandboxes: BTreeMap::new(),
        })
    }

    pub(crate) fn handle_request(&mut self, request: JsonRpcRequest) -> Option<Value> {
        if request.jsonrpc != crate::JSONRPC_VERSION {
            return request.id.map(|id| {
                crate::jsonrpc::error_response(
                    id,
                    -32600,
                    format!("unsupported jsonrpc version `{}`", request.jsonrpc),
                )
            });
        }

        match request.method.as_str() {
            "initialize" => request
                .id
                .map(|id| crate::jsonrpc::success_response(id, self.initialize_result())),
            "notifications/initialized" => None,
            "ping" => request.id.map(|id| crate::jsonrpc::success_response(id, json!({}))),
            "tools/list" => request.id.map(|id| crate::jsonrpc::success_response(id, self.tools_list_result())),
            "tools/call" => request.id.map(|id| match self.handle_tool_call(request.params) {
                Ok(result) => crate::jsonrpc::success_response(id, result),
                Err(error) => crate::jsonrpc::success_response(
                    id,
                    json!({
                        "content": [{ "type": "text", "text": format!("Tool call failed: {error}") }],
                        "structuredContent": {
                            "summary": "Tool call failed",
                            "data": { "error": error.to_string() }
                        },
                        "isError": true
                    }),
                ),
            }),
            _ => request.id.map(|id| {
                crate::jsonrpc::error_response(id, -32601, format!("method `{}` is not supported", request.method))
            }),
        }
    }

    fn initialize_result(&self) -> Value {
        json!({
            "protocolVersion": crate::MCP_PROTOCOL_VERSION,
            "capabilities": {
                "tools": {
                    "listChanged": false
                }
            },
            "serverInfo": {
                "name": "ozone-mcp",
                "version": env!("CARGO_PKG_VERSION")
            }
        })
    }

    fn tools_list_result(&self) -> Value {
        json!({
            "tools": crate::tool_definitions()
        })
    }

    fn handle_tool_call(&mut self, params: Option<Value>) -> Result<Value> {
        let params = params.unwrap_or_else(|| json!({}));
        let tool_name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("tool call is missing `name`"))?;
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| Value::Object(Map::new()));
        let reply = crate::tool_dispatch::dispatch_tool_call(self, tool_name, &arguments)?;
        Ok(reply.into_result())
    }

    #[allow(dead_code)]
    pub fn screenshot_tool(&mut self, args: &Value) -> Result<ToolReply> {
        tools::screenshot_tool(self, args)
    }

    #[allow(dead_code)]
    pub fn screen_check_tool(&self, args: &Value) -> Result<ToolReply> {
        tools::screen_check_tool(self, args)
    }

    // =========================================================================
    // JOURNEY BUILDER METHODS
    // =========================================================================
    //
    // These methods build MockUserJourneySpec structs for automated testing
    // and interactive validation. They are organized as methods on OzoneMcpServer
    // to maintain shared state (repo_root, command building utilities) across
    // all journey builders.
    //
    // Journey builders are registered in capturable_screen_journey_builders()
    // as function pointers in CapturableScreenJourneyDefinition structures.
    // This pattern allows:
    //   1. Type-safe, validated navigation targets
    //   2. Reusable sandbox setup configurations
    //   3. Composable journey construction (builders call each other)
    //
    // For testing infrastructure (VTE parsing, screen evaluation), see
    // testing/screen.rs. For sandbox setup utilities, see testing/journey.rs.
    //
    // =========================================================================

    /// Build a named mock user journey with optional arguments.
    ///
    /// Supported journey names (for advanced testing):
    /// - "launcher_monitor_roundtrip": Navigate to launcher, then to monitor
    /// - "launcher_to_ozone_plus": Navigate from launcher to ozone+ shell
    /// - "ozone_plus_chat_journey": Open ozone+ and send a chat prompt
    pub(crate) fn build_mock_user_journey(
        &self,
        journey_name: &str,
        args: &Value,
    ) -> Result<MockUserJourneySpec> {
        testing::build_mock_user_journey(self, journey_name, args)
    }

    pub fn build_mock_user_target_journey(&self, target_name: &str) -> Result<MockUserJourneySpec> {
        self.build_capturable_screen_journey(target_name, &json!({}), target_name)
    }

    pub fn prepare_mock_user_sandbox(
        &mut self,
        sandbox_id: Option<String>,
        journey_name: Option<&str>,
        target_name: Option<&str>,
    ) -> Result<PreparedSandbox> {
        let setup = match (journey_name, target_name) {
            (Some(_), Some(_)) => bail!("provide either `journey` or `target`, not both"),
            (Some(journey_name), None) => {
                self.recommended_mock_user_journey_sandbox_setup(journey_name)?
            }
            (None, Some(target_name)) => self.capturable_target_sandbox_setup(target_name)?,
            (None, None) => bail!("mock_user_tool requires either `journey` or `target`"),
        };
        self.prepare_sandbox_from_setup(sandbox_id, setup)
    }

    pub fn prepare_target_sandbox(
        &mut self,
        sandbox_id: Option<String>,
        target_name: &str,
    ) -> Result<PreparedSandbox> {
        let setup = self.capturable_target_sandbox_setup(target_name)?;
        self.prepare_sandbox_from_setup(sandbox_id, setup)
    }

    pub(crate) fn capturable_target_sandbox_setup(&self, target_name: &str) -> Result<Value> {
        Ok((self
            .capturable_screen_definition(target_name)?
            .sandbox_setup)())
    }

    pub(crate) fn recommended_mock_user_journey_sandbox_setup(&self, journey_name: &str) -> Result<Value> {
        match journey_name {
            "launcher_monitor_roundtrip" => self.capturable_target_sandbox_setup("base_monitor"),
            "launcher_to_ozone_plus" => {
                self.capturable_target_sandbox_setup("base_ozone_plus_shell")
            }
            "ozone_plus_chat_journey" => {
                let mut setup = self.capturable_target_sandbox_setup("base_ozone_plus_shell")?;
                let setup_map = setup.as_object_mut().ok_or_else(|| {
                    anyhow!("sandbox setup for `{journey_name}` must be an object")
                })?;
                setup_map.insert("requiresMockBackend".to_owned(), Value::Bool(true));
                Ok(setup)
            }
            other => bail!("unsupported mock-user journey `{other}`"),
        }
    }

    pub(crate) fn prepare_sandbox_from_setup(
        &mut self,
        sandbox_id: Option<String>,
        setup: Value,
    ) -> Result<PreparedSandbox> {
        if let Some(sandbox_id) = sandbox_id {
            return Ok(PreparedSandbox {
                sandbox_id,
                auto_created: false,
                auto_started_mock_backend: false,
                setup_applied: None,
            });
        }

        let setup_map = setup
            .as_object()
            .cloned()
            .ok_or_else(|| anyhow!("sandbox setup must be a JSON object"))?;
        let mut create_args = Map::new();
        create_args.insert("action".to_owned(), Value::String("create".to_owned()));
        create_args.extend(setup_map);
        let reply = self.sandbox_tool(&Value::Object(create_args))?;
        let sandbox_id = reply
            .data
            .get("sandboxId")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("sandbox creation reply did not include sandboxId"))?
            .to_owned();
        let auto_started_mock_backend = setup
            .get("requiresMockBackend")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if auto_started_mock_backend {
            let _model_name = setup
                .get("models")
                .and_then(Value::as_array)
                .and_then(|models| models.first())
                .and_then(Value::as_str)
                .unwrap_or("mock-model.gguf");
            #[cfg(feature = "legacy-tools")]
            tools::mock_backend_tool(
                self,
                &json!({
                    "action": "start",
                    "sandboxId": sandbox_id,
                    "modelName": _model_name,
                }),
            )?;
        }
        Ok(PreparedSandbox {
            sandbox_id,
            auto_created: true,
            auto_started_mock_backend,
            setup_applied: Some(setup),
        })
    }

    pub fn annotate_prepared_sandbox(&self, data: &mut Value, prepared: &PreparedSandbox) {
        let Value::Object(map) = data else {
            return;
        };
        if !prepared.auto_created {
            return;
        }
        map.insert("sandboxAutoCreated".to_owned(), Value::Bool(true));
        map.insert(
            "mockBackendAutoStarted".to_owned(),
            Value::Bool(prepared.auto_started_mock_backend),
        );
        if let Some(setup) = prepared.setup_applied.clone() {
            map.insert("sandboxSetupApplied".to_owned(), setup);
        }
    }

    // --------- Core Builder Dispatch ---------

    /// Navigate to a capturable screen target and build a journey to reach it.
    ///
    /// Valid targets are registered in capturable_screen_journey_builders().
    /// Use screen_nav_targets MCP tool to list available targets.
    pub(crate) fn build_capturable_screen_journey(
        &self,
        target_screen: &str,
        args: &Value,
        journey_name: &str,
    ) -> Result<MockUserJourneySpec> {
        testing::build_capturable_screen_journey(self, target_screen, args, journey_name)
    }

    pub(crate) fn capturable_screen_definition(
        &self,
        target_screen: &str,
    ) -> Result<&'static CapturableScreenJourneyDefinition> {
        testing::capturable_screen_definition(target_screen)
    }

    pub(crate) fn screen_nav_target_data(
        &self,
        definition: &CapturableScreenJourneyDefinition,
    ) -> Result<Value> {
        testing::screen_nav_target_data(self, definition)
    }

    pub fn run_mock_user_journey(
        &self,
        sandbox_id: &str,
        journey: &MockUserJourneySpec,
        target: Option<String>,
        args: &Value,
        capture_override: Option<PtyVteCaptureConfig>,
    ) -> Result<Value> {
        let sandbox = self
            .sandboxes
            .get(sandbox_id)
            .ok_or_else(|| anyhow!("sandbox `{sandbox_id}` was not found"))?;
        let capture_settings =
            mock_user_capture_settings(args, sandbox, journey, capture_override)?;
        let runner_spec = MockUserRunnerSpec {
            name: journey.name.clone(),
            target,
            cwd: journey.cwd.clone(),
            command: journey.command.clone(),
            steps: journey.steps.clone(),
            capture_settings,
        };
        let script_body = r###"def run():
    master, proc = open_pty_process(SPEC["command"], SPEC["cwd"])
    results = []
    screenshots = []
    step_captures = SPEC.get("stepCaptures") or []

    def scoped_capture(paths):
        if not paths:
            return None
        previous_png = CAPTURE.get("pngPath")
        previous_json = CAPTURE.get("jsonPath")
        if paths.get("pngPath"):
            CAPTURE["pngPath"] = paths["pngPath"]
        else:
            CAPTURE.pop("pngPath", None)
        if paths.get("jsonPath"):
            CAPTURE["jsonPath"] = paths["jsonPath"]
        else:
            CAPTURE.pop("jsonPath", None)
        try:
            return capture_screen()
        finally:
            if previous_png is None:
                CAPTURE.pop("pngPath", None)
            else:
                CAPTURE["pngPath"] = previous_png
            if previous_json is None:
                CAPTURE.pop("jsonPath", None)
            else:
                CAPTURE["jsonPath"] = previous_json

    def wait_for_markers(markers, timeout_s):
        if not markers:
            pump(master, proc, timeout_s)
            return []
        deadline = time.time() + timeout_s
        matched = []
        while time.time() < deadline:
            pump(master, proc, min(0.2, max(0.0, deadline - time.time())))
            full_screen = screen_text()
            matched = [marker for marker in markers if marker in full_screen]
            if matched:
                return matched
        return matched

    for index, step in enumerate(SPEC["steps"]):
        action = step["action"]
        if action["kind"] == "wait":
            matched = wait_for_markers(step.get("expectAny", []), action["ms"] / 1000.0)
        elif action["kind"] == "key":
            send_key(master, action["key"])
            pump(master, proc, step["settleMs"] / 1000.0)
            matched = None
        elif action["kind"] == "text":
            send_text(master, action["text"])
            pump(master, proc, step["settleMs"] / 1000.0)
            matched = None
        else:
            fail("unsupported action kind `" + action["kind"] + "`")

        full_screen = screen_text()
        window_snapshot = screen_tail()
        if matched is None:
            matched = [marker for marker in step.get("expectAny", []) if marker in full_screen]
        ok = True if not step.get("expectAny") else bool(matched)
        step_result = {
            "name": step["name"],
            "action": action["kind"],
            "ok": ok,
            "matchedMarkers": matched,
            "tail": window_snapshot[-1200:],
        }
        step_capture = None
        if SPEC.get("captureScreenshots") and index < len(step_captures):
            step_capture = scoped_capture(step_captures[index])
        if step_capture:
            step_summary = summarize_capture(step_capture)
            step_result["screen"] = step_summary
            screenshots.append(
                {
                    "stepIndex": index,
                    "name": step["name"],
                    **step_summary,
                }
            )
        results.append(step_result)

    process_state = stop_process(proc)
    final_capture = capture_screen()
    visible_markers = sorted({marker for step in results for marker in step["matchedMarkers"]})
    return {
        "ok": all(step["ok"] for step in results),
        "journey": SPEC["name"],
        "target": SPEC.get("target"),
        "command": SPEC["command"],
        "success": all(step["ok"] for step in results),
        "captureScreenshots": bool(SPEC.get("captureScreenshots")),
        "outputDir": SPEC.get("outputDir"),
        "rawBytes": len(buffer),
        "steps": results,
        "screenshots": screenshots,
        "visibleMarkersReached": visible_markers,
        "processExitedBeforeStop": process_state["processExitedBeforeStop"],
        "exitCode": process_state["exitCode"],
        "captureTime": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "text": final_capture["text"],
        "finalTail": final_capture["tailText"],
        "paths": {
            "png": final_capture.get("pngPath"),
            "json": final_capture.get("jsonPath"),
        },
        "dimensions": {
            "rows": final_capture["screenRows"],
            "columns": final_capture["screenColumns"],
            "font": final_capture.get("font"),
        },
        "captureSummary": {
            "stepCount": len(results),
            "screenshotCount": len(screenshots),
            "matchedMarkers": visible_markers,
            "cursor": final_capture["cursor"],
        },
        "finalCapture": final_capture,
        "finalScreen": summarize_capture(final_capture),
    }
"###;
        let output = self.run_python_vte_helper(
            sandbox,
            &runner_spec,
            script_body,
            "failed to run mock-user PTY helper",
        )?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let mut data = serde_json::from_str::<Value>(&stdout).unwrap_or_else(|_| {
            json!({
                "journey": journey.name,
                "command": journey.command,
                "success": false,
                "steps": [],
                "visibleMarkersReached": [],
                "processExitedBeforeStop": false,
                "exitCode": null,
                "finalTail": stdout.trim(),
            })
        });
        if let Value::Object(map) = &mut data {
            map.insert("sandboxId".to_owned(), Value::String(sandbox_id.to_owned()));
            map.insert("stderr".to_owned(), Value::String(stderr));
            map.insert("runnerOk".to_owned(), Value::Bool(output.status.success()));
        }
        Ok(data)
    }

    pub fn run_python_vte_helper(
        &self,
        sandbox: &Sandbox,
        spec: &impl Serialize,
        script_body: &str,
        error_context: &str,
    ) -> Result<std::process::Output> {
        let spec_json = serde_json::to_string(spec)?;
        let script = [
            crate::PYTHON_PTY_VTE_HELPER,
            script_body,
            crate::PYTHON_PTY_VTE_HELPER_TRAILER,
        ]
        .join("\n\n")
        .replace("__SPEC_JSON__", &serde_json::to_string(&spec_json)?);
        let mut command = Command::new("python3");
        command.arg("-c").arg(script).current_dir(&self.repo_root);
        command.envs(sandbox.command_env());
        command.output().with_context(|| error_context.to_owned())
    }

    #[cfg(feature = "legacy-tools")]
    pub fn with_repo<T>(
        &self,
        sandbox_id: Option<&str>,
        f: impl FnOnce(SqliteRepository) -> Result<T>,
    ) -> Result<T> {
        self.with_sandbox_env(sandbox_id, || {
            let repo = SqliteRepository::from_xdg()?;
            f(repo)
        })
    }

    pub fn with_sandbox_env<T>(
        &self,
        sandbox_id: Option<&str>,
        f: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let _guard = EnvOverrideGuard::new(
            sandbox_id
                .and_then(|id| self.sandboxes.get(id))
                .map(Sandbox::env_overrides)
                .unwrap_or_default(),
        );
        f()
    }

    pub fn run_workspace_command(
        &self,
        program: &str,
        args: &[String],
        sandbox_id: Option<&str>,
    ) -> Result<CommandOutput> {
        let mut command = Command::new(program);
        command.args(args).current_dir(&self.repo_root);
        let env_map = sandbox_id
            .and_then(|id| self.sandboxes.get(id))
            .map(Sandbox::command_env)
            .unwrap_or_default();
        command.envs(env_map);
        let output = command
            .output()
            .with_context(|| format!("failed to run `{program}`"))?;
        Ok(CommandOutput {
            command: format!("{program} {}", args.join(" ")),
            success: output.status.success(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

impl Drop for OzoneMcpServer {
    fn drop(&mut self) {
        for sandbox in self.sandboxes.values_mut() {
            let _ = sandbox.stop_backend();
        }
    }
}

