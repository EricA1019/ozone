use std::{
    collections::BTreeMap,
    env, fs,
    io::{self, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use sandbox::{ManagedBackend, Sandbox};

use anyhow::{anyhow, bail, Context, Result};
use ozone_core::{
    engine::{
        BranchId, ConversationMessage, MessageId, SwipeCandidate, SwipeGroup,
        SwipeGroupId,
    },
    session::SessionId,
};
use ozone_persist::{
    BranchRecord, PersistError, PinnedMemoryView, SqliteRepository,
};
use serde::Serialize;
use serde_json::{json, Map, Value};
use uuid::Uuid;

mod sandbox;
mod testing;
mod tools;
mod jsonrpc;
mod tool_dispatch;

use self::jsonrpc::{error_response, read_message, success_response, write_message, JsonRpcRequest};

use testing::{
    sandbox_setup_base_launch_path, sandbox_setup_base_launcher,
    sandbox_setup_base_ozone_plus_shell, sandbox_setup_base_profile_review,
    sandbox_setup_base_profile_run, sandbox_setup_base_splash,
    sandbox_setup_base_tier_picker, sandbox_setup_ozone_plus_entry,
    CapturableScreenJourneyDefinition, LauncherSmokeRunnerSpec, MockUserCaptureSettings, MockUserJourneySpec,
    MockUserRunnerSpec, PtyVteCaptureArtifacts, PtyVteCaptureConfig,
    PreparedSandbox,
};

const JSONRPC_VERSION: &str = "2.0";
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
pub const OZONE_PLUS_PACKAGE: &str = "ozone-plus";
const DEFAULT_PTY_ROWS: u16 = 40;
const DEFAULT_PTY_COLUMNS: u16 = 120;
const DEFAULT_CAPTURE_TAIL_CHARS: usize = 6000;
const DEFAULT_CAPTURE_FONT_SIZE: u16 = 16;
const LEGACY_MOCK_USER_JOURNEYS: &[&str] = &[
    "launcher_monitor_roundtrip",
    "launcher_to_ozone_plus",
    "ozone_plus_chat_journey",
];
pub fn run_stdio_server() -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();
    let mut server = OzoneMcpServer::new()?;

    while let Some(request) = read_message(&mut reader)? {
        if let Some(response) = server.handle_request(request) {
            write_message(&mut writer, &response)?;
            writer.flush()?;
        }
    }

    Ok(())
}

#[derive(Debug)]
struct OzoneMcpServer {
    repo_root: PathBuf,
    sandboxes: BTreeMap<String, Sandbox>,
}

impl OzoneMcpServer {
    fn new() -> Result<Self> {
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

    fn handle_request(&mut self, request: JsonRpcRequest) -> Option<Value> {
        if request.jsonrpc != JSONRPC_VERSION {
            return request.id.map(|id| {
                error_response(
                    id,
                    -32600,
                    format!("unsupported jsonrpc version `{}`", request.jsonrpc),
                )
            });
        }

        match request.method.as_str() {
            "initialize" => request
                .id
                .map(|id| success_response(id, self.initialize_result())),
            "notifications/initialized" => None,
            "ping" => request.id.map(|id| success_response(id, json!({}))),
            "tools/list" => request.id.map(|id| success_response(id, self.tools_list_result())),
            "tools/call" => request.id.map(|id| match self.handle_tool_call(request.params) {
                Ok(result) => success_response(id, result),
                Err(error) => success_response(
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
                error_response(id, -32601, format!("method `{}` is not supported", request.method))
            }),
        }
    }

    fn initialize_result(&self) -> Value {
        json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
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
            "tools": tool_definitions()
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
        let reply = tool_dispatch::dispatch_tool_call(self, tool_name, &arguments)?;
        Ok(reply.into_result())
    }

    pub fn sandbox_tool(&mut self, args: &Value) -> Result<ToolReply> {
        let prefix = optional_string(args, "namePrefix").unwrap_or_else(|| "ozone-mcp".to_owned());
        let sandbox_id = format!("sandbox-{}", Uuid::new_v4());
        let root = env::temp_dir().join(format!(
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

        for model_name in optional_string_array(args, "models")? {
            fs::write(models_dir.join(&model_name), [])?;
        }

        let mut launcher_script = None;
        if optional_bool(args, "createLauncherStub").unwrap_or(false) {
            let exit_code = optional_i64(args, "launcherExitCode").unwrap_or(0);
            let invocation_log = root.join("launcher-invocation.txt");
            let script_path = root.join("mock-launcher.sh");
            fs::write(
                &script_path,
                format!(
                    "#!/bin/sh\nprintf \"%s\\n\" \"$@\" > \"{}\"\nexit {}\n",
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
        self.sandboxes.insert(sandbox_id, sandbox);

        Ok(ToolReply::success(
            "Created temp-XDG sandbox".to_owned(),
            data,
        ))
    }






    pub fn start_mock_backend(&mut self, args: &Value) -> Result<ToolReply> {
        let sandbox_id = required_string(args, "sandboxId")?;
        let port = optional_u64(args, "port").unwrap_or(5001) as u16;
        let model_name =
            optional_string(args, "modelName").unwrap_or_else(|| "mock-model.gguf".to_owned());
        let sandbox = self
            .sandboxes
            .get_mut(&sandbox_id)
            .ok_or_else(|| anyhow!("sandbox `{sandbox_id}` was not found"))?;
        sandbox.stop_backend()?;

        let script_path = sandbox.root.join("mock_kobold.py");
        let log_path = sandbox.root.join("mock_kobold.log");
        let script = format!(
            r#"from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import time

MODEL_NAME = {model_name:?}
PORT = {port}

class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        pass

    def _json(self, payload, code=200):
        data = json.dumps(payload).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        if self.path == "/api/v1/model":
            return self._json({{"result": MODEL_NAME}})
        if self.path == "/api/v1/config/max_context_length":
            return self._json({{"value": 8192}})
        if self.path == "/api/extra/perf":
            return self._json({{"last_process_speed": 12.5, "last_eval_speed": 18.0}})
        return self._json({{"error": "not found", "path": self.path}}, code=404)

    def do_POST(self):
        if self.path != "/api/extra/generate/stream":
            return self._json({{"error": "not found", "path": self.path}}, code=404)

        length = int(self.headers.get("Content-Length", "0") or 0)
        payload = self.rfile.read(length) if length else b""
        prompt = ""
        if payload:
            try:
                prompt = json.loads(payload.decode("utf-8")).get("prompt", "")
            except Exception:
                prompt = ""
        prompt = (prompt or "").lower()
        if "observatory" in prompt:
            tokens = ["The", " observatory", " key", " is", " logged."]
        elif "second" in prompt:
            tokens = ["Second", " reply", " confirmed."]
        else:
            tokens = ["Hello", " from", " mock", " backend."]

        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.end_headers()
        for token in tokens:
            self.wfile.write(f"data: {{json.dumps({{'token': token}})}}\n\n".encode("utf-8"))
            self.wfile.flush()
            time.sleep(0.02)
        self.wfile.write(b'data: {{"done": true}}\n\n')
        self.wfile.flush()

HTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
"#,
        );
        fs::write(&script_path, script)?;

        let log_file = fs::File::create(&log_path)?;
        let child = Command::new("python3")
            .arg(&script_path)
            .stdout(Stdio::from(log_file.try_clone()?))
            .stderr(Stdio::from(log_file))
            .spawn()
            .with_context(|| "failed to launch python3 mock backend")?;
        thread::sleep(Duration::from_millis(300));

        let base_url = format!("http://127.0.0.1:{port}");
        let pid = child.id();
        sandbox.backend = Some(ManagedBackend {
            child,
            port,
            model_name: model_name.clone(),
            base_url: base_url.clone(),
            log_path: log_path.clone(),
        });

        Ok(ToolReply::success(
            "Started mock backend".to_owned(),
            json!({
                "sandboxId": sandbox_id,
                "pid": pid,
                "port": port,
                "baseUrl": base_url,
                "modelName": model_name,
                "logPath": log_path
            }),
        ))
    }


    pub fn stop_mock_backend(&mut self, args: &Value) -> Result<ToolReply> {
        let sandbox_id = required_string(args, "sandboxId")?;
        let sandbox = self
            .sandboxes
            .get_mut(&sandbox_id)
            .ok_or_else(|| anyhow!("sandbox `{sandbox_id}` was not found"))?;
        let stopped = sandbox.stop_backend()?;
        Ok(ToolReply::success(
            "Stopped mock backend".to_owned(),
            json!({
                "sandboxId": sandbox_id,
                "stopped": stopped
            }),
        ))
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
    fn build_mock_user_journey(
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

    fn capturable_target_sandbox_setup(&self, target_name: &str) -> Result<Value> {
        Ok((self
            .capturable_screen_definition(target_name)?
            .sandbox_setup)())
    }

    fn recommended_mock_user_journey_sandbox_setup(&self, journey_name: &str) -> Result<Value> {
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

    fn prepare_sandbox_from_setup(
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
            let model_name = setup
                .get("models")
                .and_then(Value::as_array)
                .and_then(|models| models.first())
                .and_then(Value::as_str)
                .unwrap_or("mock-model.gguf");
            tools::mock_backend_tool(self, &json!({
                "action": "start",
                "sandboxId": sandbox_id,
                "modelName": model_name,
            }))?;
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
    fn build_capturable_screen_journey(
        &self,
        target_screen: &str,
        args: &Value,
        journey_name: &str,
    ) -> Result<MockUserJourneySpec> {
        testing::build_capturable_screen_journey(self, target_screen, args, journey_name)
    }

    fn capturable_screen_definition(
        &self,
        target_screen: &str,
    ) -> Result<&'static CapturableScreenJourneyDefinition> {
        testing::capturable_screen_definition(target_screen)
    }

    fn screen_nav_target_data(
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
            PYTHON_PTY_VTE_HELPER,
            script_body,
            PYTHON_PTY_VTE_HELPER_TRAILER,
        ]
        .join("\n\n")
        .replace("__SPEC_JSON__", &serde_json::to_string(&spec_json)?);
        let mut command = Command::new("python3");
        command.arg("-c").arg(script).current_dir(&self.repo_root);
        command.envs(sandbox.command_env());
        command.output().with_context(|| error_context.to_owned())
    }

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

pub fn capturable_screen_journey_builders() -> &'static [testing::CapturableScreenJourneyDefinition] {
    &[
        CapturableScreenJourneyDefinition {
            target_screen: "base_splash",
            description: "Cold-start Ozone splash screen.",
            builder: testing::build_base_splash_screen_journey,
            sandbox_setup: sandbox_setup_base_splash,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_tier_picker",
            description: "First-run tier picker between splash and launcher.",
            builder: testing::build_base_tier_picker_screen_journey,
            sandbox_setup: sandbox_setup_base_tier_picker,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_launcher",
            description: "Base Ozone launcher dashboard.",
            builder: testing::build_base_launcher_screen_journey,
            sandbox_setup: sandbox_setup_base_launcher,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_exit_confirm",
            description: "Launcher exit confirmation dialog.",
            builder: testing::build_base_exit_confirm_screen_journey,
            sandbox_setup: sandbox_setup_base_launcher,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_settings",
            description: "Base Ozone settings screen.",
            builder: testing::build_base_settings_screen_journey,
            sandbox_setup: sandbox_setup_base_launcher,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_model_picker_launch",
            description: "Launch-mode model picker.",
            builder: testing::build_base_model_picker_launch_screen_journey,
            sandbox_setup: sandbox_setup_base_launch_path,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_confirm_launch",
            description: "Launch confirmation dialog before backend start.",
            builder: testing::build_base_confirm_launch_screen_journey,
            sandbox_setup: sandbox_setup_base_launch_path,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_frontend_choice",
            description: "Frontend choice screen shown when no frontend is preselected.",
            builder: testing::build_base_frontend_choice_screen_journey,
            sandbox_setup: sandbox_setup_base_launch_path,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_launching",
            description: "Transient launch-progress screen after confirming frontend.",
            builder: testing::build_base_launching_screen_journey,
            sandbox_setup: sandbox_setup_base_launch_path,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_monitor",
            description: "Live Ozone monitor screen.",
            builder: testing::build_base_monitor_screen_journey,
            sandbox_setup: sandbox_setup_base_launch_path,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_model_picker_profile",
            description: "Profile-mode model picker.",
            builder: testing::build_base_model_picker_profile_screen_journey,
            sandbox_setup: sandbox_setup_base_profile_review,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_profile_advisory",
            description: "Profiling advisor overview.",
            builder: testing::build_base_profile_advisory_screen_journey,
            sandbox_setup: sandbox_setup_base_profile_review,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_profile_confirm",
            description: "Profiling action confirmation dialog.",
            builder: testing::build_base_profile_confirm_screen_journey,
            sandbox_setup: sandbox_setup_base_profile_run,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_profile_running",
            description: "Profiling in-progress screen.",
            builder: testing::build_base_profile_running_screen_journey,
            sandbox_setup: sandbox_setup_base_profile_run,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_profile_failure",
            description: "Profiling failure / issue-report screen.",
            builder: testing::build_base_profile_failure_screen_journey,
            sandbox_setup: sandbox_setup_base_profile_review,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_ozone_plus_shell",
            description: "ozone+ conversation shell reached through the base launcher handoff.",
            builder: testing::build_base_ozone_plus_shell_journey,
            sandbox_setup: sandbox_setup_base_ozone_plus_shell,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "ozone_plus_main_menu",
            description: "ozone+ main menu from direct handoff.",
            builder: testing::build_ozone_plus_main_menu_screen_journey,
            sandbox_setup: sandbox_setup_ozone_plus_entry,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "ozone_plus_sessions",
            description: "ozone+ session list screen.",
            builder: testing::build_ozone_plus_sessions_screen_journey,
            sandbox_setup: sandbox_setup_ozone_plus_entry,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "ozone_plus_characters",
            description: "ozone+ character manager screen.",
            builder: testing::build_ozone_plus_characters_screen_journey,
            sandbox_setup: sandbox_setup_ozone_plus_entry,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "ozone_plus_character_create",
            description: "ozone+ new-character form.",
            builder: testing::build_ozone_plus_character_create_screen_journey,
            sandbox_setup: sandbox_setup_ozone_plus_entry,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "ozone_plus_character_import",
            description: "ozone+ import-character form.",
            builder: testing::build_ozone_plus_character_import_screen_journey,
            sandbox_setup: sandbox_setup_ozone_plus_entry,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "ozone_plus_settings",
            description: "ozone+ settings/config screen.",
            builder: testing::build_ozone_plus_settings_screen_journey,
            sandbox_setup: sandbox_setup_ozone_plus_entry,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "ozone_plus_conversation",
            description: "ozone+ conversation shell from the main menu.",
            builder: testing::build_ozone_plus_conversation_screen_journey,
            sandbox_setup: sandbox_setup_ozone_plus_entry,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "ozone_plus_help",
            description: "ozone+ help overlay from conversation mode.",
            builder: testing::build_ozone_plus_help_screen_journey,
            sandbox_setup: sandbox_setup_ozone_plus_entry,
        },
    ]
}

const PYTHON_PTY_VTE_HELPER: &str = r###"import json
import os
import pty
import select
import signal
import subprocess
import time
import fcntl
import struct
import termios
from pathlib import Path

SPEC = json.loads(__SPEC_JSON__)
CAPTURE = SPEC.get("capture") or {}
ROWS = int(CAPTURE.get("rows") or 40)
COLUMNS = int(CAPTURE.get("columns") or 120)
TAIL_CHARS = int(CAPTURE.get("tailChars") or 1600)
FONT_SIZE = int(CAPTURE.get("fontSize") or 16)
DEFAULT_FG = (229, 229, 229)
DEFAULT_BG = (12, 12, 12)
ANSI_RGB = {
    "default": DEFAULT_FG,
    "black": (12, 12, 12),
    "red": (205, 49, 49),
    "green": (13, 188, 121),
    "brown": (229, 229, 16),
    "yellow": (229, 229, 16),
    "blue": (36, 114, 200),
    "magenta": (188, 63, 188),
    "cyan": (17, 168, 205),
    "white": (229, 229, 229),
    "brightblack": (102, 102, 102),
    "brightred": (241, 76, 76),
    "brightgreen": (35, 209, 139),
    "brightyellow": (245, 245, 67),
    "brightblue": (59, 142, 234),
    "brightmagenta": (214, 112, 214),
    "brightcyan": (41, 184, 219),
    "brightwhite": (255, 255, 255),
}
KEY_BYTES = {
    "enter": b"\r",
    "esc": b"\x1b",
    "up": b"\x1b[A",
    "down": b"\x1b[B",
    "right": b"\x1b[C",
    "left": b"\x1b[D",
    "tab": b"\t",
}

try:
    import pyte
except ModuleNotFoundError:
    pyte = None

try:
    from PIL import Image, ImageDraw, ImageFont
except ModuleNotFoundError:
    Image = ImageDraw = ImageFont = None

def fail(message, *, missing_dependencies=None, **extra):
    payload = {"ok": False, "error": message}
    if missing_dependencies:
        payload["missingModules"] = list(missing_dependencies)
    payload.update(extra)
    print(json.dumps(payload, ensure_ascii=False))
    raise SystemExit(1)

missing_dependencies = []
if pyte is None:
    missing_dependencies.append("pyte")
if Image is None:
    missing_dependencies.append("Pillow")
if missing_dependencies:
    fail(
        "missing python dependencies for VTE capture helper: "
        + ", ".join(missing_dependencies)
        + ". Install with `python3 -m pip install pyte Pillow`.",
        missing_dependencies=missing_dependencies,
    )

screen = pyte.Screen(COLUMNS, ROWS)
stream = pyte.ByteStream(screen)
buffer = bytearray()

def ensure_parent(path):
    if path:
        Path(path).parent.mkdir(parents=True, exist_ok=True)

def child_env():
    env = os.environ.copy()
    env.setdefault("TERM", "xterm-color")
    env["LINES"] = str(ROWS)
    env["COLUMNS"] = str(COLUMNS)
    return env

def open_pty_process(command, cwd):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLUMNS, 0, 0))
    proc = subprocess.Popen(
        command,
        cwd=cwd,
        stdin=slave,
        stdout=slave,
        stderr=slave,
        start_new_session=True,
        env=child_env(),
    )
    os.close(slave)
    return master, proc

def append_chunk(chunk):
    if not chunk:
        return False
    buffer.extend(chunk)
    stream.feed(chunk)
    return True

def pump(master, proc, seconds):
    end = time.time() + seconds
    while time.time() < end:
        timeout = min(0.2, max(0.0, end - time.time()))
        ready, _, _ = select.select([master], [], [], timeout)
        if master in ready:
            try:
                chunk = os.read(master, 65536)
            except OSError:
                break
            if not chunk:
                break
            append_chunk(chunk)
        if proc.poll() is not None and not ready:
            break

def send_key(master, key):
    payload = KEY_BYTES.get(key)
    if payload is None:
        fail(f"unsupported PTY key `{key}`")
    os.write(master, payload)

def send_text(master, text):
    os.write(master, text.encode("utf-8"))

def stop_process(proc):
    process_exited = proc.poll() is not None
    exit_code = proc.returncode if process_exited else None
    if not process_exited:
        try:
            os.killpg(proc.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            proc.wait(timeout=5)
        except Exception:
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
    return {
        "processExitedBeforeStop": process_exited,
        "exitCode": exit_code,
    }

def screen_text():
    return "\n".join(screen.display)

def screen_tail():
    return screen_text()[-TAIL_CHARS:]

def screen_contains(marker):
    return bool(marker) and marker in screen_text()

def font_candidates():
    return [
        os.environ.get("OZONE_MCP_VTE_FONT"),
        "DejaVuSansMono.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
        "LiberationMono-Regular.ttf",
        "/usr/share/fonts/truetype/liberation2/LiberationMono-Regular.ttf",
        "NotoSansMono-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf",
    ]

def load_font():
    for candidate in font_candidates():
        if not candidate:
            continue
        try:
            return ImageFont.truetype(candidate, FONT_SIZE), candidate
        except OSError:
            continue
    return ImageFont.load_default(), "PillowDefault"

def resolve_color(name, fallback):
    if not isinstance(name, str):
        return fallback
    lower = name.lower()
    if lower == "default":
        return fallback
    return ANSI_RGB.get(lower, fallback)

def cell_style(char):
    fg = resolve_color(getattr(char, "fg", "default"), DEFAULT_FG)
    bg = resolve_color(getattr(char, "bg", "default"), DEFAULT_BG)
    reverse = bool(getattr(char, "reverse", False))
    if reverse:
        fg, bg = bg, fg
    return {
        "fg": getattr(char, "fg", "default"),
        "bg": getattr(char, "bg", "default"),
        "bold": bool(getattr(char, "bold", False)),
        "italics": bool(getattr(char, "italics", False)),
        "underscore": bool(getattr(char, "underscore", False)),
        "strikethrough": bool(getattr(char, "strikethrough", False)),
        "blink": bool(getattr(char, "blink", False)),
        "reverse": reverse,
        "resolvedFg": list(fg),
        "resolvedBg": list(bg),
    }

def serialize_screen():
    display = list(screen.display)
    rows = []
    for row_index in range(screen.lines):
        line = screen.buffer.get(row_index) or {}
        row_cells = []
        for column_index in range(screen.columns):
            char = line.get(column_index) or screen.default_char
            style = cell_style(char)
            row_cells.append(
                {
                    "column": column_index,
                    "text": getattr(char, "data", " "),
                    **style,
                }
            )
        rows.append(
            {
                "index": row_index,
                "row": row_index,
                "text": display[row_index],
                "cells": row_cells,
            }
        )
    text = "\n".join(display)
    return {
        "screenRows": screen.lines,
        "screenColumns": screen.columns,
        "lineCount": screen.lines,
        "columnCount": screen.columns,
        "cursor": {
            "row": screen.cursor.y,
            "column": screen.cursor.x,
        },
        "cursorRow": screen.cursor.y,
        "cursorCol": screen.cursor.x,
        "display": display,
        "text": text,
        "tailText": text[-TAIL_CHARS:],
        "rows": rows,
        "grid": rows,
    }

def render_screen_png(capture, png_path):
    ensure_parent(png_path)
    font, font_name = load_font()
    bbox = font.getbbox("M")
    cell_width = max(1, bbox[2] - bbox[0])
    cell_height = max(1, bbox[3] - bbox[1] + 2)
    baseline_y = -bbox[1]
    image = Image.new(
        "RGB",
        (capture["screenColumns"] * cell_width, capture["screenRows"] * cell_height),
        DEFAULT_BG,
    )
    draw = ImageDraw.Draw(image)
    for row in capture["grid"]:
        top = row.get("index", row["row"]) * cell_height
        for cell in row["cells"]:
            left = cell["column"] * cell_width
            fg = tuple(cell["resolvedFg"])
            bg = tuple(cell["resolvedBg"])
            if bg != DEFAULT_BG:
                draw.rectangle((left, top, left + cell_width, top + cell_height), fill=bg)
            text = cell["text"] or " "
            if text != " ":
                draw.text((left, top + baseline_y), text, font=font, fill=fg)
            if cell["underscore"]:
                underline_y = top + cell_height - 2
                draw.line((left, underline_y, left + cell_width - 1, underline_y), fill=fg, width=1)
            if cell["strikethrough"]:
                strike_y = top + (cell_height // 2)
                draw.line((left, strike_y, left + cell_width - 1, strike_y), fill=fg, width=1)
    image.save(png_path, format="PNG")
    capture["font"] = {
        "family": font_name,
        "size": FONT_SIZE,
        "cellWidth": cell_width,
        "cellHeight": cell_height,
    }

def capture_screen():
    capture = serialize_screen()
    png_path = CAPTURE.get("pngPath")
    json_path = CAPTURE.get("jsonPath")
    if png_path and not json_path:
        json_path = str(Path(png_path).with_suffix(".json"))
    if png_path:
        render_screen_png(capture, png_path)
        capture["pngPath"] = png_path
    if json_path:
        ensure_parent(json_path)
        with open(json_path, "w", encoding="utf-8") as handle:
            json.dump(capture, handle, ensure_ascii=False, indent=2)
        capture["jsonPath"] = json_path
    return capture

def summarize_capture(capture):
    return {
        "screenRows": capture["screenRows"],
        "screenColumns": capture["screenColumns"],
        "cursor": capture["cursor"],
        "tailText": capture["tailText"],
        "display": capture["display"],
        "pngPath": capture.get("pngPath"),
        "jsonPath": capture.get("jsonPath"),
        "font": capture.get("font"),
    }
"###;

const PYTHON_PTY_VTE_HELPER_TRAILER: &str = r###"if __name__ == "__main__":
    try:
        print(json.dumps(run(), ensure_ascii=False))
    except SystemExit:
        raise
    except Exception as exc:
        print(
            json.dumps(
                {
                    "ok": False,
                    "error": str(exc),
                    "errorType": type(exc).__name__,
                },
                ensure_ascii=False,
            )
        )
        raise SystemExit(1)
"###;

impl PtyVteCaptureConfig {
    fn defaults() -> Self {
        Self {
            rows: DEFAULT_PTY_ROWS,
            columns: DEFAULT_PTY_COLUMNS,
            tail_chars: DEFAULT_CAPTURE_TAIL_CHARS,
            font_size: DEFAULT_CAPTURE_FONT_SIZE,
            png_path: None,
            json_path: None,
        }
    }

    fn sandbox_artifacts(sandbox: &Sandbox, stem: &str) -> Self {
        let captures_dir = sandbox.root.join("captures");
        let artifacts = PtyVteCaptureArtifacts::for_stem(&captures_dir, stem);
        Self::defaults().with_artifacts(&artifacts)
    }

    fn with_artifacts(mut self, artifacts: &PtyVteCaptureArtifacts) -> Self {
        self.png_path = Some(artifacts.png_path.clone());
        self.json_path = Some(artifacts.json_path.clone());
        self
    }
}

impl PtyVteCaptureArtifacts {
    fn for_stem(output_dir: &Path, stem: &str) -> Self {
        let sanitized_stem = sanitize_prefix(stem);
        let png_path = output_dir.join(format!("{sanitized_stem}.png"));
        let json_path = output_dir.join(format!("{sanitized_stem}.json"));
        Self {
            png_path: png_path.display().to_string(),
            json_path: json_path.display().to_string(),
        }
    }
}

pub fn screenshot_capture_config(
    args: &Value,
    output_dir: &Path,
    target: &str,
) -> Result<PtyVteCaptureConfig> {
    let stem = screenshot_file_stem(optional_string(args, "filename").as_deref(), target)?;
    let dimensions = optional_object(args, "dimensions");
    let rows = dimensions
        .and_then(|value| value.get("rows"))
        .and_then(Value::as_u64)
        .or_else(|| optional_u64(args, "rows"))
        .unwrap_or(DEFAULT_PTY_ROWS as u64);
    let columns = dimensions
        .and_then(|value| value.get("columns"))
        .and_then(Value::as_u64)
        .or_else(|| optional_u64(args, "columns"))
        .unwrap_or(DEFAULT_PTY_COLUMNS as u64);
    let tail_chars = optional_u64(args, "tailChars").unwrap_or(DEFAULT_CAPTURE_TAIL_CHARS as u64);
    let font_size = optional_u64(args, "fontSize").unwrap_or(DEFAULT_CAPTURE_FONT_SIZE as u64);

    Ok(PtyVteCaptureConfig {
        rows: checked_u16(rows, "rows")?,
        columns: checked_u16(columns, "columns")?,
        tail_chars: checked_usize(tail_chars, "tailChars")?,
        font_size: checked_u16(font_size, "fontSize")?,
        png_path: Some(output_dir.join(format!("{stem}.png")).display().to_string()),
        json_path: Some(
            output_dir
                .join(format!("{stem}.json"))
                .display()
                .to_string(),
        ),
    })
}

fn mock_user_capture_settings(
    args: &Value,
    sandbox: &Sandbox,
    journey: &MockUserJourneySpec,
    capture_override: Option<PtyVteCaptureConfig>,
) -> Result<MockUserCaptureSettings> {
    let capture_screenshots = optional_bool(args, "captureScreenshots").unwrap_or(false);
    let mut capture = capture_override.unwrap_or(mock_user_capture_config(args)?);
    let output_dir = if capture_screenshots {
        Some(
            resolve_mock_user_output_dir(
                sandbox,
                &journey.name,
                optional_string(args, "outputDir").as_deref(),
            )
            .display()
            .to_string(),
        )
    } else {
        None
    };
    let step_captures = output_dir
        .as_deref()
        .map(|dir| {
            journey
                .steps
                .iter()
                .enumerate()
                .map(|(index, step)| {
                    PtyVteCaptureArtifacts::for_stem(
                        Path::new(dir),
                        &format!("step-{:02}-{}", index + 1, step.name),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    if let Some(dir) = output_dir.as_deref() {
        capture =
            capture.with_artifacts(&PtyVteCaptureArtifacts::for_stem(Path::new(dir), "final"));
    }
    Ok(MockUserCaptureSettings {
        capture,
        capture_screenshots,
        output_dir,
        step_captures,
    })
}

fn mock_user_capture_config(args: &Value) -> Result<PtyVteCaptureConfig> {
    let dimensions = optional_object(args, "dimensions");
    let rows = dimensions
        .and_then(|value| value.get("rows"))
        .and_then(Value::as_u64)
        .or_else(|| optional_u64(args, "rows"))
        .unwrap_or(DEFAULT_PTY_ROWS as u64);
    let columns = dimensions
        .and_then(|value| value.get("columns"))
        .and_then(Value::as_u64)
        .or_else(|| optional_u64(args, "columns"))
        .unwrap_or(DEFAULT_PTY_COLUMNS as u64);
    let tail_chars = optional_u64(args, "tailChars").unwrap_or(DEFAULT_CAPTURE_TAIL_CHARS as u64);
    let font_size = optional_u64(args, "fontSize").unwrap_or(DEFAULT_CAPTURE_FONT_SIZE as u64);

    Ok(PtyVteCaptureConfig {
        rows: checked_u16(rows, "rows")?,
        columns: checked_u16(columns, "columns")?,
        tail_chars: checked_usize(tail_chars, "tailChars")?,
        font_size: checked_u16(font_size, "fontSize")?,
        png_path: None,
        json_path: None,
    })
}

fn resolve_mock_user_output_dir(
    sandbox: &Sandbox,
    journey_name: &str,
    output_dir: Option<&str>,
) -> PathBuf {
    match output_dir.map(PathBuf::from) {
        Some(path) if path.is_absolute() => path,
        Some(path) => sandbox.root.join(path),
        None => sandbox
            .root
            .join("captures")
            .join(format!("mock-user-{}", sanitize_prefix(journey_name))),
    }
}

fn screenshot_file_stem(filename: Option<&str>, target: &str) -> Result<String> {
    let raw = filename.unwrap_or(target);
    let candidate = Path::new(raw);
    if candidate.components().count() != 1 {
        bail!("`filename` must be a plain file name without directory segments");
    }
    let stem = candidate
        .file_stem()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("`filename` must contain a valid file name"))?;
    Ok(sanitize_prefix(stem))
}

#[derive(Debug, Serialize)]
struct ToolDefinition {
    name: &'static str,
    description: &'static str,
    #[serde(rename = "inputSchema")]
    input_schema: Value,
}

fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "workspace_status",
            description: "Inspect Ozone workspace roots, members, and default paths.",
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "cargo_tool",
            description: "Run focused cargo build/test/check/clippy commands inside the Ozone workspace.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["check", "test", "build", "clippy"] },
                    "package": { "type": "string" },
                    "release": { "type": "boolean" },
                    "quiet": { "type": "boolean" },
                    "extraArgs": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["action"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "catalog_list",
            description: "List GGUF files and broken symlinks in the active or sandboxed models directory.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "sandboxId": { "type": "string" }
                },
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "preferences_get",
            description: "Read the active or sandboxed Ozone preferences.json file.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "sandboxId": { "type": "string" }
                },
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "sandbox_tool",
            description: "Create or destroy a temp-XDG sandbox for Ozone smoke tests.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["create", "destroy"] },
                    "sandboxId": { "type": "string" },
                    "namePrefix": { "type": "string" },
                    "models": { "type": "array", "items": { "type": "string" } },
                    "preferences": { "type": "object" },
                    "createLauncherStub": { "type": "boolean" },
                    "launcherExitCode": { "type": "integer" }
                },
                "required": ["action"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "mock_backend_tool",
            description: "Start or stop a mock KoboldCpp-compatible backend inside a sandbox.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["start", "stop"] },
                    "sandboxId": { "type": "string" },
                    "port": { "type": "integer" },
                    "modelName": { "type": "string" }
                },
                "required": ["action", "sandboxId"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "session_tool",
            description: "Create, list, inspect metadata, or load transcripts for ozone+ sessions.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["create", "list", "metadata", "transcript"] },
                    "sandboxId": { "type": "string" },
                    "sessionId": { "type": "string" },
                    "name": { "type": "string" },
                    "characterName": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "branchId": { "type": "string" }
                },
                "required": ["action"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "message_tool",
            description: "Send a runtime-backed message through ozone-plus.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["send"] },
                    "sandboxId": { "type": "string" },
                    "sessionId": { "type": "string" },
                    "content": { "type": "string" },
                    "author": { "type": "string" },
                    "authorName": { "type": "string" }
                },
                "required": ["action", "sessionId", "content"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "memory_tool",
            description: "Create note memories, pin message memories, or list pinned memories.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["note", "pin", "list"] },
                    "sandboxId": { "type": "string" },
                    "sessionId": { "type": "string" },
                    "content": { "type": "string" },
                    "messageId": { "type": "string" },
                    "expiresAfterTurns": { "type": "integer" }
                },
                "required": ["action", "sessionId"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "search_tool",
            description: "Run ozone-plus session/global search or trigger index rebuild with structured command results.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["session", "global", "index_rebuild"] },
                    "sandboxId": { "type": "string" },
                    "sessionId": { "type": "string" },
                    "query": { "type": "string" }
                },
                "required": ["action"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "branch_tool",
            description: "Create, list, or activate ozone+ branches.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["create", "list", "activate"] },
                    "sandboxId": { "type": "string" },
                    "sessionId": { "type": "string" },
                    "name": { "type": "string" },
                    "fromMessageId": { "type": "string" },
                    "branchId": { "type": "string" },
                    "activate": { "type": "boolean" }
                },
                "required": ["action", "sessionId"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "swipe_tool",
            description: "Add, list, or activate ozone+ swipe candidates.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["add", "list", "activate"] },
                    "sandboxId": { "type": "string" },
                    "sessionId": { "type": "string" },
                    "parentMessageId": { "type": "string" },
                    "content": { "type": "string" },
                    "contextMessageId": { "type": "string" },
                    "swipeGroupId": { "type": "string" },
                    "ordinal": { "type": "integer" },
                    "author": { "type": "string" },
                    "authorName": { "type": "string" },
                    "state": { "type": "string", "enum": ["active", "discarded", "failed_mid_stream"] }
                },
                "required": ["action", "sessionId"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "export_tool",
            description: "Export ozone+ sessions or transcripts, optionally writing files.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["session", "transcript"] },
                    "sandboxId": { "type": "string" },
                    "sessionId": { "type": "string" },
                    "branchId": { "type": "string" },
                    "format": { "type": "string", "enum": ["json", "text"] },
                    "outputPath": { "type": "string" }
                },
                "required": ["action", "sessionId"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "import_card",
            description: "Import a character card into ozone+ from a file path or JSON string.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "sandboxId": { "type": "string" },
                    "path": { "type": "string" },
                    "cardJson": { "type": "string" },
                    "sessionName": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "provenance": { "type": "string" }
                },
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "launcher_smoke",
            description: "Drive the base ozone launcher in a PTY and report whether it handed off into a launcher-managed ozone+ session.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "sandboxId": { "type": "string" },
                    "liveRefreshModelName": { "type": "string" },
                    "enterCount": { "type": "integer" }
                },
                "required": ["sandboxId"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "screen_nav_targets",
            description: "List centralized cold-start navigation targets for capturable ozone and ozone+ screens.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "enum": capturable_screen_journey_builders()
                            .iter()
                            .map(|entry| entry.target_screen)
                            .collect::<Vec<_>>()
                    }
                },
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "mock_user_tool",
            description: "Play through named front-door terminal journeys in real ozone / ozone-plus binaries using PTY input only. Omitting sandboxId auto-prepares the recommended temp-XDG sandbox for the requested target or journey.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "sandboxId": { "type": "string" },
                    "journey": {
                        "type": "string",
                        "enum": LEGACY_MOCK_USER_JOURNEYS
                    },
                    "target": {
                        "type": "string",
                        "enum": capturable_screen_journey_builders()
                            .iter()
                            .map(|entry| entry.target_screen)
                            .collect::<Vec<_>>()
                    },
                    "prompt": { "type": "string" },
                    "captureScreenshots": { "type": "boolean", "default": false },
                    "outputDir": { "type": "string" },
                    "rows": { "type": "integer", "minimum": 1 },
                    "columns": { "type": "integer", "minimum": 1 },
                    "fontSize": { "type": "integer", "minimum": 1 },
                    "tailChars": { "type": "integer", "minimum": 1 }
                },
                "anyOf": [
                    { "required": ["journey"] },
                    { "required": ["target"] }
                ],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "screenshot_tool",
            description: "Navigate to a centralized capturable screen target and save a PNG plus JSON terminal snapshot. Omitting sandboxId auto-prepares the target's recommended temp-XDG sandbox.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "sandboxId": { "type": "string" },
                    "target": {
                        "type": "string",
                        "enum": capturable_screen_journey_builders()
                            .iter()
                            .map(|entry| entry.target_screen)
                            .collect::<Vec<_>>()
                    },
                    "outputDir": { "type": "string" },
                    "filename": { "type": "string" },
                    "dimensions": {
                        "type": "object",
                        "properties": {
                            "rows": { "type": "integer", "minimum": 1 },
                            "columns": { "type": "integer", "minimum": 1 }
                        },
                        "additionalProperties": false
                    },
                    "rows": { "type": "integer", "minimum": 1 },
                    "columns": { "type": "integer", "minimum": 1 },
                    "fontSize": { "type": "integer", "minimum": 1 },
                    "tailChars": { "type": "integer", "minimum": 1 }
                },
                "required": ["target", "outputDir"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "screen_check_tool",
            description: "Run structured grid-based assertions against a screenshot JSON sidecar or matching PNG artifact path.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "artifactPath": { "type": "string" },
                    "path": { "type": "string" },
                    "sidecarPath": { "type": "string" },
                    "checks": {
                        "type": "array",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "properties": {
                                "type": {
                                    "type": "string",
                                    "enum": [
                                        "text_present",
                                        "text_absent",
                                        "color_at",
                                        "border_intact",
                                        "layout_columns",
                                        "no_overlap",
                                        "baseline_compare"
                                    ]
                                },
                                "name": { "type": "string" },
                                "text": { "type": "string" },
                                "baselinePath": { "type": "string" },
                                "baselineSidecarPath": { "type": "string" },
                                "caseSensitive": { "type": "boolean", "default": false },
                                "minOccurrences": { "type": "integer", "minimum": 1 },
                                "row": { "type": "integer", "minimum": 0 },
                                "column": { "type": "integer", "minimum": 0 },
                                "count": { "type": "integer", "minimum": 1 },
                                "minGap": { "type": "integer", "minimum": 1 },
                                "fg": {
                                    "oneOf": [
                                        { "type": "string" },
                                        {
                                            "type": "array",
                                            "items": { "type": "integer", "minimum": 0, "maximum": 255 },
                                            "minItems": 3,
                                            "maxItems": 3
                                        }
                                    ]
                                },
                                "bg": {
                                    "oneOf": [
                                        { "type": "string" },
                                        {
                                            "type": "array",
                                            "items": { "type": "integer", "minimum": 0, "maximum": 255 },
                                            "minItems": 3,
                                            "maxItems": 3
                                        }
                                    ]
                                },
                                "region": {
                                    "type": "object",
                                    "properties": {
                                        "top": { "type": "integer", "minimum": 0 },
                                        "left": { "type": "integer", "minimum": 0 },
                                        "bottom": { "type": "integer", "minimum": 0 },
                                        "right": { "type": "integer", "minimum": 0 }
                                    },
                                    "additionalProperties": false
                                },
                                "regions": {
                                    "type": "array",
                                    "minItems": 2,
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "name": { "type": "string" },
                                            "top": { "type": "integer", "minimum": 0 },
                                            "left": { "type": "integer", "minimum": 0 },
                                            "bottom": { "type": "integer", "minimum": 0 },
                                            "right": { "type": "integer", "minimum": 0 }
                                        },
                                        "required": ["top", "left", "bottom", "right"],
                                        "additionalProperties": false
                                    }
                                }
                            },
                            "required": ["type"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["checks"],
                "anyOf": [
                    { "required": ["artifactPath"] },
                    { "required": ["path"] },
                    { "required": ["sidecarPath"] }
                ],
                "additionalProperties": false
            }),
        },
    ]
}

#[derive(Debug)]
struct ToolReply {
    summary: String,
    data: Value,
    is_error: bool,
}

impl ToolReply {
    fn success(summary: String, data: Value) -> Self {
        Self {
            summary,
            data,
            is_error: false,
        }
    }

    fn error(summary: String, data: Value) -> Self {
        Self {
            summary,
            data,
            is_error: true,
        }
    }

    fn into_result(self) -> Value {
        let text = format!(
            "{}\n{}",
            self.summary,
            serde_json::to_string_pretty(&self.data).unwrap_or_else(|_| "{}".to_owned())
        );
        json!({
            "content": [{ "type": "text", "text": text }],
            "structuredContent": {
                "summary": self.summary,
                "data": self.data
            },
            "isError": self.is_error
        })
    }
}

#[derive(Debug)]
struct CommandOutput {
    command: String,
    success: bool,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
}

struct EnvOverrideGuard {
    previous: Vec<(String, Option<String>)>,
}

impl EnvOverrideGuard {
    fn new(overrides: BTreeMap<String, String>) -> Self {
        let mut previous = Vec::with_capacity(overrides.len());
        for (key, value) in overrides {
            previous.push((key.clone(), env::var(&key).ok()));
            env::set_var(&key, value);
        }
        Self { previous }
    }
}

impl Drop for EnvOverrideGuard {
    fn drop(&mut self) {
        while let Some((key, value)) = self.previous.pop() {
            match value {
                Some(value) => env::set_var(&key, value),
                None => env::remove_var(&key),
            }
        }
    }
}

pub fn command_output_data(output: &std::process::Output) -> Value {
    json!({
        "success": output.status.success(),
        "exitCode": output.status.code(),
        "stdout": String::from_utf8_lossy(&output.stdout),
        "stderr": String::from_utf8_lossy(&output.stderr)
    })
}

pub fn required_string(args: &Value, key: &str) -> Result<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("missing required string field `{key}`"))
}

pub fn optional_string(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(ToOwned::to_owned)
}

pub fn optional_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(Value::as_bool)
}

fn optional_object<'a>(args: &'a Value, key: &str) -> Option<&'a Map<String, Value>> {
    args.get(key).and_then(Value::as_object)
}

pub fn optional_u64(args: &Value, key: &str) -> Option<u64> {
    args.get(key).and_then(Value::as_u64)
}

pub fn required_u64(args: &Value, key: &str) -> Result<u64> {
    optional_u64(args, key).ok_or_else(|| anyhow!("missing required integer field `{key}`"))
}

fn host_toolchain_dir(name: &str) -> Option<String> {
    env::var_os("HOME").map(|home| PathBuf::from(home).join(name).display().to_string())
}

fn checked_u16(value: u64, key: &str) -> Result<u16> {
    u16::try_from(value).map_err(|_| anyhow!("field `{key}` must be <= {}", u16::MAX))
}

fn checked_usize(value: u64, key: &str) -> Result<usize> {
    usize::try_from(value).map_err(|_| anyhow!("field `{key}` is too large"))
}

pub fn optional_i64(args: &Value, key: &str) -> Option<i64> {
    args.get(key).and_then(Value::as_i64)
}

pub fn optional_string_array(args: &Value, key: &str) -> Result<Vec<String>> {
    match args.get(key) {
        None => Ok(Vec::new()),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| anyhow!("field `{key}` must contain only strings"))
            })
            .collect(),
        Some(_) => bail!("field `{key}` must be an array of strings"),
    }
}

pub fn parse_session_id(value: &str) -> Result<SessionId> {
    SessionId::parse(value).map_err(|error| anyhow!(error.to_string()))
}

pub fn parse_branch_id(value: &str) -> Result<BranchId> {
    BranchId::parse(value).map_err(|error| anyhow!(error.to_string()))
}

pub fn parse_message_id(value: &str) -> Result<MessageId> {
    MessageId::parse(value).map_err(|error| anyhow!(error.to_string()))
}

pub fn parse_swipe_group_id(value: &str) -> Result<SwipeGroupId> {
    SwipeGroupId::parse(value).map_err(|error| anyhow!(error.to_string()))
}

pub fn now_timestamp_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

pub fn parse_prefixed_field(text: &str, prefix: &str) -> Option<String> {
    text.lines().find_map(|line| {
        line.strip_prefix(prefix)
            .map(str::trim)
            .map(ToOwned::to_owned)
    })
}

fn normalize_preferences_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, nested)| {
                    (
                        normalize_preferences_key(key),
                        normalize_preferences_json(nested),
                    )
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(normalize_preferences_json).collect()),
        other => other.clone(),
    }
}

fn normalize_preferences_key(key: &str) -> String {
    let mut normalized = String::with_capacity(key.len() + 4);
    for ch in key.chars() {
        if ch == '-' {
            normalized.push('_');
        } else if ch.is_ascii_uppercase() {
            if !normalized.is_empty() {
                normalized.push('_');
            }
            normalized.push(ch.to_ascii_lowercase());
        } else {
            normalized.push(ch);
        }
    }
    normalized
}

pub fn default_preferences_json() -> Value {
    json!({
        "version": 1,
        "last_model_name": "",
        "last_context_size": null,
        "last_gpu_layers": null,
        "last_quant_kv": null,
        "last_threads": null,
        "last_blas_threads": null,
        "no_browser": false,
        "preferred_backend": null,
        "preferred_frontend": null,
        "preferred_tier": null,
        "side_by_side_monitor": false,
        "llamacpp_gpu_layers": null,
        "llamacpp_context_size": null,
        "llamacpp_threads": null,
        "theme_preset": "dark-mint",
        "show_inspector": false,
        "timestamp_style": "relative",
        "message_density": "comfortable"
    })
}

pub fn merge_json_objects(base: Value, overlay: Value) -> Value {
    match (base, overlay) {
        (Value::Object(mut base_map), Value::Object(overlay_map)) => {
            for (key, overlay_value) in overlay_map {
                let merged_value = match base_map.remove(&key) {
                    Some(base_value) => merge_json_objects(base_value, overlay_value),
                    None => overlay_value,
                };
                base_map.insert(key, merged_value);
            }
            Value::Object(base_map)
        }
        (_, overlay) => overlay,
    }
}

pub fn sanitize_prefix(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

pub fn probe_session_lock(repo: &SqliteRepository, session_id: &SessionId) -> Result<Value> {
    let instance_id = format!("ozone-mcp-{}", Uuid::new_v4().simple());
    match repo.acquire_session_lock(session_id, &instance_id) {
        Ok(lock) => {
            let released = repo.release_session_lock(session_id, &instance_id)?;
            Ok(json!({
                "status": "available",
                "instanceId": lock.instance_id,
                "acquiredAt": lock.acquired_at,
                "heartbeatAt": lock.heartbeat_at,
                "released": released
            }))
        }
        Err(PersistError::SessionLocked {
            instance_id,
            acquired_at,
        }) => Ok(json!({
            "status": "locked",
            "instanceId": instance_id,
            "acquiredAt": acquired_at
        })),
        Err(error) => Err(anyhow!(error.to_string())),
    }
}

pub fn session_summary_json(session: &ozone_persist::SessionSummary) -> Value {
    json!({
        "sessionId": session.session_id,
        "name": session.name,
        "characterName": session.character_name,
        "createdAt": session.created_at,
        "lastOpenedAt": session.last_opened_at,
        "messageCount": session.message_count,
        "dbSizeBytes": session.db_size_bytes,
        "tags": session.tags,
        "lastMessageId": serde_json::Value::Null
    })
}

pub fn branch_record_json(record: &BranchRecord) -> Value {
    json!({
        "branchId": record.branch.branch_id,
        "sessionId": record.branch.session_id,
        "name": record.branch.name,
        "state": record.branch.state.as_str(),
        "tipMessageId": record.branch.tip_message_id,
        "forkedFromMessageId": record.forked_from,
        "createdAt": record.branch.created_at,
        "description": record.branch.description
    })
}

pub fn message_json(message: &ConversationMessage) -> Value {
    json!({
        "messageId": message.message_id,
        "sessionId": message.session_id,
        "parentId": message.parent_id,
        "authorKind": message.author_kind,
        "authorName": message.author_name,
        "content": message.content,
        "createdAt": message.created_at,
        "editedAt": message.edited_at,
        "isHidden": message.is_hidden
    })
}

pub fn pinned_memory_record_json(record: &ozone_persist::PinnedMemoryRecord) -> Value {
    json!({
        "artifactId": record.artifact_id,
        "sessionId": record.session_id,
        "sourceMessageId": record.source_message_id,
        "provenance": record.provenance.as_str(),
        "createdAt": record.created_at,
        "snapshotVersion": record.snapshot_version,
        "text": record.content.text,
        "pinnedBy": record.content.pinned_by,
        "expiresAfterTurns": record.content.expires_after_turns
    })
}

pub fn pinned_memory_view_json(view: &PinnedMemoryView) -> Value {
    json!({
        "record": pinned_memory_record_json(&view.record),
        "isActive": view.is_active,
        "turnsElapsed": view.turns_elapsed,
        "remainingTurns": view.remaining_turns
    })
}

pub fn swipe_group_json(group: &SwipeGroup) -> Value {
    json!({
        "swipeGroupId": group.swipe_group_id,
        "parentMessageId": group.parent_message_id,
        "parentContextMessageId": group.parent_context_message_id,
        "activeOrdinal": group.active_ordinal
    })
}

pub fn swipe_candidate_json(candidate: &SwipeCandidate) -> Value {
    json!({
        "swipeGroupId": candidate.swipe_group_id,
        "ordinal": candidate.ordinal,
        "messageId": candidate.message_id,
        "state": candidate.state.as_str(),
        "partialContent": candidate.partial_content,
        "tokensGenerated": candidate.tokens_generated
    })
}

pub fn render_transcript_text(export: &ozone_persist::TranscriptExport) -> String {
    let mut lines = vec![
        "ozone+ transcript export".to_owned(),
        format!("session id: {}", export.session.session_id),
        format!("session name: {}", export.session.name),
    ];
    if let Some(branch) = &export.branch {
        lines.push(format!("branch id: {}", branch.branch_id));
        lines.push(format!("branch name: {}", branch.name));
    }
    lines.push(String::new());
    for message in &export.messages {
        let author = message
            .author_name
            .as_deref()
            .unwrap_or(&message.author_kind);
        lines.push(format!("[{}] {}", author, message.content));
        lines.push(String::new());
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::{
        capturable_screen_journey_builders, default_preferences_json, merge_json_objects,
        mock_user_capture_settings, normalize_preferences_json,
        screenshot_capture_config, tool_definitions, EnvOverrideGuard, OzoneMcpServer,
        Sandbox,
    };
    use crate::testing::{MockUserAction, MockUserJourneyStep};
    use ozone_core::session::SessionId;
    use serde_json::{json, Value};
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use uuid::Uuid;

    fn with_front_door_profile<T>(profile: &str, f: impl FnOnce() -> T) -> T {
        let mut overrides = BTreeMap::new();
        overrides.insert(
            "OZONE_MCP_FRONT_DOOR_PROFILE".to_owned(),
            profile.to_owned(),
        );
        let _guard = EnvOverrideGuard::new(overrides);
        f()
    }

    fn assert_release_front_door_binaries_exist(server: &OzoneMcpServer) {
        for binary in ["ozone", "ozone-plus"] {
            let path = server.repo_root.join("target/release").join(binary);
            assert!(
                path.is_file(),
                "missing release binary {} for release smoke; build target/release/{} first",
                path.display(),
                binary
            );
        }
    }

    fn assert_mock_user_success(data: &Value, context: &str) {
        let final_tail = data
            .get("finalTail")
            .and_then(Value::as_str)
            .unwrap_or_default();
        assert_eq!(
            data.get("runnerOk").and_then(Value::as_bool),
            Some(true),
            "{context}: PTY runner failed\n{}",
            final_tail
        );
        assert_eq!(
            data.get("success").and_then(Value::as_bool),
            Some(true),
            "{context}: journey markers were not reached\n{}",
            final_tail
        );
    }

    fn build_fresh_base_launcher_journey(
        server: &OzoneMcpServer,
    ) -> crate::testing::MockUserJourneySpec {
        crate::testing::MockUserJourneySpec {
            name: "release_fresh_base_launcher".to_owned(),
            cwd: server.repo_root.to_string_lossy().into_owned(),
            command: crate::testing::append_args(
                &crate::testing::front_door_binary_command(
                    &server.repo_root,
                    "ozone",
                    &["--mode", "base"],
                ),
                &["--no-browser"],
            ),
            steps: vec![MockUserJourneyStep::wait_for(
                "reach launcher",
                35000,
                ["Launch", "Open ozone+", "Settings"],
            )],
        }
    }

    fn run_release_binary(
        server: &OzoneMcpServer,
        sandbox_id: &str,
        binary: &str,
        args: &[&str],
    ) -> super::CommandOutput {
        let program = server.repo_root.join("target/release").join(binary);
        let program = program.display().to_string();
        let args = args.iter().map(|value| (*value).to_owned()).collect::<Vec<_>>();
        let output = server
            .run_workspace_command(&program, &args, Some(sandbox_id))
            .expect("release binary command");
        assert!(
            output.success,
            "release command failed: {}\nstdout:\n{}\nstderr:\n{}",
            output.command,
            output.stdout,
            output.stderr
        );
        output
    }

    fn first_session_id(server: &OzoneMcpServer, sandbox_id: &str) -> String {
        server
            .with_repo(Some(sandbox_id), |repo| {
                Ok(repo
                    .list_sessions()?
                    .into_iter()
                    .next()
                    .map(|session| session.session_id.to_string()))
            })
            .expect("list sessions")
            .expect("persisted session")
    }

    fn active_transcript_len(server: &OzoneMcpServer, sandbox_id: &str, session_id: &str) -> usize {
        let session_id = SessionId::parse(session_id).expect("valid session id");
        server
            .with_repo(Some(sandbox_id), |repo| Ok(repo.get_active_branch_transcript(&session_id)?.len()))
            .expect("active transcript")
    }

    fn release_smoke_artifact_dir(name: &str) -> PathBuf {
        let output_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("repo root")
            .join("target/test-artifacts/ozone-mcp")
            .join("release-smoke")
            .join(format!("{name}-{}", Uuid::new_v4()));
        fs::create_dir_all(&output_dir).expect("create release-smoke artifact dir");
        output_dir
    }

    #[test]
    fn mock_user_launcher_monitor_journey_contains_monitor_markers() {
        let server = OzoneMcpServer::new().expect("server");
        let journey = server
            .build_mock_user_journey("launcher_monitor_roundtrip", &json!({}))
            .expect("journey");
        assert!(matches!(
            journey.steps[2].action,
            MockUserAction::Key { .. }
        ));
        assert!(journey.steps[2]
            .expect_any
            .iter()
            .any(|marker| marker == "Confirm Launch"));
        assert!(journey.steps[3]
            .expect_any
            .iter()
            .any(|marker| marker == "Ozone Monitor"));
    }

    #[test]
    fn mock_user_chat_journey_includes_insert_and_response_markers() {
        let server = OzoneMcpServer::new().expect("server");
        let journey = server
            .build_mock_user_journey(
                "ozone_plus_chat_journey",
                &json!({ "prompt": "Check the observatory key" }),
            )
            .expect("journey");
        assert!(matches!(
            journey.steps[4].action,
            MockUserAction::Key { .. }
        ));
        assert!(journey.steps[4]
            .expect_any
            .iter()
            .any(|marker| marker == "Composer"));
        assert!(matches!(
            journey.steps[5].action,
            MockUserAction::Text { .. }
        ));
        assert!(journey.steps[5].expect_any.is_empty());
        assert!(matches!(
            journey.steps[6].action,
            MockUserAction::Key { .. }
        ));
        assert!(journey.steps[6]
            .expect_any
            .iter()
            .any(|marker| marker == "assistant"));
    }

    #[test]
    fn capturable_screen_library_covers_base_and_ozone_plus_entry_surfaces() {
        let screens: Vec<_> = capturable_screen_journey_builders()
            .iter()
            .map(|entry| entry.target_screen)
            .collect();
        assert_eq!(
            screens,
            vec![
                "base_splash",
                "base_tier_picker",
                "base_launcher",
                "base_exit_confirm",
                "base_settings",
                "base_model_picker_launch",
                "base_confirm_launch",
                "base_frontend_choice",
                "base_launching",
                "base_monitor",
                "base_model_picker_profile",
                "base_profile_advisory",
                "base_profile_confirm",
                "base_profile_running",
                "base_profile_failure",
                "base_ozone_plus_shell",
                "ozone_plus_main_menu",
                "ozone_plus_sessions",
                "ozone_plus_characters",
                "ozone_plus_character_create",
                "ozone_plus_character_import",
                "ozone_plus_settings",
                "ozone_plus_conversation",
                "ozone_plus_help",
            ]
        );
    }

    #[test]
    fn capturable_screen_journeys_build_expected_commands_and_markers() {
        let server = OzoneMcpServer::new().expect("server");
        let cases = [
            ("base_splash", "ozone", "Continue"),
            ("base_tier_picker", "ozone", "Choose Your Tier"),
            ("base_launcher", "ozone", "Open ozone+"),
            ("base_exit_confirm", "ozone", "Confirm Exit"),
            ("base_settings", "ozone", "Active Defaults"),
            ("base_model_picker_launch", "ozone", "Model Picker · Launch"),
            ("base_confirm_launch", "ozone", "Confirm Launch"),
            ("base_frontend_choice", "ozone", "Choose Frontend"),
            ("base_launching", "ozone", "Launching KoboldCpp"),
            ("base_monitor", "ozone", "Ozone Monitor"),
            (
                "base_model_picker_profile",
                "ozone",
                "Model Picker · Profile",
            ),
            ("base_profile_advisory", "ozone", "Profiling Advisor"),
            ("base_profile_confirm", "ozone", "Confirm Profiling Step"),
            ("base_profile_running", "ozone", "Profiling In Progress"),
            ("base_profile_failure", "ozone", "Profiling Failed"),
            ("base_ozone_plus_shell", "ozone", "New Chat"),
            ("ozone_plus_main_menu", "ozone-plus", "New Chat"),
            ("ozone_plus_sessions", "ozone-plus", "Sessions"),
            ("ozone_plus_characters", "ozone-plus", "Characters"),
            ("ozone_plus_character_create", "ozone-plus", "New Character"),
            (
                "ozone_plus_character_import",
                "ozone-plus",
                "Import Character Card",
            ),
            ("ozone_plus_settings", "ozone-plus", "config.toml"),
            ("ozone_plus_conversation", "ozone-plus", "Conversation"),
            ("ozone_plus_help", "ozone-plus", "Slash Commands"),
        ];

        for (screen, command_fragment, marker) in cases {
            let journey = server
                .build_capturable_screen_journey(screen, &json!({}), screen)
                .unwrap_or_else(|error| panic!("failed to build {screen}: {error}"));
            assert!(
                journey
                    .command
                    .iter()
                    .any(|part| part.contains(command_fragment)),
                "{screen} should use {command_fragment:?}: {:?}",
                journey.command
            );
            assert!(
                journey
                    .steps
                    .iter()
                    .flat_map(|step| step.expect_any.iter())
                    .any(|value| value == marker),
                "{screen} should expect marker {marker:?}"
            );
        }
    }

    #[test]
    fn launcher_to_ozone_plus_journey_reuses_capturable_screen_spec() {
        let server = OzoneMcpServer::new().expect("server");
        let from_screen = server
            .build_capturable_screen_journey(
                "base_ozone_plus_shell",
                &json!({ "prompt": "ignored" }),
                "launcher_to_ozone_plus",
            )
            .expect("screen journey");
        let from_mock_user = server
            .build_mock_user_journey("launcher_to_ozone_plus", &json!({ "prompt": "ignored" }))
            .expect("mock-user journey");
        assert_eq!(from_mock_user, from_screen);
    }

    #[test]
    fn mock_user_target_lookup_builds_screen_journey() {
        let server = OzoneMcpServer::new().expect("server");
        let journey = server
            .build_mock_user_target_journey("ozone_plus_help")
            .expect("screen journey");
        assert_eq!(journey.name, "ozone_plus_help");
        assert!(journey
            .steps
            .iter()
            .flat_map(|step| step.expect_any.iter())
            .any(|value| value == "Slash Commands"));
    }

    #[test]
    fn mock_user_tool_is_listed_with_capture_inputs() {
        let definition = tool_definitions()
            .into_iter()
            .find(|tool| tool.name == "mock_user_tool")
            .expect("mock_user_tool");
        assert_eq!(
            definition.input_schema["properties"]["captureScreenshots"]["default"],
            json!(false)
        );
        assert!(definition.input_schema["properties"]
            .get("outputDir")
            .is_some());
        assert!(definition.input_schema["properties"].get("rows").is_some());
        assert!(definition.input_schema["properties"]
            .get("columns")
            .is_some());
        assert!(definition.input_schema["properties"]
            .get("fontSize")
            .is_some());
        assert!(definition.input_schema.get("required").is_none());
        assert_eq!(
            definition.input_schema["anyOf"],
            json!([{ "required": ["journey"] }, { "required": ["target"] }])
        );
    }

    #[test]
    fn mock_user_capture_settings_add_step_artifacts_when_enabled() {
        let server = OzoneMcpServer::new().expect("server");
        let journey = server
            .build_mock_user_journey("launcher_to_ozone_plus", &json!({}))
            .expect("journey");
        let sandbox = Sandbox {
            id: "sandbox-123".to_owned(),
            root: PathBuf::from("/sandbox"),
            data_home: PathBuf::from("/sandbox/data"),
            home: PathBuf::from("/sandbox/home"),
            models_dir: PathBuf::from("/sandbox/models"),
            launcher_script: None,
            backend: None,
        };
        let settings = mock_user_capture_settings(
            &json!({
                "captureScreenshots": true,
                "outputDir": "captures/custom",
                "rows": 55,
                "columns": 140,
                "fontSize": 18
            }),
            &sandbox,
            &journey,
            None,
        )
        .expect("capture settings");
        assert!(settings.capture_screenshots);
        assert_eq!(
            settings.output_dir.as_deref(),
            Some("/sandbox/captures/custom")
        );
        assert_eq!(settings.capture.rows, 55);
        assert_eq!(settings.capture.columns, 140);
        assert_eq!(settings.capture.font_size, 18);
        assert_eq!(settings.step_captures.len(), journey.steps.len());
        assert_eq!(
            settings.capture.png_path.as_deref(),
            Some("/sandbox/captures/custom/final.png")
        );
        assert_eq!(
            settings.capture.json_path.as_deref(),
            Some("/sandbox/captures/custom/final.json")
        );
    }

    #[test]
    fn screenshot_tool_is_listed_with_required_inputs() {
        let definition = tool_definitions()
            .into_iter()
            .find(|tool| tool.name == "screenshot_tool")
            .expect("screenshot tool");
        assert_eq!(
            definition.input_schema["required"],
            json!(["target", "outputDir"])
        );
        assert_eq!(
            definition.input_schema["properties"]["target"]["enum"]
                .as_array()
                .expect("target enum")
                .len(),
            capturable_screen_journey_builders().len()
        );
    }

    #[test]
    fn screenshot_capture_config_uses_requested_output_settings() {
        let config = screenshot_capture_config(
            &json!({
                "filename": "launcher.png",
                "dimensions": { "rows": 55, "columns": 140 },
                "fontSize": 18,
                "tailChars": 2048
            }),
            &PathBuf::from("/repo/captures"),
            "base_launcher",
        )
        .expect("capture config");
        assert_eq!(config.rows, 55);
        assert_eq!(config.columns, 140);
        assert_eq!(config.font_size, 18);
        assert_eq!(config.tail_chars, 2048);
        assert_eq!(
            config.png_path.as_deref(),
            Some("/repo/captures/launcher.png")
        );
        assert_eq!(
            config.json_path.as_deref(),
            Some("/repo/captures/launcher.json")
        );
    }

    #[test]
    fn screenshot_tool_reports_clear_error_for_unknown_target() {
        let mut server = OzoneMcpServer::new().expect("server");
        let error = server
            .screenshot_tool(&json!({
                "sandboxId": "sandbox-123",
                "target": "does_not_exist",
                "outputDir": "captures"
            }))
            .expect_err("invalid target should fail");
        assert!(error
            .to_string()
            .contains("use `screen_nav_targets` to list valid targets"));
    }

    #[test]
    fn screen_check_tool_is_listed_with_required_inputs() {
        let definition = tool_definitions()
            .into_iter()
            .find(|tool| tool.name == "screen_check_tool")
            .expect("screen check tool");
        assert_eq!(definition.input_schema["required"], json!(["checks"]));
        assert_eq!(
            definition.input_schema["anyOf"],
            json!([
                { "required": ["artifactPath"] },
                { "required": ["path"] },
                { "required": ["sidecarPath"] }
            ])
        );
        assert_eq!(
            definition.input_schema["properties"]["checks"]["items"]["properties"]["type"]["enum"],
            json!([
                "text_present",
                "text_absent",
                "color_at",
                "border_intact",
                "layout_columns",
                "no_overlap",
                "baseline_compare"
            ])
        );
    }

    #[test]
    fn chat_journey_auto_setup_enables_mock_backend() {
        let server = OzoneMcpServer::new().expect("server");
        let setup = server
            .recommended_mock_user_journey_sandbox_setup("ozone_plus_chat_journey")
            .expect("setup");
        assert_eq!(setup["models"], json!(["mock-model.gguf"]));
        assert_eq!(setup["createLauncherStub"], json!(true));
        assert_eq!(setup["requiresMockBackend"], json!(true));
    }

    #[test]
    fn normalize_preferences_json_converts_camel_case_keys() {
        let normalized = merge_json_objects(
            default_preferences_json(),
            normalize_preferences_json(&json!({
                "preferredTier": "base",
                "preferredBackend": "KoboldCpp",
                "preferredFrontend": "OzonePlus",
                "sideBySideMonitor": true
            })),
        );
        assert_eq!(normalized["version"], json!(1));
        assert_eq!(normalized["preferred_tier"], json!("base"));
        assert_eq!(normalized["preferred_backend"], json!("KoboldCpp"));
        assert_eq!(normalized["preferred_frontend"], json!("OzonePlus"));
        assert_eq!(normalized["side_by_side_monitor"], json!(true));
    }

    #[test]
    fn screen_check_tool_passes_core_checks_against_fixture() {
        let fixture = screen_check_fixture_path();
        let server = OzoneMcpServer::new().expect("server");
        let reply = server
            .screen_check_tool(&json!({
                "artifactPath": fixture.with_extension("png").display().to_string(),
                "checks": [
                    { "type": "text_present", "text": "Menu" },
                    { "type": "text_absent", "text": "Danger" },
                    { "type": "color_at", "row": 1, "column": 1, "fg": "yellow", "bg": [12, 12, 12] },
                    { "type": "border_intact", "region": { "top": 0, "left": 0, "bottom": 4, "right": 17 } },
                    { "type": "layout_columns", "count": 2, "region": { "top": 1, "left": 1, "bottom": 2, "right": 15 }, "minGap": 2 },
                    {
                        "type": "no_overlap",
                        "regions": [
                            { "name": "left", "top": 1, "left": 1, "bottom": 2, "right": 2 },
                            { "name": "right", "top": 1, "left": 6, "bottom": 2, "right": 7 }
                        ]
                    }
                ]
            }))
            .expect("screen check reply");
        assert!(!reply.is_error, "{}", reply.summary);
        assert_eq!(reply.data["summary"]["passed"], json!(6));
        assert_eq!(reply.data["summary"]["failed"], json!(0));
        assert_eq!(reply.data["checks"].as_array().expect("checks").len(), 6);
    }

    #[test]
    fn screen_check_tool_returns_error_reply_when_check_fails() {
        let fixture = screen_check_fixture_path();
        let server = OzoneMcpServer::new().expect("server");
        let reply = server
            .screen_check_tool(&json!({
                "artifactPath": fixture.display().to_string(),
                "checks": [
                    { "type": "text_absent", "text": "Menu" }
                ]
            }))
            .expect("screen check reply");
        assert!(reply.is_error, "{}", reply.summary);
        assert_eq!(reply.data["summary"]["failed"], json!(1));
        assert_eq!(reply.data["checks"][0]["passed"], json!(false));
    }

    #[test]
    fn screen_check_tool_passes_baseline_compare_against_matching_sidecar() {
        let fixture = screen_check_fixture_path();
        let server = OzoneMcpServer::new().expect("server");
        let reply = server
            .screen_check_tool(&json!({
                "artifactPath": fixture.display().to_string(),
                "checks": [
                    { "type": "baseline_compare", "baselinePath": fixture.display().to_string() }
                ]
            }))
            .expect("screen check reply");
        assert!(!reply.is_error, "{}", reply.summary);
        assert_eq!(reply.data["summary"]["passed"], json!(1));
        assert_eq!(reply.data["checks"][0]["passed"], json!(true));
        assert_eq!(reply.data["checks"][0]["detail"]["diffCount"], json!(0));
        assert_eq!(reply.data["checks"][0]["detail"]["changedCells"], json!([]));
        assert_eq!(
            reply.data["checks"][0]["detail"]["differenceSummary"],
            json!("No grid differences detected")
        );
        assert_eq!(
            reply.data["checks"][0]["detail"]["matchPercent"],
            json!(100.0)
        );
    }

    #[test]
    fn screen_check_tool_reports_baseline_compare_differences() {
        let fixture = screen_check_fixture_path();
        let differing_baseline = write_modified_baseline_fixture();
        let server = OzoneMcpServer::new().expect("server");
        let reply = server
            .screen_check_tool(&json!({
                "artifactPath": fixture.display().to_string(),
                "checks": [
                    {
                        "type": "baseline_compare",
                        "baselineSidecarPath": differing_baseline.path().display().to_string()
                    }
                ]
            }))
            .expect("screen check reply");
        assert!(reply.is_error, "{}", reply.summary);
        assert_eq!(reply.data["summary"]["failed"], json!(1));
        assert_eq!(reply.data["checks"][0]["passed"], json!(false));
        assert_eq!(reply.data["checks"][0]["detail"]["diffCount"], json!(1));
        assert_eq!(
            reply.data["checks"][0]["detail"]["changedCells"][0],
            json!({ "row": 0, "column": 2 })
        );
        assert_eq!(
            reply.data["checks"][0]["detail"]["sampleDiffs"][0]["kind"],
            json!("changed")
        );
        assert!(reply.data["checks"][0]["detail"]["differenceSummary"]
            .as_str()
            .expect("difference summary")
            .contains("1 cell diff(s)"));
    }

    #[test]
    fn screen_check_tool_reports_clear_error_for_missing_sidecar() {
        let server = OzoneMcpServer::new().expect("server");
        let error = server
            .screen_check_tool(&json!({
                "artifactPath": "does/not/exist.png",
                "checks": [
                    { "type": "text_present", "text": "Menu" }
                ]
            }))
            .expect_err("missing sidecar should fail");
        assert!(error.to_string().contains("screen capture sidecar"));
    }

    #[test]
    #[ignore = "release smoke"]
    fn release_smoke_gate_fresh_temp_xdg_user_path() {
        with_front_door_profile("release", || {
            let mut server = OzoneMcpServer::new().expect("server");
            assert_release_front_door_binaries_exist(&server);

            let launcher_sandbox = server
                .prepare_sandbox_from_setup(None, crate::testing::sandbox_setup_base_launch_path())
                .expect("launcher sandbox");
            let launcher_journey = build_fresh_base_launcher_journey(&server);
            let launcher_capture_args = json!({
                "filename": "base-launcher-release",
                "rows": 55,
                "columns": 140,
                "fontSize": 18,
                "tailChars": 2048
            });
            let launcher_capture_dir = release_smoke_artifact_dir("base-launcher");
            let launcher_capture = screenshot_capture_config(
                &launcher_capture_args,
                &launcher_capture_dir,
                "base_launcher",
            )
            .expect("launcher capture config");
            let launcher_run = server
                .run_mock_user_journey(
                    &launcher_sandbox.sandbox_id,
                    &launcher_journey,
                    Some("base_launcher".to_owned()),
                    &launcher_capture_args,
                    Some(launcher_capture),
                )
                .expect("launcher run");
            assert_mock_user_success(&launcher_run, "fresh temp-XDG base launcher");

            let launcher_sidecar = launcher_run["paths"]["json"].as_str().unwrap_or_else(|| {
                panic!(
                    "fresh temp-XDG base launcher capture did not produce a sidecar\n{}",
                    serde_json::to_string_pretty(&launcher_run)
                        .unwrap_or_else(|_| launcher_run.to_string())
                )
            });
            assert!(
                Path::new(launcher_sidecar).is_file(),
                "fresh temp-XDG base launcher sidecar missing: {launcher_sidecar}"
            );
            let launcher_screen = server
                .screen_check_tool(&json!({
                    "sidecarPath": launcher_sidecar,
                    "checks": [
                        { "type": "text_present", "text": "Launch" },
                        { "type": "text_present", "text": "Open ozone+" },
                        { "type": "text_present", "text": "Settings" },
                        { "type": "text_absent", "text": "local-first AI tooling" }
                    ]
                }))
                .expect("launcher screen check");
            assert!(
                !launcher_screen.is_error,
                "fresh temp-XDG launcher screen check failed: {}\n{}",
                launcher_screen.summary,
                serde_json::to_string_pretty(&launcher_screen.data)
                    .unwrap_or_else(|_| launcher_screen.data.to_string())
            );

            let mut settings_journey = crate::testing::build_base_settings_screen_journey(
                &server.repo_root,
                "release_fresh_base_settings",
                &json!({}),
            )
            .expect("settings journey");
            if let Some(step) = settings_journey.steps.last_mut() {
                step.expect_any.clear();
                step.settle_ms = step.settle_ms.max(1000);
            }
            let settings_capture_args = json!({
                "filename": "base-settings-release",
                "rows": 55,
                "columns": 140,
                "fontSize": 18,
                "tailChars": 2048
            });
            let settings_capture_dir = release_smoke_artifact_dir("base-settings");
            let settings_capture = screenshot_capture_config(
                &settings_capture_args,
                &settings_capture_dir,
                "base_settings",
            )
            .expect("settings capture config");
            let settings_run = server
                .run_mock_user_journey(
                    &launcher_sandbox.sandbox_id,
                    &settings_journey,
                    Some("base_settings".to_owned()),
                    &settings_capture_args,
                    Some(settings_capture),
                )
                .expect("settings run");
            assert_mock_user_success(&settings_run, "fresh temp-XDG base settings");

            let settings_sidecar = settings_run["paths"]["json"].as_str().unwrap_or_else(|| {
                panic!(
                    "fresh temp-XDG base settings capture did not produce a sidecar\n{}",
                    serde_json::to_string_pretty(&settings_run)
                        .unwrap_or_else(|_| settings_run.to_string())
                )
            });
            assert!(
                Path::new(settings_sidecar).is_file(),
                "fresh temp-XDG base settings sidecar missing: {settings_sidecar}"
            );
            let settings_screen = server
                .screen_check_tool(&json!({
                    "sidecarPath": settings_sidecar,
                    "checks": [
                        { "type": "text_present", "text": "Settings" },
                        { "type": "text_present", "text": "Active Defaults" },
                        { "type": "text_present", "text": "Backend" },
                        { "type": "text_present", "text": "Frontend" }
                    ]
                }))
                .expect("settings screen check");
            assert!(
                !settings_screen.is_error,
                "fresh temp-XDG settings screen check failed: {}\n{}",
                settings_screen.summary,
                serde_json::to_string_pretty(&settings_screen.data)
                    .unwrap_or_else(|_| settings_screen.data.to_string())
            );

            let confirm_launch_journey = crate::testing::build_base_confirm_launch_screen_journey(
                &server.repo_root,
                "release_fresh_base_confirm_launch",
                &json!({}),
            )
            .expect("confirm launch journey");
            let confirm_launch_capture_args = json!({
                "filename": "base-confirm-launch-release",
                "rows": 55,
                "columns": 140,
                "fontSize": 18,
                "tailChars": 2048
            });
            let confirm_launch_capture_dir = release_smoke_artifact_dir("base-confirm-launch");
            let confirm_launch_capture = screenshot_capture_config(
                &confirm_launch_capture_args,
                &confirm_launch_capture_dir,
                "base_confirm_launch",
            )
            .expect("confirm launch capture config");
            let confirm_launch_run = server
                .run_mock_user_journey(
                    &launcher_sandbox.sandbox_id,
                    &confirm_launch_journey,
                    Some("base_confirm_launch".to_owned()),
                    &confirm_launch_capture_args,
                    Some(confirm_launch_capture),
                )
                .expect("confirm launch run");
            assert_mock_user_success(
                &confirm_launch_run,
                "fresh temp-XDG base confirm launch",
            );

            let confirm_launch_sidecar = confirm_launch_run["paths"]["json"]
                .as_str()
                .unwrap_or_else(|| {
                    panic!(
                        "fresh temp-XDG base confirm launch capture did not produce a sidecar\n{}",
                        serde_json::to_string_pretty(&confirm_launch_run)
                            .unwrap_or_else(|_| confirm_launch_run.to_string())
                    )
                });
            assert!(
                Path::new(confirm_launch_sidecar).is_file(),
                "fresh temp-XDG base confirm launch sidecar missing: {confirm_launch_sidecar}"
            );
            let confirm_launch_screen = server
                .screen_check_tool(&json!({
                    "sidecarPath": confirm_launch_sidecar,
                    "checks": [
                        { "type": "text_present", "text": "Confirm Launch" },
                        { "type": "text_present", "text": "Context:" },
                        { "type": "text_present", "text": "QuantKV:" },
                        { "type": "text_absent", "text": "Choose Frontend" }
                    ]
                }))
                .expect("confirm launch screen check");
            assert!(
                !confirm_launch_screen.is_error,
                "fresh temp-XDG confirm launch screen check failed: {}\n{}",
                confirm_launch_screen.summary,
                serde_json::to_string_pretty(&confirm_launch_screen.data)
                    .unwrap_or_else(|_| confirm_launch_screen.data.to_string())
            );

            let frontend_choice_journey =
                crate::testing::build_base_frontend_choice_screen_journey(
                    &server.repo_root,
                    "release_fresh_base_frontend_choice",
                    &json!({}),
                )
                .expect("frontend choice journey");
            let frontend_choice_capture_args = json!({
                "filename": "base-frontend-choice-release",
                "rows": 55,
                "columns": 140,
                "fontSize": 18,
                "tailChars": 2048
            });
            let frontend_choice_capture_dir = release_smoke_artifact_dir("base-frontend-choice");
            let frontend_choice_capture = screenshot_capture_config(
                &frontend_choice_capture_args,
                &frontend_choice_capture_dir,
                "base_frontend_choice",
            )
            .expect("frontend choice capture config");
            let frontend_choice_run = server
                .run_mock_user_journey(
                    &launcher_sandbox.sandbox_id,
                    &frontend_choice_journey,
                    Some("base_frontend_choice".to_owned()),
                    &frontend_choice_capture_args,
                    Some(frontend_choice_capture),
                )
                .expect("frontend choice run");
            assert_mock_user_success(
                &frontend_choice_run,
                "fresh temp-XDG base frontend choice",
            );

            let frontend_choice_sidecar = frontend_choice_run["paths"]["json"]
                .as_str()
                .unwrap_or_else(|| {
                    panic!(
                        "fresh temp-XDG base frontend choice capture did not produce a sidecar\n{}",
                        serde_json::to_string_pretty(&frontend_choice_run)
                            .unwrap_or_else(|_| frontend_choice_run.to_string())
                    )
                });
            assert!(
                Path::new(frontend_choice_sidecar).is_file(),
                "fresh temp-XDG base frontend choice sidecar missing: {frontend_choice_sidecar}"
            );
            let frontend_choice_screen = server
                .screen_check_tool(&json!({
                    "sidecarPath": frontend_choice_sidecar,
                    "checks": [
                        { "type": "text_present", "text": "Choose Frontend" },
                        { "type": "text_present", "text": "SillyTavern" },
                        { "type": "text_present", "text": "ozone+" },
                        { "type": "text_absent", "text": "Launching KoboldCpp" }
                    ]
                }))
                .expect("frontend choice screen check");
            assert!(
                !frontend_choice_screen.is_error,
                "fresh temp-XDG frontend choice screen check failed: {}\n{}",
                frontend_choice_screen.summary,
                serde_json::to_string_pretty(&frontend_choice_screen.data)
                    .unwrap_or_else(|_| frontend_choice_screen.data.to_string())
            );

            let chat_sandbox = server
                .prepare_mock_user_sandbox(None, Some("ozone_plus_chat_journey"), None)
                .expect("chat sandbox");
            run_release_binary(
                &server,
                &chat_sandbox.sandbox_id,
                "ozone-plus",
                &["create", "Release Smoke Fresh"],
            );
            let session_id = first_session_id(&server, &chat_sandbox.sandbox_id);
            run_release_binary(
                &server,
                &chat_sandbox.sandbox_id,
                "ozone-plus",
                &["send", session_id.as_str(), "Check the observatory key"],
            );

            let transcript_len = active_transcript_len(&server, &chat_sandbox.sandbox_id, &session_id);
            assert!(
                transcript_len >= 2,
                "fresh-user ozone+ smoke should persist a non-empty transcript"
            );
        });
    }

    #[test]
    #[ignore = "release smoke"]
    fn release_smoke_gate_existing_user_data_path() {
        with_front_door_profile("release", || {
            let mut server = OzoneMcpServer::new().expect("server");
            assert_release_front_door_binaries_exist(&server);

            let first_args = json!({
                "prompt": "Remember the observatory key"
            });
            let prepared = server
                .prepare_mock_user_sandbox(None, Some("ozone_plus_chat_journey"), None)
                .expect("prepared sandbox");
            run_release_binary(
                &server,
                &prepared.sandbox_id,
                "ozone-plus",
                &["create", "Release Smoke Existing"],
            );
            let session_id = first_session_id(&server, &prepared.sandbox_id);
            run_release_binary(
                &server,
                &prepared.sandbox_id,
                "ozone-plus",
                &[
                    "send",
                    session_id.as_str(),
                    first_args["prompt"].as_str().expect("first prompt"),
                ],
            );

            let existing_session_count = server
                .with_repo(Some(&prepared.sandbox_id), |repo| Ok(repo.list_sessions()?.len()))
                .expect("existing session count");
            assert!(
                existing_session_count >= 1,
                "existing-user smoke needs persisted data before the second pass"
            );
            let initial_transcript_len =
                active_transcript_len(&server, &prepared.sandbox_id, &session_id);

            let second_args = json!({
                "prompt": "Use the existing data path and answer again"
            });
            let list_output = run_release_binary(
                &server,
                &prepared.sandbox_id,
                "ozone-plus",
                &["list"],
            );
            assert!(
                list_output.stdout.contains(&session_id),
                "existing-user smoke should list the persisted session"
            );
            run_release_binary(
                &server,
                &prepared.sandbox_id,
                "ozone-plus",
                &[
                    "send",
                    session_id.as_str(),
                    second_args["prompt"].as_str().expect("second prompt"),
                ],
            );

            let final_session_count = server
                .with_repo(Some(&prepared.sandbox_id), |repo| Ok(repo.list_sessions()?.len()))
                .expect("final session count");
            assert!(
                final_session_count >= existing_session_count,
                "existing-user smoke should preserve or grow persisted session state"
            );
            let final_transcript_len = active_transcript_len(&server, &prepared.sandbox_id, &session_id);
            assert!(
                final_transcript_len > initial_transcript_len,
                "existing-user smoke should append to the persisted transcript"
            );
        });
    }

    fn screen_check_fixture_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/screen-check-fixture.json")
    }

    struct TestSidecarFile {
        path: PathBuf,
    }

    impl TestSidecarFile {
        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestSidecarFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    fn write_modified_baseline_fixture() -> TestSidecarFile {
        let fixture_path = screen_check_fixture_path();
        let mut capture: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(&fixture_path).expect("read screen-check fixture"),
        )
        .expect("parse screen-check fixture");
        capture["rows"][0]["cells"][2]["text"] = json!("X");
        capture["rows"][0]["text"] = json!("┌─Xenu───────────┐");
        capture["display"][0] = json!("┌─Xenu───────────┐");
        capture["text"] = json!(capture["text"]
            .as_str()
            .expect("fixture text")
            .replacen("Menu", "Xenu", 1));
        capture["tailText"] = json!(capture["tailText"]
            .as_str()
            .expect("fixture tailText")
            .replacen("Menu", "Xenu", 1));

        let output_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("repo root")
            .join("target/test-artifacts/ozone-mcp");
        fs::create_dir_all(&output_dir).expect("create test-artifact dir");
        let path = output_dir.join(format!("baseline-compare-{}.json", Uuid::new_v4()));
        fs::write(
            &path,
            serde_json::to_vec_pretty(&capture).expect("serialize modified baseline fixture"),
        )
        .expect("write modified baseline fixture");
        TestSidecarFile { path }
    }
}
