#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
use std::{
    collections::BTreeMap,
    env,
    io::{self, BufReader, Write},
    path::{Path, PathBuf},
    process::Command,
};

use sandbox::Sandbox;

use anyhow::{anyhow, bail, Context, Result};
#[cfg(feature = "legacy-tools")]
use ozone_persist::{
    BranchRecord, PersistError, PinnedMemoryView, PinnedMemoryRecord, SqliteRepository,
    SessionSummary, TranscriptExport,
};
use serde::Serialize;
use serde_json::{json, Map, Value};

mod jsonrpc;
mod sandbox;
mod testing;
mod tool_dispatch;
mod tools;
mod arg_helpers;
mod persist_helpers;
mod types;
mod tool_defs;

use self::arg_helpers::*;
use self::persist_helpers::*;
use self::tool_defs::*;
use self::types::*;

use self::jsonrpc::{
    error_response, read_message, success_response, write_message, JsonRpcRequest,
};

use testing::{
    sandbox_setup_base_launch_path, sandbox_setup_base_launcher,
    sandbox_setup_base_ozone_plus_shell, sandbox_setup_base_profile_review,
    sandbox_setup_base_profile_run, sandbox_setup_base_splash, sandbox_setup_base_tier_picker,
    sandbox_setup_ozone_plus_entry, CapturableScreenJourneyDefinition,
    MockUserCaptureSettings, MockUserJourneySpec, MockUserRunnerSpec, PreparedSandbox,
    PtyVteCaptureArtifacts, PtyVteCaptureConfig,
};

const JSONRPC_VERSION: &str = "2.0";
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
pub const OZONE_PLUS_PACKAGE: &str = "ozone-plus";
const DEFAULT_PTY_ROWS: u16 = 40;
const DEFAULT_PTY_COLUMNS: u16 = 120;
const DEFAULT_CAPTURE_TAIL_CHARS: usize = 6000;
const DEFAULT_CAPTURE_FONT_SIZE: u16 = 16;
const ENV_ENABLE_LEGACY_TOOLS: &str = "OZONE_MCP_ENABLE_LEGACY_TOOLS";
const LEGACY_TOOL_NAMES: &[&str] = &[
    "mock_backend_tool",
    "launcher_smoke",
    "session_tool",
    "message_tool",
    "memory_tool",
    "search_tool",
    "branch_tool",
    "swipe_tool",
    "export_tool",
    "import_card",
];
const LEGACY_CAPTURE_TARGETS: &[&str] = &[
    "base_ozone_plus_shell",
    "ozone_plus_main_menu",
    "ozone_plus_sessions",
    "ozone_plus_characters",
    "ozone_plus_character_create",
    "ozone_plus_character_import",
    "ozone_plus_settings",
    "ozone_plus_conversation",
    "ozone_plus_help",
];
const ACTIVE_MOCK_USER_JOURNEYS: &[&str] = &["launcher_monitor_roundtrip"];
const LEGACY_MOCK_USER_JOURNEYS: &[&str] = &[
    "launcher_monitor_roundtrip",
    "launcher_to_ozone_plus",
    "ozone_plus_chat_journey",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolScope {
    ActiveRc,
    LegacyArchived,
}

pub(crate) fn legacy_tools_enabled() -> bool {
    env_flag_enabled(ENV_ENABLE_LEGACY_TOOLS)
}

fn env_flag_enabled(name: &str) -> bool {
    env::var(name)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

pub(crate) fn is_legacy_tool_name(tool_name: &str) -> bool {
    LEGACY_TOOL_NAMES.contains(&tool_name)
}

pub(crate) fn is_legacy_capture_target(target_name: &str) -> bool {
    LEGACY_CAPTURE_TARGETS.contains(&target_name)
}

pub(crate) fn is_legacy_mock_user_journey(journey_name: &str) -> bool {
    LEGACY_MOCK_USER_JOURNEYS.contains(&journey_name)
        && !ACTIVE_MOCK_USER_JOURNEYS.contains(&journey_name)
}

pub(crate) fn scoped_capture_targets(
    include_legacy: bool,
) -> Vec<&'static CapturableScreenJourneyDefinition> {
    capturable_screen_journey_builders()
        .iter()
        .filter(|entry| include_legacy || !is_legacy_capture_target(entry.target_screen))
        .collect()
}

fn mock_user_journey_names(include_legacy: bool) -> &'static [&'static str] {
    if include_legacy {
        LEGACY_MOCK_USER_JOURNEYS
    } else {
        ACTIVE_MOCK_USER_JOURNEYS
    }
}

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

pub fn capturable_screen_journey_builders() -> &'static [testing::CapturableScreenJourneyDefinition]
{
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
    #[cfg_attr(not(feature = "legacy-tools"), allow(dead_code))]
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

    #[cfg_attr(not(feature = "legacy-tools"), allow(dead_code))]
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



#[cfg(test)]
mod tests;
