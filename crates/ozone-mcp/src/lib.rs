use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use ozone_core::{
    engine::{
        ActivateSwipeCommand, BranchId, BranchState, ConversationBranch, ConversationMessage,
        CreateBranchCommand, MessageId, SwipeCandidate, SwipeCandidateState, SwipeGroup,
        SwipeGroupId,
    },
    paths,
    session::{CreateSessionRequest, SessionId},
};
use ozone_persist::{
    AuthorId, BranchRecord, CharacterCard, CreateMessageRequest, CreateNoteMemoryRequest,
    ImportCharacterCardRequest, PersistError, PinMessageMemoryRequest, PinnedMemoryView,
    Provenance, SqliteRepository,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use uuid::Uuid;

const JSONRPC_VERSION: &str = "2.0";
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
const OZONE_PLUS_PACKAGE: &str = "ozone-plus";
const DEFAULT_PTY_ROWS: u16 = 40;
const DEFAULT_PTY_COLUMNS: u16 = 120;
const DEFAULT_CAPTURE_TAIL_CHARS: usize = 1600;
const DEFAULT_CAPTURE_FONT_SIZE: u16 = 16;
const DEFAULT_LAYOUT_MIN_GAP: usize = 2;
const DEFAULT_BORDER_MAX_BLANK_RUN: usize = 1;
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
        let reply = match tool_name {
            "workspace_status" => self.workspace_status_tool()?,
            "cargo_tool" => self.cargo_tool(&arguments)?,
            "catalog_list" => self.catalog_list_tool(&arguments)?,
            "preferences_get" => self.preferences_get_tool(&arguments)?,
            "sandbox_tool" => self.sandbox_tool(&arguments)?,
            "mock_backend_tool" => self.mock_backend_tool(&arguments)?,
            "session_tool" => self.session_tool(&arguments)?,
            "message_tool" => self.message_tool(&arguments)?,
            "memory_tool" => self.memory_tool(&arguments)?,
            "search_tool" => self.search_tool(&arguments)?,
            "branch_tool" => self.branch_tool(&arguments)?,
            "swipe_tool" => self.swipe_tool(&arguments)?,
            "export_tool" => self.export_tool(&arguments)?,
            "import_card" => self.import_card_tool(&arguments)?,
            "launcher_smoke" => self.launcher_smoke_tool(&arguments)?,
            "screen_nav_targets" => self.screen_nav_targets_tool(&arguments)?,
            "mock_user_tool" => self.mock_user_tool(&arguments)?,
            "screenshot_tool" => self.screenshot_tool(&arguments)?,
            "screen_check_tool" => self.screen_check_tool(&arguments)?,
            _ => ToolReply::error(
                "Unknown tool".to_owned(),
                json!({ "error": format!("tool `{tool_name}` does not exist") }),
            ),
        };
        Ok(reply.into_result())
    }

    fn workspace_status_tool(&self) -> Result<ToolReply> {
        let preferences_path = paths::preferences_path();
        let data_dir = paths::data_dir();
        let models_dir = paths::models_dir();
        let workspace_members = vec![
            "apps/ozone-mcp",
            "apps/ozone-plus",
            "crates/ozone-core",
            "crates/ozone-engine",
            "crates/ozone-inference",
            "crates/ozone-mcp",
            "crates/ozone-memory",
            "crates/ozone-persist",
            "crates/ozone-tui",
        ];

        Ok(ToolReply::success(
            "Loaded workspace status".to_owned(),
            json!({
                "repoRoot": self.repo_root,
                "serverVersion": env!("CARGO_PKG_VERSION"),
                "workspaceMembers": workspace_members,
                "defaultPaths": {
                    "dataDir": data_dir,
                    "preferencesPath": preferences_path,
                    "modelsDir": models_dir,
                    "presetsPath": paths::presets_path(),
                    "launcherPath": paths::launcher_path()
                }
            }),
        ))
    }

    fn cargo_tool(&self, args: &Value) -> Result<ToolReply> {
        let action = required_string(args, "action")?;
        let package = optional_string(args, "package");
        let release = optional_bool(args, "release").unwrap_or(false);
        let quiet = optional_bool(args, "quiet").unwrap_or(false);
        let extra_args = optional_string_array(args, "extraArgs")?;

        let mut command = Command::new("cargo");
        command.current_dir(&self.repo_root);
        match action.as_str() {
            "check" | "test" | "build" | "clippy" => {
                command.arg(&action);
            }
            other => bail!("unsupported cargo action `{other}`"),
        }
        if let Some(package) = package.as_deref() {
            command.arg("-p").arg(package);
        }
        if quiet {
            command.arg("--quiet");
        }
        if release {
            command.arg("--release");
        }
        if action == "clippy" {
            command.arg("--");
            if extra_args.is_empty() {
                command.arg("-D").arg("warnings");
            } else {
                command.args(&extra_args);
            }
        } else {
            command.args(&extra_args);
        }

        let output = command
            .output()
            .with_context(|| format!("failed to run cargo {action}"))?;
        let data = command_output_data(&output);
        let summary = if output.status.success() {
            format!("cargo {action} succeeded")
        } else {
            format!("cargo {action} failed")
        };
        Ok(if output.status.success() {
            ToolReply::success(summary, data)
        } else {
            ToolReply::error(summary, data)
        })
    }

    fn catalog_list_tool(&self, args: &Value) -> Result<ToolReply> {
        let sandbox_id = optional_string(args, "sandboxId");
        let (models_dir, prefs_path) = self.with_sandbox_env(sandbox_id.as_deref(), || {
            Ok((paths::models_dir(), paths::preferences_path()))
        })?;

        let mut models = Vec::new();
        if models_dir.exists() {
            for entry in fs::read_dir(&models_dir)
                .with_context(|| format!("failed to read models dir {}", models_dir.display()))?
            {
                let entry = entry?;
                let path = entry.path();
                let file_name = entry.file_name().to_string_lossy().into_owned();
                let metadata = fs::symlink_metadata(&path)?;
                let is_gguf = file_name.ends_with(".gguf");
                let broken_symlink = metadata.file_type().is_symlink() && !path.exists();
                if is_gguf || broken_symlink {
                    models.push(json!({
                        "name": file_name,
                        "path": path,
                        "isSymlink": metadata.file_type().is_symlink(),
                        "isBrokenSymlink": broken_symlink,
                        "sizeBytes": if broken_symlink { None } else { fs::metadata(&path).ok().map(|value| value.len()) }
                    }));
                }
            }
        }
        models.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));

        Ok(ToolReply::success(
            "Listed model catalog files".to_owned(),
            json!({
                "modelsDir": models_dir,
                "preferencesPath": prefs_path,
                "models": models
            }),
        ))
    }

    fn preferences_get_tool(&self, args: &Value) -> Result<ToolReply> {
        let sandbox_id = optional_string(args, "sandboxId");
        let preferences_path =
            self.with_sandbox_env(sandbox_id.as_deref(), || Ok(paths::preferences_path()))?;
        let data = match preferences_path {
            Some(path) if path.exists() => {
                let text = fs::read_to_string(&path)
                    .with_context(|| format!("failed to read {}", path.display()))?;
                let parsed = serde_json::from_str::<Value>(&text).ok();
                json!({
                    "path": path,
                    "exists": true,
                    "raw": text,
                    "parsed": parsed
                })
            }
            Some(path) => json!({
                "path": path,
                "exists": false,
                "raw": null,
                "parsed": null
            }),
            None => json!({
                "path": null,
                "exists": false,
                "raw": null,
                "parsed": null
            }),
        };

        Ok(ToolReply::success(
            "Loaded preferences file".to_owned(),
            data,
        ))
    }

    fn sandbox_tool(&mut self, args: &Value) -> Result<ToolReply> {
        match required_string(args, "action")?.as_str() {
            "create" => self.create_sandbox(args),
            "destroy" => self.destroy_sandbox(args),
            other => Ok(ToolReply::error(
                "Sandbox action failed".to_owned(),
                json!({ "error": format!("unsupported sandbox action `{other}`") }),
            )),
        }
    }

    fn create_sandbox(&mut self, args: &Value) -> Result<ToolReply> {
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
            let text = serde_json::to_string_pretty(preferences)?;
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

    fn destroy_sandbox(&mut self, args: &Value) -> Result<ToolReply> {
        let sandbox_id = required_string(args, "sandboxId")?;
        let mut sandbox = self
            .sandboxes
            .remove(&sandbox_id)
            .ok_or_else(|| anyhow!("sandbox `{sandbox_id}` was not found"))?;
        sandbox.stop_backend()?;
        if sandbox.root.exists() {
            fs::remove_dir_all(&sandbox.root)
                .with_context(|| format!("failed to remove {}", sandbox.root.display()))?;
        }
        Ok(ToolReply::success(
            "Destroyed sandbox".to_owned(),
            json!({ "sandboxId": sandbox_id }),
        ))
    }

    fn mock_backend_tool(&mut self, args: &Value) -> Result<ToolReply> {
        match required_string(args, "action")?.as_str() {
            "start" => self.start_mock_backend(args),
            "stop" => self.stop_mock_backend(args),
            other => Ok(ToolReply::error(
                "Mock backend action failed".to_owned(),
                json!({ "error": format!("unsupported mock backend action `{other}`") }),
            )),
        }
    }

    fn start_mock_backend(&mut self, args: &Value) -> Result<ToolReply> {
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

    fn stop_mock_backend(&mut self, args: &Value) -> Result<ToolReply> {
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

    fn session_tool(&mut self, args: &Value) -> Result<ToolReply> {
        let action = required_string(args, "action")?;
        let sandbox_id = optional_string(args, "sandboxId");
        match action.as_str() {
            "create" => {
                let name = required_string(args, "name")?;
                let character_name = optional_string(args, "characterName");
                let tags = optional_string_array(args, "tags")?;
                self.with_repo(sandbox_id.as_deref(), |repo| {
                    let mut request = CreateSessionRequest::new(name);
                    request.character_name = character_name;
                    request.tags = tags;
                    let session = repo.create_session(request)?;
                    Ok(ToolReply::success(
                        "Created ozone+ session".to_owned(),
                        json!({ "session": session_summary_json(&session) }),
                    ))
                })
            }
            "list" => self.with_repo(sandbox_id.as_deref(), |repo| {
                let sessions = repo.list_sessions()?;
                Ok(ToolReply::success(
                    "Listed ozone+ sessions".to_owned(),
                    json!({
                        "sessions": sessions.iter().map(session_summary_json).collect::<Vec<_>>()
                    }),
                ))
            }),
            "metadata" => {
                let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
                self.with_repo(sandbox_id.as_deref(), |repo| {
                    let session = repo
                        .get_session(&session_id)?
                        .ok_or_else(|| anyhow!("session {session_id} was not found"))?;
                    let active_branch = repo.get_active_branch(&session_id)?;
                    let transcript_message_count = match active_branch.as_ref() {
                        Some(record) => repo
                            .list_branch_messages(&session_id, &record.branch.branch_id)?
                            .len(),
                        None => 0,
                    };
                    let lock_probe = probe_session_lock(&repo, &session_id)?;
                    Ok(ToolReply::success(
                        "Loaded session metadata".to_owned(),
                        json!({
                            "session": session_summary_json(&session),
                            "activeBranch": active_branch.as_ref().map(branch_record_json),
                            "transcriptMessageCount": transcript_message_count,
                            "lock": lock_probe
                        }),
                    ))
                })
            }
            "transcript" => {
                let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
                let branch_id = optional_string(args, "branchId")
                    .map(|value| parse_branch_id(&value))
                    .transpose()?;
                self.with_repo(sandbox_id.as_deref(), |repo| {
                    let branch = match branch_id.as_ref() {
                        Some(branch_id) => repo
                            .get_branch(&session_id, branch_id)?
                            .ok_or_else(|| anyhow!("branch {branch_id} was not found"))?,
                        None => repo
                            .get_active_branch(&session_id)?
                            .ok_or_else(|| anyhow!("session {session_id} has no active branch"))?,
                    };
                    let messages =
                        repo.list_branch_messages(&session_id, &branch.branch.branch_id)?;
                    Ok(ToolReply::success(
                        "Loaded transcript".to_owned(),
                        json!({
                            "branch": branch_record_json(&branch),
                            "messages": messages.iter().map(message_json).collect::<Vec<_>>()
                        }),
                    ))
                })
            }
            other => Ok(ToolReply::error(
                "Session action failed".to_owned(),
                json!({ "error": format!("unsupported session action `{other}`") }),
            )),
        }
    }

    fn message_tool(&mut self, args: &Value) -> Result<ToolReply> {
        let action = required_string(args, "action")?;
        if action != "send" {
            return Ok(ToolReply::error(
                "Message action failed".to_owned(),
                json!({ "error": format!("unsupported message action `{action}`") }),
            ));
        }

        let session_id = required_string(args, "sessionId")?;
        let content = required_string(args, "content")?;
        let sandbox_id = optional_string(args, "sandboxId");
        let author_kind = optional_string(args, "author").unwrap_or_else(|| "user".to_owned());
        let author_name = optional_string(args, "authorName");
        let mut command = vec![
            "run".to_owned(),
            "-p".to_owned(),
            OZONE_PLUS_PACKAGE.to_owned(),
            "--quiet".to_owned(),
            "--".to_owned(),
            "send".to_owned(),
            session_id.clone(),
            content.clone(),
        ];
        if author_kind != "user" {
            command.push("--author".to_owned());
            command.push(author_kind);
        }
        if let Some(author_name) = author_name {
            command.push("--author-name".to_owned());
            command.push(author_name);
        }
        let output = self.run_workspace_command("cargo", &command, sandbox_id.as_deref())?;
        let message_ids = output
            .stdout
            .lines()
            .filter_map(|line| line.strip_prefix("  message id      "))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let data = json!({
            "command": output.command,
            "ok": output.success,
            "messageIds": message_ids,
            "stdout": output.stdout,
            "stderr": output.stderr,
            "exitCode": output.exit_code
        });
        Ok(if output.success {
            ToolReply::success("Completed runtime-backed send".to_owned(), data)
        } else {
            ToolReply::error("Runtime-backed send failed".to_owned(), data)
        })
    }

    fn memory_tool(&mut self, args: &Value) -> Result<ToolReply> {
        let action = required_string(args, "action")?;
        let sandbox_id = optional_string(args, "sandboxId");
        match action.as_str() {
            "note" => {
                let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
                let content = required_string(args, "content")?;
                self.with_repo(sandbox_id.as_deref(), |repo| {
                    let record = repo.create_note_memory(
                        &session_id,
                        CreateNoteMemoryRequest::new(
                            content,
                            AuthorId::User,
                            Provenance::UserAuthored,
                        ),
                    )?;
                    Ok(ToolReply::success(
                        "Created note memory".to_owned(),
                        json!({ "record": pinned_memory_record_json(&record) }),
                    ))
                })
            }
            "pin" => {
                let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
                let message_id = parse_message_id(&required_string(args, "messageId")?)?;
                let expires_after_turns =
                    optional_u64(args, "expiresAfterTurns").map(|value| value as u32);
                self.with_repo(sandbox_id.as_deref(), |repo| {
                    let record = repo.pin_message_memory(
                        &session_id,
                        &message_id,
                        PinMessageMemoryRequest {
                            pinned_by: AuthorId::User,
                            expires_after_turns,
                            provenance: Provenance::UserAuthored,
                        },
                    )?;
                    Ok(ToolReply::success(
                        "Pinned memory".to_owned(),
                        json!({ "record": pinned_memory_record_json(&record) }),
                    ))
                })
            }
            "list" => {
                let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
                self.with_repo(sandbox_id.as_deref(), |repo| {
                    let memories = repo.list_pinned_memories(&session_id)?;
                    Ok(ToolReply::success(
                        "Listed pinned memories".to_owned(),
                        json!({
                            "memories": memories.iter().map(pinned_memory_view_json).collect::<Vec<_>>()
                        }),
                    ))
                })
            }
            other => Ok(ToolReply::error(
                "Memory action failed".to_owned(),
                json!({ "error": format!("unsupported memory action `{other}`") }),
            )),
        }
    }

    fn search_tool(&mut self, args: &Value) -> Result<ToolReply> {
        let action = required_string(args, "action")?;
        let sandbox_id = optional_string(args, "sandboxId");
        let output = match action.as_str() {
            "session" => {
                let session_id = required_string(args, "sessionId")?;
                let query = required_string(args, "query")?;
                self.run_workspace_command(
                    "cargo",
                    &[
                        "run".to_owned(),
                        "-p".to_owned(),
                        OZONE_PLUS_PACKAGE.to_owned(),
                        "--quiet".to_owned(),
                        "--".to_owned(),
                        "search".to_owned(),
                        "session".to_owned(),
                        session_id,
                        query,
                    ],
                    sandbox_id.as_deref(),
                )?
            }
            "global" => {
                let query = required_string(args, "query")?;
                self.run_workspace_command(
                    "cargo",
                    &[
                        "run".to_owned(),
                        "-p".to_owned(),
                        OZONE_PLUS_PACKAGE.to_owned(),
                        "--quiet".to_owned(),
                        "--".to_owned(),
                        "search".to_owned(),
                        "global".to_owned(),
                        query,
                    ],
                    sandbox_id.as_deref(),
                )?
            }
            "index_rebuild" => self.run_workspace_command(
                "cargo",
                &[
                    "run".to_owned(),
                    "-p".to_owned(),
                    OZONE_PLUS_PACKAGE.to_owned(),
                    "--quiet".to_owned(),
                    "--".to_owned(),
                    "index".to_owned(),
                    "rebuild".to_owned(),
                ],
                sandbox_id.as_deref(),
            )?,
            other => {
                return Ok(ToolReply::error(
                    "Search action failed".to_owned(),
                    json!({ "error": format!("unsupported search action `{other}`") }),
                ));
            }
        };

        let mode = parse_prefixed_field(&output.stdout, "  mode            ");
        let hits = parse_prefixed_field(&output.stdout, "  hits            ")
            .and_then(|value| value.parse::<u64>().ok());
        let data = json!({
            "command": output.command,
            "ok": output.success,
            "mode": mode,
            "hits": hits,
            "stdout": output.stdout,
            "stderr": output.stderr,
            "exitCode": output.exit_code
        });
        Ok(if output.success {
            ToolReply::success("Completed search/index command".to_owned(), data)
        } else {
            ToolReply::error("Search/index command failed".to_owned(), data)
        })
    }

    fn branch_tool(&mut self, args: &Value) -> Result<ToolReply> {
        let action = required_string(args, "action")?;
        let sandbox_id = optional_string(args, "sandboxId");
        match action.as_str() {
            "create" => {
                let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
                let name = required_string(args, "name")?;
                let activate = optional_bool(args, "activate").unwrap_or(false);
                let from_message_id = optional_string(args, "fromMessageId")
                    .map(|value| parse_message_id(&value))
                    .transpose()?;
                self.with_repo(sandbox_id.as_deref(), |repo| {
                    let tip_message_id = match from_message_id {
                        Some(value) => value,
                        None => {
                            repo.get_active_branch(&session_id)?
                                .ok_or_else(|| {
                                    anyhow!("session {session_id} has no active branch")
                                })?
                                .branch
                                .tip_message_id
                        }
                    };
                    let mut branch = ConversationBranch::new(
                        parse_branch_id(&Uuid::new_v4().to_string())?,
                        session_id.clone(),
                        name,
                        tip_message_id.clone(),
                        now_timestamp_ms(),
                    );
                    branch.state = if activate {
                        BranchState::Active
                    } else {
                        BranchState::Inactive
                    };
                    let record = repo.create_branch(CreateBranchCommand {
                        branch,
                        forked_from: tip_message_id,
                    })?;
                    Ok(ToolReply::success(
                        "Created branch".to_owned(),
                        json!({ "branch": branch_record_json(&record) }),
                    ))
                })
            }
            "list" => {
                let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
                self.with_repo(sandbox_id.as_deref(), |repo| {
                    let branches = repo.list_branches(&session_id)?;
                    Ok(ToolReply::success(
                        "Listed branches".to_owned(),
                        json!({
                            "branches": branches.iter().map(branch_record_json).collect::<Vec<_>>()
                        }),
                    ))
                })
            }
            "activate" => {
                let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
                let branch_id = parse_branch_id(&required_string(args, "branchId")?)?;
                self.with_repo(sandbox_id.as_deref(), |repo| {
                    let branch = repo.activate_branch(&session_id, &branch_id)?;
                    Ok(ToolReply::success(
                        "Activated branch".to_owned(),
                        json!({ "branch": branch_record_json(&branch) }),
                    ))
                })
            }
            other => Ok(ToolReply::error(
                "Branch action failed".to_owned(),
                json!({ "error": format!("unsupported branch action `{other}`") }),
            )),
        }
    }

    fn swipe_tool(&mut self, args: &Value) -> Result<ToolReply> {
        let action = required_string(args, "action")?;
        let sandbox_id = optional_string(args, "sandboxId");
        match action.as_str() {
            "add" => {
                let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
                let parent_message_id =
                    parse_message_id(&required_string(args, "parentMessageId")?)?;
                let content = required_string(args, "content")?;
                let parent_context_message_id = optional_string(args, "contextMessageId")
                    .map(|value| parse_message_id(&value))
                    .transpose()?;
                let swipe_group_id = optional_string(args, "swipeGroupId")
                    .map(|value| parse_swipe_group_id(&value))
                    .transpose()?;
                let ordinal = optional_u64(args, "ordinal").map(|value| value as u16);
                let author_kind =
                    optional_string(args, "author").unwrap_or_else(|| "assistant".to_owned());
                let author_name = optional_string(args, "authorName");
                let state = optional_string(args, "state")
                    .map(|value| value.parse::<SwipeCandidateState>())
                    .transpose()?
                    .unwrap_or_default();
                self.with_repo(sandbox_id.as_deref(), |repo| {
                    let message_record = repo.insert_message(
                        &session_id,
                        CreateMessageRequest {
                            parent_id: Some(parent_message_id.to_string()),
                            author_kind,
                            author_name,
                            content,
                        },
                    )?;
                    let message_id = parse_message_id(&message_record.message_id)?;
                    let existing_group = match swipe_group_id.as_ref() {
                        Some(group_id) => repo.get_swipe_group(&session_id, group_id)?,
                        None => repo
                            .list_swipe_groups(&session_id)?
                            .into_iter()
                            .find(|group| group.parent_message_id == parent_message_id),
                    };
                    let mut group = existing_group.unwrap_or_else(|| {
                        let mut group = SwipeGroup::new(
                            parse_swipe_group_id(&Uuid::new_v4().to_string())
                                .expect("generated uuid should parse"),
                            parent_message_id.clone(),
                        );
                        group.parent_context_message_id = parent_context_message_id.clone();
                        group
                    });
                    if group.parent_context_message_id.is_none() {
                        group.parent_context_message_id = parent_context_message_id;
                    }
                    let next_ordinal = match ordinal {
                        Some(value) => value,
                        None => {
                            match repo.list_swipe_candidates(&session_id, &group.swipe_group_id) {
                                Ok(candidates) => candidates
                                    .iter()
                                    .map(|candidate| candidate.ordinal)
                                    .max()
                                    .unwrap_or(0)
                                    .saturating_add(1),
                                Err(PersistError::SwipeGroupNotFound(_)) => 0,
                                Err(error) => return Err(anyhow!(error.to_string())),
                            }
                        }
                    };
                    let candidate = repo.record_swipe_candidate(
                        &session_id,
                        ozone_persist::RecordSwipeCandidateCommand {
                            group: group.clone(),
                            candidate: SwipeCandidate {
                                swipe_group_id: group.swipe_group_id.clone(),
                                ordinal: next_ordinal,
                                message_id,
                                state,
                                partial_content: None,
                                tokens_generated: None,
                            },
                        },
                    )?;
                    Ok(ToolReply::success(
                        "Added swipe candidate".to_owned(),
                        json!({
                            "group": swipe_group_json(&group),
                            "candidate": swipe_candidate_json(&candidate)
                        }),
                    ))
                })
            }
            "list" => {
                let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
                self.with_repo(sandbox_id.as_deref(), |repo| {
                    let groups = repo.list_swipe_groups(&session_id)?;
                    let mut results = Vec::new();
                    for group in groups {
                        let candidates = repo.list_swipe_candidates(&session_id, &group.swipe_group_id)?;
                        results.push(json!({
                            "group": swipe_group_json(&group),
                            "candidates": candidates.iter().map(swipe_candidate_json).collect::<Vec<_>>()
                        }));
                    }
                    Ok(ToolReply::success(
                        "Listed swipe groups".to_owned(),
                        json!({ "swipes": results }),
                    ))
                })
            }
            "activate" => {
                let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
                let swipe_group_id = parse_swipe_group_id(&required_string(args, "swipeGroupId")?)?;
                let ordinal = required_u64(args, "ordinal")? as u16;
                self.with_repo(sandbox_id.as_deref(), |repo| {
                    let group = repo.activate_swipe_candidate(
                        &session_id,
                        ActivateSwipeCommand {
                            swipe_group_id: swipe_group_id.clone(),
                            ordinal,
                        },
                    )?;
                    let selected_candidate = repo
                        .list_swipe_candidates(&session_id, &group.swipe_group_id)?
                        .into_iter()
                        .find(|candidate| candidate.ordinal == group.active_ordinal)
                        .ok_or_else(|| {
                            anyhow!(
                                "swipe group {} is missing active ordinal {}",
                                group.swipe_group_id,
                                group.active_ordinal
                            )
                        })?;
                    if let Some(active_branch) = repo.get_active_branch(&session_id)? {
                        let candidate_message_ids = repo
                            .list_swipe_candidates(&session_id, &group.swipe_group_id)?
                            .into_iter()
                            .map(|candidate| candidate.message_id)
                            .collect::<Vec<_>>();
                        if active_branch.branch.tip_message_id == group.parent_message_id
                            || candidate_message_ids.contains(&active_branch.branch.tip_message_id)
                        {
                            let _ = repo.set_branch_tip(
                                &session_id,
                                &active_branch.branch.branch_id,
                                &selected_candidate.message_id,
                            )?;
                        }
                    }
                    Ok(ToolReply::success(
                        "Activated swipe candidate".to_owned(),
                        json!({
                            "group": swipe_group_json(&group),
                            "selectedCandidate": swipe_candidate_json(&selected_candidate)
                        }),
                    ))
                })
            }
            other => Ok(ToolReply::error(
                "Swipe action failed".to_owned(),
                json!({ "error": format!("unsupported swipe action `{other}`") }),
            )),
        }
    }

    fn export_tool(&mut self, args: &Value) -> Result<ToolReply> {
        let action = required_string(args, "action")?;
        let sandbox_id = optional_string(args, "sandboxId");
        let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
        match action.as_str() {
            "session" => self.with_repo(sandbox_id.as_deref(), |repo| {
                let export = repo.export_session(&session_id)?;
                if let Some(output_path) = optional_string(args, "outputPath") {
                    let text = serde_json::to_string_pretty(&export)?;
                    fs::write(&output_path, format!("{text}\n"))?;
                }
                Ok(ToolReply::success(
                    "Exported session".to_owned(),
                    json!({ "export": export }),
                ))
            }),
            "transcript" => {
                let branch_id = optional_string(args, "branchId")
                    .map(|value| parse_branch_id(&value))
                    .transpose()?;
                let format = optional_string(args, "format").unwrap_or_else(|| "json".to_owned());
                self.with_repo(sandbox_id.as_deref(), |repo| {
                    let export = repo.export_transcript(&session_id, branch_id.as_ref())?;
                    if let Some(output_path) = optional_string(args, "outputPath") {
                        match format.as_str() {
                            "json" => {
                                let text = serde_json::to_string_pretty(&export)?;
                                fs::write(&output_path, format!("{text}\n"))?;
                            }
                            "text" => {
                                fs::write(&output_path, render_transcript_text(&export))?;
                            }
                            other => bail!("unsupported transcript export format `{other}`"),
                        }
                    }
                    Ok(ToolReply::success(
                        "Exported transcript".to_owned(),
                        json!({ "export": export, "format": format }),
                    ))
                })
            }
            other => Ok(ToolReply::error(
                "Export action failed".to_owned(),
                json!({ "error": format!("unsupported export action `{other}`") }),
            )),
        }
    }

    fn import_card_tool(&mut self, args: &Value) -> Result<ToolReply> {
        let sandbox_id = optional_string(args, "sandboxId");
        let session_name = optional_string(args, "sessionName");
        let tags = optional_string_array(args, "tags")?;
        let path = optional_string(args, "path");
        let card_json = optional_string(args, "cardJson");
        let provenance = optional_string(args, "provenance");
        let card = match (path.as_deref(), card_json.as_deref()) {
            (Some(path), _) => {
                let text = fs::read_to_string(path)
                    .with_context(|| format!("failed to read character card {}", path))?;
                CharacterCard::from_json_str(&text).map_err(|error| anyhow!(error.to_string()))?
            }
            (None, Some(card_json)) => CharacterCard::from_json_str(card_json)
                .map_err(|error| anyhow!(error.to_string()))?,
            (None, None) => bail!("import_card requires either `path` or `cardJson`"),
        };
        self.with_repo(sandbox_id.as_deref(), |repo| {
            let imported = repo.import_character_card(ImportCharacterCardRequest {
                card,
                session_name,
                tags,
                provenance: provenance
                    .unwrap_or_else(|| path.clone().unwrap_or_else(|| "ozone-mcp".to_owned())),
            })?;
            Ok(ToolReply::success(
                "Imported character card".to_owned(),
                json!({
                    "session": session_summary_json(&imported.session),
                    "seededBranchId": imported.seeded_branch_id.map(|value| value.to_string()),
                    "seededMessageId": imported.seeded_message_id.map(|value| value.to_string())
                }),
            ))
        })
    }

    fn launcher_smoke_tool(&mut self, args: &Value) -> Result<ToolReply> {
        let sandbox_id = required_string(args, "sandboxId")?;
        let live_refresh_model_name = optional_string(args, "liveRefreshModelName");
        let enter_count = optional_u64(args, "enterCount").unwrap_or(4);
        let sandbox = self
            .sandboxes
            .get(&sandbox_id)
            .ok_or_else(|| anyhow!("sandbox `{sandbox_id}` was not found"))?;
        let refresh_model_path = live_refresh_model_name
            .as_ref()
            .map(|name| sandbox.models_dir.join(name));
        let runner_spec = LauncherSmokeRunnerSpec {
            repo_root: self.repo_root.to_string_lossy().into_owned(),
            live_refresh_path: refresh_model_path.map(|path| path.to_string_lossy().into_owned()),
            enter_count,
            capture: PtyVteCaptureConfig::sandbox_artifacts(sandbox, "launcher-smoke-final"),
        };
        let script_body = r###"def run():
    master, proc = open_pty_process(
        ["cargo", "run", "--quiet", "--", "--mode", "base", "--frontend", "ozonePlus", "--no-browser"],
        SPEC["repoRoot"],
    )
    live_refresh_path = SPEC.get("liveRefreshPath")
    live_refresh_name = os.path.basename(live_refresh_path) if live_refresh_path else None
    saw_live_refresh_model = False

    pump(master, proc, 5.5)
    if live_refresh_path:
        open(live_refresh_path, "ab").close()
        pump(master, proc, 2.5)
        saw_live_refresh_model = screen_contains(live_refresh_name)

    for index in range(int(SPEC["enterCount"])):
        send_key(master, "enter")
        pump(master, proc, 3.0 if index + 1 == int(SPEC["enterCount"]) else 1.0)
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
        let output = self.run_python_vte_helper(
            sandbox,
            &runner_spec,
            script_body,
            "failed to run launcher smoke helper",
        )?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let pty_data = serde_json::from_str::<Value>(&stdout).unwrap_or_else(
            |_| json!({ "bufferTail": stdout.trim(), "sawLiveRefreshModel": false }),
        );
        let launcher_invocation_log = sandbox.root.join("launcher-invocation.txt");
        let sessions = self.with_repo(Some(&sandbox_id), |repo| {
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
            "launcherSession": launcher_session.cloned()
        });
        Ok(if output.status.success() {
            ToolReply::success("Completed launcher handoff smoke".to_owned(), data)
        } else {
            ToolReply::error("Launcher handoff smoke failed".to_owned(), data)
        })
    }

    fn mock_user_tool(&mut self, args: &Value) -> Result<ToolReply> {
        let sandbox_id = required_string(args, "sandboxId")?;
        let journey = match (
            optional_string(args, "journey"),
            optional_string(args, "target"),
        ) {
            (Some(_), Some(_)) => bail!("provide either `journey` or `target`, not both"),
            (Some(journey_name), None) => self.build_mock_user_journey(&journey_name, args)?,
            (None, Some(target_name)) => self.build_mock_user_target_journey(&target_name)?,
            (None, None) => bail!("mock_user_tool requires either `journey` or `target`"),
        };
        let run_name = journey.name.clone();
        let data = self.run_mock_user_journey(&sandbox_id, &journey, None, args, None)?;
        let success = data
            .get("success")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Ok(if success {
            ToolReply::success(format!("Completed mock-user journey `{run_name}`"), data)
        } else {
            ToolReply::error(format!("Mock-user journey `{run_name}` failed"), data)
        })
    }

    fn screen_nav_targets_tool(&self, args: &Value) -> Result<ToolReply> {
        let targets = if let Some(target_name) = optional_string(args, "target") {
            let target = capturable_screen_journey_builders()
                .iter()
                .find(|entry| entry.target_screen == target_name)
                .ok_or_else(|| anyhow!("unknown screen navigation target `{target_name}`"))?;
            vec![self.screen_nav_target_data(target)?]
        } else {
            capturable_screen_journey_builders()
                .iter()
                .map(|target| self.screen_nav_target_data(target))
                .collect::<Result<Vec<_>>>()?
        };

        Ok(ToolReply::success(
            "Loaded screen navigation targets".to_owned(),
            json!({ "targets": targets }),
        ))
    }

    fn screenshot_tool(&mut self, args: &Value) -> Result<ToolReply> {
        let sandbox_id = required_string(args, "sandboxId")?;
        let target = required_string(args, "target")?;
        let output_dir = PathBuf::from(required_string(args, "outputDir")?);
        let journey = self.build_mock_user_target_journey(&target)?;
        fs::create_dir_all(&output_dir)
            .with_context(|| format!("failed to create output dir {}", output_dir.display()))?;

        let capture = screenshot_capture_config(args, &output_dir, &target)?;
        let mut data = self.run_mock_user_journey(
            &sandbox_id,
            &journey,
            Some(target.clone()),
            args,
            Some(capture),
        )?;
        if let Value::Object(map) = &mut data {
            map.insert(
                "outputDir".to_owned(),
                Value::String(output_dir.display().to_string()),
            );
        }
        let success = data
            .get("success")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let missing_dependencies = data
            .get("missingModules")
            .and_then(Value::as_array)
            .is_some_and(|value| !value.is_empty());
        let summary = if missing_dependencies {
            format!("Screenshot capture for `{target}` failed: missing Python dependencies")
        } else if success {
            format!("Captured screenshot for `{target}`")
        } else {
            format!("Screenshot capture for `{target}` failed")
        };
        Ok(if success {
            ToolReply::success(summary, data)
        } else {
            ToolReply::error(summary, data)
        })
    }

    fn screen_check_tool(&self, args: &Value) -> Result<ToolReply> {
        let artifact_path = optional_string(args, "artifactPath")
            .or_else(|| optional_string(args, "path"))
            .or_else(|| optional_string(args, "sidecarPath"))
            .ok_or_else(|| {
                anyhow!("screen_check_tool requires `artifactPath` (or `path` / `sidecarPath`)")
            })?;
        let checks = args
            .get("checks")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("field `checks` must be an array of check objects"))?;
        if checks.is_empty() {
            bail!("field `checks` must contain at least one check");
        }

        let (sidecar_path, capture) = load_screen_capture_sidecar(&artifact_path)?;
        let results = checks
            .iter()
            .enumerate()
            .map(|(index, check)| evaluate_screen_check(index, check, &capture))
            .collect::<Result<Vec<_>>>()?;
        let passed = results.iter().filter(|result| result.passed).count();
        let failed = results.len().saturating_sub(passed);
        let success = failed == 0;
        let summary = if success {
            format!("Screen check passed ({passed}/{passed} checks)")
        } else {
            format!(
                "Screen check failed ({passed}/{} checks passed)",
                results.len()
            )
        };

        let data = json!({
            "artifactPath": artifact_path,
            "sidecarPath": sidecar_path.display().to_string(),
            "pngPath": capture.png_path,
            "screen": {
                "rows": capture.screen_rows,
                "columns": capture.screen_columns,
                "cursor": capture.cursor,
                "font": capture.font
            },
            "summary": {
                "total": results.len(),
                "passed": passed,
                "failed": failed,
                "success": success
            },
            "checks": results
        });

        Ok(if success {
            ToolReply::success(summary, data)
        } else {
            ToolReply::error(summary, data)
        })
    }

    fn build_mock_user_journey(
        &self,
        journey_name: &str,
        args: &Value,
    ) -> Result<MockUserJourneySpec> {
        match journey_name {
            "launcher_monitor_roundtrip" => {
                let mut journey =
                    self.build_capturable_screen_journey("base_monitor", args, journey_name)?;
                journey.steps.push(MockUserJourneyStep::text(
                    "return to launcher",
                    "r",
                    1200,
                    ["Launch", "Open ozone+", "Settings"],
                ));
                Ok(journey)
            }
            "launcher_to_ozone_plus" => {
                self.build_capturable_screen_journey("base_ozone_plus_shell", args, journey_name)
            }
            "ozone_plus_chat_journey" => {
                let prompt = optional_string(args, "prompt")
                    .unwrap_or_else(|| "Check the observatory key".to_owned());
                let mut journey = self.build_capturable_screen_journey(
                    "base_ozone_plus_shell",
                    args,
                    journey_name,
                )?;
                if let Some(step) = journey.steps.last_mut() {
                    step.settle_ms = 2500;
                }
                journey.steps.extend([
                    MockUserJourneyStep::text("enter insert mode", "i", 400, ["INSERT"]),
                    MockUserJourneyStep::text("type prompt", &prompt, 400, []),
                    MockUserJourneyStep::key(
                        "send prompt",
                        "enter",
                        3500,
                        ["koboldcpp backend", "observatory", "logged", "mock backend"],
                    ),
                ]);
                Ok(journey)
            }
            other => bail!("unsupported mock-user journey `{other}`"),
        }
    }

    fn build_mock_user_target_journey(&self, target_name: &str) -> Result<MockUserJourneySpec> {
        self.build_capturable_screen_journey(target_name, &json!({}), target_name)
    }

    fn build_capturable_screen_journey(
        &self,
        target_screen: &str,
        args: &Value,
        journey_name: &str,
    ) -> Result<MockUserJourneySpec> {
        let builder = self.capturable_screen_definition(target_screen)?.builder;
        builder(self, journey_name, args)
    }

    fn capturable_screen_definition(
        &self,
        target_screen: &str,
    ) -> Result<&'static CapturableScreenJourneyDefinition> {
        capturable_screen_journey_builders()
            .iter()
            .find(|entry| entry.target_screen == target_screen)
            .ok_or_else(|| {
                anyhow!(
                    "unknown screen navigation target `{target_screen}`; use `screen_nav_targets` to list valid targets"
                )
            })
    }

    fn screen_nav_target_data(
        &self,
        definition: &CapturableScreenJourneyDefinition,
    ) -> Result<Value> {
        let journey = self.build_capturable_screen_journey(
            definition.target_screen,
            &json!({}),
            definition.target_screen,
        )?;
        Ok(json!({
            "name": definition.target_screen,
            "description": definition.description,
            "command": journey.command,
            "toolArguments": {
                "target": definition.target_screen
            },
            "sandboxSetup": (definition.sandbox_setup)(),
        }))
    }

    fn build_base_splash_screen_journey(
        &self,
        journey_name: &str,
        _args: &Value,
    ) -> Result<MockUserJourneySpec> {
        Ok(MockUserJourneySpec {
            name: journey_name.to_owned(),
            cwd: self.repo_root.to_string_lossy().into_owned(),
            command: append_args(
                &self.front_door_binary_command("ozone", &["--mode", "base"]),
                &["--no-browser"],
            ),
            steps: vec![MockUserJourneyStep::wait_for(
                "render splash",
                5500,
                ["Continue", "local-first AI tooling"],
            )],
        })
    }

    fn build_base_tier_picker_screen_journey(
        &self,
        journey_name: &str,
        _args: &Value,
    ) -> Result<MockUserJourneySpec> {
        Ok(MockUserJourneySpec {
            name: journey_name.to_owned(),
            cwd: self.repo_root.to_string_lossy().into_owned(),
            command: self.front_door_binary_command("ozone", &["--pick", "--no-browser"]),
            steps: vec![
                MockUserJourneyStep::wait("splash settle", 5500),
                MockUserJourneyStep::key(
                    "open tier picker",
                    "enter",
                    1000,
                    ["Choose Your Tier", "ozone+", "ozonelite"],
                ),
            ],
        })
    }

    fn build_base_launcher_screen_journey(
        &self,
        journey_name: &str,
        _args: &Value,
    ) -> Result<MockUserJourneySpec> {
        Ok(MockUserJourneySpec {
            name: journey_name.to_owned(),
            cwd: self.repo_root.to_string_lossy().into_owned(),
            command: append_args(
                &self.front_door_binary_command("ozone", &["--mode", "base"]),
                &["--no-browser"],
            ),
            steps: vec![
                MockUserJourneyStep::wait("splash settle", 5500),
                MockUserJourneyStep::key(
                    "reach launcher",
                    "enter",
                    1000,
                    ["Launch", "Open ozone+", "Settings"],
                ),
            ],
        })
    }

    fn build_base_exit_confirm_screen_journey(
        &self,
        journey_name: &str,
        _args: &Value,
    ) -> Result<MockUserJourneySpec> {
        let mut journey = self.build_base_launcher_screen_journey(journey_name, &json!({}))?;
        journey.steps.push(MockUserJourneyStep::key(
            "open exit confirm",
            "esc",
            600,
            ["Confirm Exit", "Leave Ozone?", "Stay"],
        ));
        Ok(journey)
    }

    fn build_base_settings_screen_journey(
        &self,
        journey_name: &str,
        _args: &Value,
    ) -> Result<MockUserJourneySpec> {
        let mut journey = self.build_base_launcher_screen_journey(journey_name, &json!({}))?;
        journey.steps.extend([
            MockUserJourneyStep::key("move to profile", "down", 150, []),
            MockUserJourneyStep::key("move to ozone plus", "down", 150, []),
            MockUserJourneyStep::key("move to settings", "down", 150, []),
            MockUserJourneyStep::key(
                "open settings",
                "enter",
                800,
                ["Settings", "Backend", "Frontend"],
            ),
        ]);
        Ok(journey)
    }

    fn build_base_model_picker_launch_screen_journey(
        &self,
        journey_name: &str,
        _args: &Value,
    ) -> Result<MockUserJourneySpec> {
        let mut journey = self.build_base_launcher_screen_journey(journey_name, &json!({}))?;
        journey.steps.push(MockUserJourneyStep::key(
            "open launch model picker",
            "enter",
            1200,
            ["Model Picker · Launch", "mock-model.gguf", "type to filter"],
        ));
        Ok(journey)
    }

    fn build_base_confirm_launch_screen_journey(
        &self,
        journey_name: &str,
        _args: &Value,
    ) -> Result<MockUserJourneySpec> {
        let mut journey =
            self.build_base_model_picker_launch_screen_journey(journey_name, &json!({}))?;
        journey.steps.push(MockUserJourneyStep::key(
            "build launch plan",
            "enter",
            1200,
            ["Confirm Launch", "Context:", "QuantKV:"],
        ));
        Ok(journey)
    }

    fn build_base_frontend_choice_screen_journey(
        &self,
        journey_name: &str,
        _args: &Value,
    ) -> Result<MockUserJourneySpec> {
        let mut journey =
            self.build_base_confirm_launch_screen_journey(journey_name, &json!({}))?;
        journey.steps.push(MockUserJourneyStep::key(
            "open frontend choice",
            "enter",
            800,
            ["Choose Frontend", "SillyTavern", "ozone+"],
        ));
        Ok(journey)
    }

    fn build_base_launching_screen_journey(
        &self,
        journey_name: &str,
        _args: &Value,
    ) -> Result<MockUserJourneySpec> {
        let mut journey =
            self.build_base_frontend_choice_screen_journey(journey_name, &json!({}))?;
        journey.steps.push(MockUserJourneyStep::key(
            "start launch",
            "enter",
            600,
            [
                "Launching KoboldCpp",
                "Preparing ozone+ handoff",
                "Please wait",
            ],
        ));
        Ok(journey)
    }

    fn build_base_monitor_screen_journey(
        &self,
        journey_name: &str,
        _args: &Value,
    ) -> Result<MockUserJourneySpec> {
        let mut journey = self.build_base_launcher_screen_journey(journey_name, &json!({}))?;
        journey.command = append_args(
            &self.front_door_binary_command("ozone", &["--mode", "base"]),
            &["--frontend", "sillyTavern", "--no-browser"],
        );
        journey.steps.extend([
            MockUserJourneyStep::key(
                "pick launch model",
                "enter",
                1500,
                ["Confirm Launch", "Context:", "QuantKV:"],
            ),
            MockUserJourneyStep::key(
                "launch into monitor",
                "enter",
                3000,
                ["Ozone Monitor", "Services", "SillyTavern"],
            ),
        ]);
        Ok(journey)
    }

    fn build_base_model_picker_profile_screen_journey(
        &self,
        journey_name: &str,
        _args: &Value,
    ) -> Result<MockUserJourneySpec> {
        let mut journey = self.build_base_launcher_screen_journey(journey_name, &json!({}))?;
        journey.steps.extend([
            MockUserJourneyStep::key("move to profile", "down", 150, []),
            MockUserJourneyStep::key(
                "open profile model picker",
                "enter",
                1200,
                [
                    "Model Picker · Profile",
                    "mock-model.gguf",
                    "type to filter",
                ],
            ),
        ]);
        Ok(journey)
    }

    fn build_base_profile_advisory_screen_journey(
        &self,
        journey_name: &str,
        _args: &Value,
    ) -> Result<MockUserJourneySpec> {
        let mut journey =
            self.build_base_model_picker_profile_screen_journey(journey_name, &json!({}))?;
        journey.steps.push(MockUserJourneyStep::key(
            "build profiling advisory",
            "enter",
            1200,
            ["Profiling Advisor", "Next Actions", "Recommendation:"],
        ));
        Ok(journey)
    }

    fn build_base_profile_confirm_screen_journey(
        &self,
        journey_name: &str,
        _args: &Value,
    ) -> Result<MockUserJourneySpec> {
        let mut journey =
            self.build_base_profile_advisory_screen_journey(journey_name, &json!({}))?;
        journey.steps.push(MockUserJourneyStep::key(
            "open profiling confirm",
            "enter",
            800,
            ["Confirm Profiling Step", "Press Enter to start", "Action:"],
        ));
        Ok(journey)
    }

    fn build_base_profile_running_screen_journey(
        &self,
        journey_name: &str,
        _args: &Value,
    ) -> Result<MockUserJourneySpec> {
        let mut journey =
            self.build_base_profile_confirm_screen_journey(journey_name, &json!({}))?;
        journey.steps.push(MockUserJourneyStep::key(
            "start profiling",
            "enter",
            800,
            ["Profiling In Progress", "Stage:", "Preparing"],
        ));
        Ok(journey)
    }

    fn build_base_profile_failure_screen_journey(
        &self,
        journey_name: &str,
        _args: &Value,
    ) -> Result<MockUserJourneySpec> {
        let mut journey =
            self.build_base_profile_advisory_screen_journey(journey_name, &json!({}))?;
        journey.steps.push(MockUserJourneyStep::key(
            "open profiling failure",
            "enter",
            800,
            ["Profiling Failed", "Suggestions", "Recovery Actions"],
        ));
        Ok(journey)
    }

    fn build_base_ozone_plus_shell_journey(
        &self,
        journey_name: &str,
        _args: &Value,
    ) -> Result<MockUserJourneySpec> {
        Ok(MockUserJourneySpec {
            name: journey_name.to_owned(),
            cwd: self.repo_root.to_string_lossy().into_owned(),
            command: append_args(
                &self.front_door_binary_command("ozone", &["--mode", "base"]),
                &["--frontend", "ozonePlus", "--no-browser"],
            ),
            steps: vec![
                MockUserJourneyStep::wait("splash settle", 5500),
                MockUserJourneyStep::key("advance splash", "enter", 1000, []),
                MockUserJourneyStep::key("confirm launch 1", "enter", 1000, []),
                MockUserJourneyStep::key("confirm launch 2", "enter", 1000, []),
                MockUserJourneyStep::key(
                    "reach ozone plus shell",
                    "enter",
                    3500,
                    [
                        ":memories",
                        "context transcript-fallback",
                        "0 turns via",
                        "Ctrl+K pin",
                    ],
                ),
            ],
        })
    }

    fn build_ozone_plus_main_menu_screen_journey(
        &self,
        journey_name: &str,
        _args: &Value,
    ) -> Result<MockUserJourneySpec> {
        Ok(MockUserJourneySpec {
            name: journey_name.to_owned(),
            cwd: self.repo_root.to_string_lossy().into_owned(),
            command: self
                .front_door_binary_command("ozone-plus", &["handoff", "--launcher-session"]),
            steps: vec![MockUserJourneyStep::wait_for(
                "settle main menu",
                1200,
                ["New Chat", "Sessions", "Characters", "Settings"],
            )],
        })
    }

    fn build_ozone_plus_sessions_screen_journey(
        &self,
        journey_name: &str,
        _args: &Value,
    ) -> Result<MockUserJourneySpec> {
        let mut journey =
            self.build_ozone_plus_main_menu_screen_journey(journey_name, &json!({}))?;
        journey.steps.push(MockUserJourneyStep::text(
            "open sessions",
            "2",
            500,
            ["Sessions", "0 total"],
        ));
        Ok(journey)
    }

    fn build_ozone_plus_characters_screen_journey(
        &self,
        journey_name: &str,
        _args: &Value,
    ) -> Result<MockUserJourneySpec> {
        let mut journey =
            self.build_ozone_plus_main_menu_screen_journey(journey_name, &json!({}))?;
        journey.steps.push(MockUserJourneyStep::text(
            "open characters",
            "3",
            500,
            ["Characters", "session(s)"],
        ));
        Ok(journey)
    }

    fn build_ozone_plus_settings_screen_journey(
        &self,
        journey_name: &str,
        _args: &Value,
    ) -> Result<MockUserJourneySpec> {
        let mut journey =
            self.build_ozone_plus_main_menu_screen_journey(journey_name, &json!({}))?;
        journey.steps.push(MockUserJourneyStep::text(
            "open settings",
            "4",
            500,
            ["Settings", "config.toml", "next session open"],
        ));
        Ok(journey)
    }

    fn build_ozone_plus_character_create_screen_journey(
        &self,
        journey_name: &str,
        _args: &Value,
    ) -> Result<MockUserJourneySpec> {
        let mut journey =
            self.build_ozone_plus_characters_screen_journey(journey_name, &json!({}))?;
        journey.steps.push(MockUserJourneyStep::text(
            "open character create",
            "n",
            600,
            ["New Character", "System Prompt", "Save"],
        ));
        Ok(journey)
    }

    fn build_ozone_plus_character_import_screen_journey(
        &self,
        journey_name: &str,
        _args: &Value,
    ) -> Result<MockUserJourneySpec> {
        let mut journey =
            self.build_ozone_plus_characters_screen_journey(journey_name, &json!({}))?;
        journey.steps.push(MockUserJourneyStep::text(
            "open character import",
            "i",
            600,
            ["Import Character Card", "File Path", "Supports:"],
        ));
        Ok(journey)
    }

    fn build_ozone_plus_conversation_screen_journey(
        &self,
        journey_name: &str,
        _args: &Value,
    ) -> Result<MockUserJourneySpec> {
        let mut journey =
            self.build_ozone_plus_main_menu_screen_journey(journey_name, &json!({}))?;
        journey.steps.push(MockUserJourneyStep::key(
            "open conversation",
            "enter",
            800,
            ["Conversation", "Composer", "Status"],
        ));
        Ok(journey)
    }

    fn build_ozone_plus_help_screen_journey(
        &self,
        journey_name: &str,
        _args: &Value,
    ) -> Result<MockUserJourneySpec> {
        let mut journey =
            self.build_ozone_plus_conversation_screen_journey(journey_name, &json!({}))?;
        journey.steps.push(MockUserJourneyStep::text(
            "open help",
            "?",
            600,
            ["Help", "Slash Commands", "Ctrl+K"],
        ));
        Ok(journey)
    }

    fn front_door_binary_command(&self, binary: &str, args: &[&str]) -> Vec<String> {
        let binary_path = self.repo_root.join("target/debug").join(binary);
        if binary_path.exists() {
            let mut command = vec![binary_path.display().to_string()];
            command.extend(args.iter().map(|value| (*value).to_owned()));
            command
        } else {
            let mut command = vec!["cargo".to_owned(), "run".to_owned(), "--quiet".to_owned()];
            if binary != "ozone" {
                command.push("-p".to_owned());
                command.push(binary.to_owned());
            }
            command.push("--".to_owned());
            command.extend(args.iter().map(|value| (*value).to_owned()));
            command
        }
    }

    fn run_mock_user_journey(
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

    for index, step in enumerate(SPEC["steps"]):
        action = step["action"]
        if action["kind"] == "wait":
            pump(master, proc, action["ms"] / 1000.0)
        elif action["kind"] == "key":
            send_key(master, action["key"])
            pump(master, proc, step["settleMs"] / 1000.0)
        elif action["kind"] == "text":
            send_text(master, action["text"])
            pump(master, proc, step["settleMs"] / 1000.0)
        else:
            fail("unsupported action kind `" + action["kind"] + "`")

        window_snapshot = screen_tail()
        matched = [marker for marker in step.get("expectAny", []) if marker in window_snapshot]
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

    fn run_python_vte_helper(
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

    fn with_repo<T>(
        &self,
        sandbox_id: Option<&str>,
        f: impl FnOnce(SqliteRepository) -> Result<T>,
    ) -> Result<T> {
        self.with_sandbox_env(sandbox_id, || {
            let repo = SqliteRepository::from_xdg()?;
            f(repo)
        })
    }

    fn with_sandbox_env<T>(
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

    fn run_workspace_command(
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

#[derive(Debug)]
struct Sandbox {
    id: String,
    root: PathBuf,
    data_home: PathBuf,
    home: PathBuf,
    models_dir: PathBuf,
    launcher_script: Option<PathBuf>,
    backend: Option<ManagedBackend>,
}

impl Sandbox {
    fn describe(&self) -> Value {
        json!({
            "sandboxId": self.id,
            "root": self.root,
            "dataHome": self.data_home,
            "home": self.home,
            "modelsDir": self.models_dir,
            "launcherScript": self.launcher_script,
            "backend": self.backend.as_ref().map(ManagedBackend::describe)
        })
    }

    fn env_overrides(&self) -> BTreeMap<String, String> {
        let mut env_map = BTreeMap::new();
        env_map.insert(
            "XDG_DATA_HOME".to_owned(),
            self.data_home.display().to_string(),
        );
        env_map.insert("HOME".to_owned(), self.home.display().to_string());
        env_map.insert(
            "OZONE_MODELS_DIR".to_owned(),
            self.models_dir.display().to_string(),
        );
        if let Some(path) = &self.launcher_script {
            env_map.insert(
                "OZONE_KOBOLDCPP_LAUNCHER".to_owned(),
                path.display().to_string(),
            );
        }
        env_map
    }

    fn command_env(&self) -> BTreeMap<String, String> {
        let mut env_map = self.env_overrides();
        if let Ok(value) = env::var("CARGO_HOME") {
            env_map.insert("CARGO_HOME".to_owned(), value);
        } else if let Some(value) = host_toolchain_dir(".cargo") {
            env_map.insert("CARGO_HOME".to_owned(), value);
        }
        if let Ok(value) = env::var("RUSTUP_HOME") {
            env_map.insert("RUSTUP_HOME".to_owned(), value);
        } else if let Some(value) = host_toolchain_dir(".rustup") {
            env_map.insert("RUSTUP_HOME".to_owned(), value);
        }
        if let Some(backend) = &self.backend {
            env_map.insert("OZONE__BACKEND__TYPE".to_owned(), "koboldcpp".to_owned());
            env_map.insert("OZONE__BACKEND__URL".to_owned(), backend.base_url.clone());
        }
        // Carry the host's Python user-site path so pyte/Pillow remain findable
        // even when HOME is overridden to the sandbox home dir.
        if let Ok(pythonpath) = env::var("PYTHONPATH") {
            env_map.insert("PYTHONPATH".to_owned(), pythonpath);
        } else {
            // Derive it from the real HOME before the sandbox override takes effect.
            let real_home = env::var("HOME").unwrap_or_default();
            if !real_home.is_empty() {
                // Mirror Python's default user-site path: $HOME/.local/lib/pythonX.Y/site-packages
                // We don't know the exact X.Y so we glob the known prefix pattern.
                let user_site_base = format!("{real_home}/.local/lib");
                if let Ok(entries) = std::fs::read_dir(&user_site_base) {
                    let paths: Vec<String> = entries
                        .filter_map(|e| e.ok())
                        .filter(|e| {
                            e.file_name()
                                .to_string_lossy()
                                .starts_with("python")
                        })
                        .map(|e| format!("{}/site-packages", e.path().display()))
                        .filter(|p| std::path::Path::new(p).exists())
                        .collect();
                    if !paths.is_empty() {
                        env_map.insert("PYTHONPATH".to_owned(), paths.join(":"));
                    }
                }
            }
        }
        env_map
    }

    fn stop_backend(&mut self) -> Result<bool> {
        let Some(mut backend) = self.backend.take() else {
            return Ok(false);
        };
        let _ = backend.child.kill();
        let _ = backend.child.wait();
        Ok(true)
    }
}

#[derive(Debug)]
struct ManagedBackend {
    child: Child,
    port: u16,
    model_name: String,
    base_url: String,
    log_path: PathBuf,
}

impl ManagedBackend {
    fn describe(&self) -> Value {
        json!({
            "pid": self.child.id(),
            "port": self.port,
            "modelName": self.model_name,
            "baseUrl": self.base_url,
            "logPath": self.log_path
        })
    }
}

type CapturableScreenJourneyBuilder =
    fn(&OzoneMcpServer, &str, &Value) -> Result<MockUserJourneySpec>;
type CapturableScreenSandboxSetup = fn() -> Value;

struct CapturableScreenJourneyDefinition {
    target_screen: &'static str,
    description: &'static str,
    builder: CapturableScreenJourneyBuilder,
    sandbox_setup: CapturableScreenSandboxSetup,
}

fn capturable_screen_journey_builders() -> &'static [CapturableScreenJourneyDefinition] {
    &[
        CapturableScreenJourneyDefinition {
            target_screen: "base_splash",
            description: "Cold-start Ozone splash screen.",
            builder: OzoneMcpServer::build_base_splash_screen_journey,
            sandbox_setup: sandbox_setup_base_splash,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_tier_picker",
            description: "First-run tier picker between splash and launcher.",
            builder: OzoneMcpServer::build_base_tier_picker_screen_journey,
            sandbox_setup: sandbox_setup_base_tier_picker,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_launcher",
            description: "Base Ozone launcher dashboard.",
            builder: OzoneMcpServer::build_base_launcher_screen_journey,
            sandbox_setup: sandbox_setup_base_launcher,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_exit_confirm",
            description: "Launcher exit confirmation dialog.",
            builder: OzoneMcpServer::build_base_exit_confirm_screen_journey,
            sandbox_setup: sandbox_setup_base_launcher,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_settings",
            description: "Base Ozone settings screen.",
            builder: OzoneMcpServer::build_base_settings_screen_journey,
            sandbox_setup: sandbox_setup_base_launcher,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_model_picker_launch",
            description: "Launch-mode model picker.",
            builder: OzoneMcpServer::build_base_model_picker_launch_screen_journey,
            sandbox_setup: sandbox_setup_base_launch_path,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_confirm_launch",
            description: "Launch confirmation dialog before backend start.",
            builder: OzoneMcpServer::build_base_confirm_launch_screen_journey,
            sandbox_setup: sandbox_setup_base_launch_path,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_frontend_choice",
            description: "Frontend choice screen shown when no frontend is preselected.",
            builder: OzoneMcpServer::build_base_frontend_choice_screen_journey,
            sandbox_setup: sandbox_setup_base_launch_path,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_launching",
            description: "Transient launch-progress screen after confirming frontend.",
            builder: OzoneMcpServer::build_base_launching_screen_journey,
            sandbox_setup: sandbox_setup_base_launch_path,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_monitor",
            description: "Live Ozone monitor screen.",
            builder: OzoneMcpServer::build_base_monitor_screen_journey,
            sandbox_setup: sandbox_setup_base_launch_path,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_model_picker_profile",
            description: "Profile-mode model picker.",
            builder: OzoneMcpServer::build_base_model_picker_profile_screen_journey,
            sandbox_setup: sandbox_setup_base_profile_review,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_profile_advisory",
            description: "Profiling advisor overview.",
            builder: OzoneMcpServer::build_base_profile_advisory_screen_journey,
            sandbox_setup: sandbox_setup_base_profile_review,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_profile_confirm",
            description: "Profiling action confirmation dialog.",
            builder: OzoneMcpServer::build_base_profile_confirm_screen_journey,
            sandbox_setup: sandbox_setup_base_profile_run,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_profile_running",
            description: "Profiling in-progress screen.",
            builder: OzoneMcpServer::build_base_profile_running_screen_journey,
            sandbox_setup: sandbox_setup_base_profile_run,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_profile_failure",
            description: "Profiling failure / issue-report screen.",
            builder: OzoneMcpServer::build_base_profile_failure_screen_journey,
            sandbox_setup: sandbox_setup_base_profile_review,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "base_ozone_plus_shell",
            description: "ozone+ conversation shell reached through the base launcher handoff.",
            builder: OzoneMcpServer::build_base_ozone_plus_shell_journey,
            sandbox_setup: sandbox_setup_base_ozone_plus_shell,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "ozone_plus_main_menu",
            description: "ozone+ main menu from direct handoff.",
            builder: OzoneMcpServer::build_ozone_plus_main_menu_screen_journey,
            sandbox_setup: sandbox_setup_ozone_plus_entry,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "ozone_plus_sessions",
            description: "ozone+ session list screen.",
            builder: OzoneMcpServer::build_ozone_plus_sessions_screen_journey,
            sandbox_setup: sandbox_setup_ozone_plus_entry,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "ozone_plus_characters",
            description: "ozone+ character manager screen.",
            builder: OzoneMcpServer::build_ozone_plus_characters_screen_journey,
            sandbox_setup: sandbox_setup_ozone_plus_entry,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "ozone_plus_character_create",
            description: "ozone+ new-character form.",
            builder: OzoneMcpServer::build_ozone_plus_character_create_screen_journey,
            sandbox_setup: sandbox_setup_ozone_plus_entry,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "ozone_plus_character_import",
            description: "ozone+ import-character form.",
            builder: OzoneMcpServer::build_ozone_plus_character_import_screen_journey,
            sandbox_setup: sandbox_setup_ozone_plus_entry,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "ozone_plus_settings",
            description: "ozone+ settings/config screen.",
            builder: OzoneMcpServer::build_ozone_plus_settings_screen_journey,
            sandbox_setup: sandbox_setup_ozone_plus_entry,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "ozone_plus_conversation",
            description: "ozone+ conversation shell from the main menu.",
            builder: OzoneMcpServer::build_ozone_plus_conversation_screen_journey,
            sandbox_setup: sandbox_setup_ozone_plus_entry,
        },
        CapturableScreenJourneyDefinition {
            target_screen: "ozone_plus_help",
            description: "ozone+ help overlay from conversation mode.",
            builder: OzoneMcpServer::build_ozone_plus_help_screen_journey,
            sandbox_setup: sandbox_setup_ozone_plus_entry,
        },
    ]
}

fn sandbox_setup(
    models: &[&str],
    preferences: Option<Value>,
    create_launcher_stub: bool,
    requires_mock_backend: bool,
) -> Value {
    json!({
        "models": models,
        "preferences": preferences,
        "createLauncherStub": create_launcher_stub,
        "requiresMockBackend": requires_mock_backend
    })
}

fn sandbox_setup_base_splash() -> Value {
    sandbox_setup(&[], Some(json!({ "preferredTier": "base" })), false, false)
}

fn sandbox_setup_base_tier_picker() -> Value {
    sandbox_setup(&[], None, false, false)
}

fn sandbox_setup_base_launcher() -> Value {
    sandbox_setup(&[], Some(json!({ "preferredTier": "base" })), false, false)
}

fn sandbox_setup_base_launch_path() -> Value {
    sandbox_setup(
        &["mock-model.gguf"],
        Some(json!({
            "preferredTier": "base",
            "preferredBackend": "KoboldCpp"
        })),
        true,
        false,
    )
}

fn sandbox_setup_base_profile_review() -> Value {
    sandbox_setup(
        &["mock-model.gguf"],
        Some(json!({
            "preferredTier": "base",
            "preferredBackend": "KoboldCpp"
        })),
        false,
        false,
    )
}

fn sandbox_setup_base_profile_run() -> Value {
    sandbox_setup_base_launch_path()
}

fn sandbox_setup_base_ozone_plus_shell() -> Value {
    sandbox_setup(
        &["mock-model.gguf"],
        Some(json!({
            "preferredTier": "base",
            "preferredBackend": "KoboldCpp",
            "preferredFrontend": "OzonePlus"
        })),
        true,
        false,
    )
}

fn sandbox_setup_ozone_plus_entry() -> Value {
    sandbox_setup(&[], None, false, false)
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

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
struct MockUserJourneySpec {
    name: String,
    cwd: String,
    command: Vec<String>,
    steps: Vec<MockUserJourneyStep>,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
struct MockUserJourneyStep {
    name: String,
    action: MockUserAction,
    #[serde(rename = "settleMs")]
    settle_ms: u64,
    #[serde(rename = "expectAny")]
    expect_any: Vec<String>,
}

impl MockUserJourneyStep {
    fn wait(name: &str, ms: u64) -> Self {
        Self::wait_for(name, ms, [])
    }

    fn wait_for(name: &str, ms: u64, expect_any: impl IntoIterator<Item = &'static str>) -> Self {
        Self {
            name: name.to_owned(),
            action: MockUserAction::Wait { ms },
            settle_ms: 0,
            expect_any: expect_any.into_iter().map(str::to_owned).collect(),
        }
    }

    fn key(
        name: &str,
        key: &str,
        settle_ms: u64,
        expect_any: impl IntoIterator<Item = &'static str>,
    ) -> Self {
        Self {
            name: name.to_owned(),
            action: MockUserAction::Key {
                key: key.to_owned(),
            },
            settle_ms,
            expect_any: expect_any.into_iter().map(str::to_owned).collect(),
        }
    }

    fn text(
        name: &str,
        text: &str,
        settle_ms: u64,
        expect_any: impl IntoIterator<Item = &'static str>,
    ) -> Self {
        Self {
            name: name.to_owned(),
            action: MockUserAction::Text {
                text: text.to_owned(),
            },
            settle_ms,
            expect_any: expect_any.into_iter().map(str::to_owned).collect(),
        }
    }
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum MockUserAction {
    Wait { ms: u64 },
    Key { key: String },
    Text { text: String },
}

#[derive(Debug, Serialize)]
struct LauncherSmokeRunnerSpec {
    #[serde(rename = "repoRoot")]
    repo_root: String,
    #[serde(rename = "liveRefreshPath", skip_serializing_if = "Option::is_none")]
    live_refresh_path: Option<String>,
    #[serde(rename = "enterCount")]
    enter_count: u64,
    capture: PtyVteCaptureConfig,
}

#[derive(Debug, Serialize)]
struct MockUserRunnerSpec {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    cwd: String,
    command: Vec<String>,
    steps: Vec<MockUserJourneyStep>,
    #[serde(flatten)]
    capture_settings: MockUserCaptureSettings,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct PtyVteCaptureConfig {
    rows: u16,
    columns: u16,
    #[serde(rename = "tailChars")]
    tail_chars: usize,
    #[serde(rename = "fontSize")]
    font_size: u16,
    #[serde(rename = "pngPath", skip_serializing_if = "Option::is_none")]
    png_path: Option<String>,
    #[serde(rename = "jsonPath", skip_serializing_if = "Option::is_none")]
    json_path: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
struct PtyVteCaptureArtifacts {
    #[serde(rename = "pngPath")]
    png_path: String,
    #[serde(rename = "jsonPath")]
    json_path: String,
}

#[derive(Debug, Serialize, Clone)]
struct MockUserCaptureSettings {
    capture: PtyVteCaptureConfig,
    #[serde(rename = "captureScreenshots")]
    capture_screenshots: bool,
    #[serde(rename = "outputDir", skip_serializing_if = "Option::is_none")]
    output_dir: Option<String>,
    #[serde(rename = "stepCaptures", skip_serializing_if = "Vec::is_empty")]
    step_captures: Vec<PtyVteCaptureArtifacts>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PtyVteCaptureResult {
    screen_rows: u16,
    screen_columns: u16,
    line_count: u16,
    column_count: u16,
    cursor: PtyVteCursor,
    cursor_row: u16,
    cursor_col: u16,
    #[serde(default)]
    display: Vec<String>,
    text: String,
    #[serde(rename = "tailText")]
    tail_text: String,
    #[serde(default)]
    rows: Vec<PtyVteCaptureRow>,
    #[serde(default)]
    grid: Vec<PtyVteCaptureRow>,
    #[serde(rename = "pngPath", default, skip_serializing_if = "Option::is_none")]
    png_path: Option<String>,
    #[serde(rename = "jsonPath", default, skip_serializing_if = "Option::is_none")]
    json_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    font: Option<PtyVteCaptureFont>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Clone)]
struct PtyVteCursor {
    row: u16,
    column: u16,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Clone)]
struct PtyVteCaptureRow {
    #[serde(default)]
    index: u16,
    #[serde(default)]
    row: Option<u16>,
    text: String,
    cells: Vec<PtyVteCaptureCell>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PtyVteCaptureCell {
    column: u16,
    text: String,
    fg: String,
    bg: String,
    bold: bool,
    italics: bool,
    underscore: bool,
    strikethrough: bool,
    blink: bool,
    reverse: bool,
    #[serde(default)]
    resolved_fg: Vec<u8>,
    #[serde(default)]
    resolved_bg: Vec<u8>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PtyVteCaptureFont {
    family: String,
    size: u16,
    cell_width: u32,
    cell_height: u32,
}

#[derive(Debug, Serialize, Clone, Copy)]
struct ScreenRegion {
    top: u16,
    left: u16,
    bottom: u16,
    right: u16,
}

#[derive(Debug, Serialize)]
struct ScreenCheckOutcome {
    index: usize,
    #[serde(rename = "type")]
    check_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    passed: bool,
    summary: String,
    detail: Value,
}

#[derive(Debug, Clone, Copy)]
struct ScreenColorMatch<'a> {
    raw: &'a str,
    resolved: &'a [u8],
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ComparableScreenCell {
    text: String,
    fg: String,
    bg: String,
    bold: bool,
    italics: bool,
    underscore: bool,
    strikethrough: bool,
    blink: bool,
    reverse: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BaselineCompareDiff {
    row: u16,
    column: u16,
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    baseline: Option<ComparableScreenCell>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actual: Option<ComparableScreenCell>,
}

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

fn screenshot_capture_config(
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
            description: "Play through named front-door terminal journeys in real ozone / ozone-plus binaries using PTY input only.",
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
                "required": ["sandboxId"],
                "anyOf": [
                    { "required": ["sandboxId", "journey"] },
                    { "required": ["sandboxId", "target"] }
                ],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "screenshot_tool",
            description: "Navigate to a centralized capturable screen target and save a PNG plus JSON terminal snapshot.",
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
                "required": ["sandboxId", "target", "outputDir"],
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

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

fn read_message(reader: &mut impl BufRead) -> Result<Option<JsonRpcRequest>> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            return Ok(None);
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .context("invalid Content-Length header")?,
            );
        }
    }

    let content_length = content_length.ok_or_else(|| anyhow!("missing Content-Length header"))?;
    let mut payload = vec![0_u8; content_length];
    reader.read_exact(&mut payload)?;
    Ok(Some(
        serde_json::from_slice::<JsonRpcRequest>(&payload)
            .context("failed to parse JSON-RPC request body")?,
    ))
}

fn write_message(writer: &mut impl Write, value: &Value) -> Result<()> {
    let payload = serde_json::to_vec(value)?;
    write!(writer, "Content-Length: {}\r\n\r\n", payload.len())?;
    writer.write_all(&payload)?;
    Ok(())
}

fn success_response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": JSONRPC_VERSION,
        "id": id,
        "result": result
    })
}

fn error_response(id: Value, code: i64, message: String) -> Value {
    json!({
        "jsonrpc": JSONRPC_VERSION,
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

fn command_output_data(output: &std::process::Output) -> Value {
    json!({
        "success": output.status.success(),
        "exitCode": output.status.code(),
        "stdout": String::from_utf8_lossy(&output.stdout),
        "stderr": String::from_utf8_lossy(&output.stderr)
    })
}

fn required_string(args: &Value, key: &str) -> Result<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("missing required string field `{key}`"))
}

fn optional_string(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(ToOwned::to_owned)
}

fn optional_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(Value::as_bool)
}

fn optional_object<'a>(args: &'a Value, key: &str) -> Option<&'a Map<String, Value>> {
    args.get(key).and_then(Value::as_object)
}

fn optional_u64(args: &Value, key: &str) -> Option<u64> {
    args.get(key).and_then(Value::as_u64)
}

fn required_u64(args: &Value, key: &str) -> Result<u64> {
    optional_u64(args, key).ok_or_else(|| anyhow!("missing required integer field `{key}`"))
}

fn host_toolchain_dir(name: &str) -> Option<String> {
    env::var_os("HOME").map(|home| PathBuf::from(home).join(name).display().to_string())
}

fn append_args(command: &[String], args: &[&str]) -> Vec<String> {
    let mut full = command.to_vec();
    full.extend(args.iter().map(|value| (*value).to_owned()));
    full
}

fn checked_u16(value: u64, key: &str) -> Result<u16> {
    u16::try_from(value).map_err(|_| anyhow!("field `{key}` must be <= {}", u16::MAX))
}

fn checked_usize(value: u64, key: &str) -> Result<usize> {
    usize::try_from(value).map_err(|_| anyhow!("field `{key}` is too large"))
}

fn optional_i64(args: &Value, key: &str) -> Option<i64> {
    args.get(key).and_then(Value::as_i64)
}

fn optional_string_array(args: &Value, key: &str) -> Result<Vec<String>> {
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

fn load_screen_capture_sidecar(artifact_path: &str) -> Result<(PathBuf, PtyVteCaptureResult)> {
    let requested_path = PathBuf::from(artifact_path);
    if requested_path.is_dir() {
        bail!(
            "screen capture path `{}` is a directory; provide a JSON sidecar or matching PNG path",
            requested_path.display()
        );
    }

    let sidecar_path = if requested_path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("json"))
    {
        requested_path.clone()
    } else {
        requested_path.with_extension("json")
    };

    if !sidecar_path.exists() {
        if requested_path == sidecar_path {
            bail!(
                "screen capture sidecar `{}` does not exist",
                sidecar_path.display()
            );
        }
        bail!(
            "screen capture sidecar `{}` does not exist for artifact `{}`",
            sidecar_path.display(),
            requested_path.display()
        );
    }

    let sidecar_text = fs::read_to_string(&sidecar_path).with_context(|| {
        format!(
            "failed to read screen capture sidecar {}",
            sidecar_path.display()
        )
    })?;
    let mut capture: PtyVteCaptureResult =
        serde_json::from_str(&sidecar_text).with_context(|| {
            format!(
                "screen capture sidecar {} is not valid JSON",
                sidecar_path.display()
            )
        })?;
    if capture.rows.is_empty() && !capture.grid.is_empty() {
        capture.rows = capture.grid.clone();
    }
    if capture.rows.is_empty() {
        bail!(
            "screen capture sidecar `{}` is missing screen-grid rows",
            sidecar_path.display()
        );
    }
    if capture.rows.iter().any(|row| row.cells.is_empty()) {
        bail!(
            "screen capture sidecar `{}` is missing screen-grid cells",
            sidecar_path.display()
        );
    }

    Ok((sidecar_path, capture))
}

fn evaluate_screen_check(
    index: usize,
    check: &Value,
    capture: &PtyVteCaptureResult,
) -> Result<ScreenCheckOutcome> {
    let object = check
        .as_object()
        .ok_or_else(|| anyhow!("check #{} must be an object", index + 1))?;
    let check_type = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("check #{} is missing string field `type`", index + 1))?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);

    match check_type {
        "text_present" => evaluate_text_present_check(index, name, object, capture),
        "text_absent" => evaluate_text_absent_check(index, name, object, capture),
        "color_at" => evaluate_color_at_check(index, name, object, capture),
        "border_intact" => evaluate_border_intact_check(index, name, object, capture),
        "layout_columns" => evaluate_layout_columns_check(index, name, object, capture),
        "no_overlap" => evaluate_no_overlap_check(index, name, object, capture),
        "baseline_compare" => evaluate_baseline_compare_check(index, name, object, capture),
        other => bail!(
            "check #{} has unsupported type `{other}`; supported types: text_present, text_absent, color_at, border_intact, layout_columns, no_overlap, baseline_compare",
            index + 1
        ),
    }
}

fn evaluate_text_present_check(
    index: usize,
    name: Option<String>,
    check: &Map<String, Value>,
    capture: &PtyVteCaptureResult,
) -> Result<ScreenCheckOutcome> {
    let text = required_check_string(check, "text", index)?;
    if text.is_empty() {
        bail!("check #{} field `text` must not be empty", index + 1);
    }
    let case_sensitive = check
        .get("caseSensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let min_occurrences = optional_check_usize(check, "minOccurrences", index)?.unwrap_or(1);
    let region = region_from_check(check, capture, index, false, "text_present")?;
    let matches = text_matches(capture, region, &text, case_sensitive)?;
    let occurrences = matches.iter().map(|(_, count)| *count).sum::<usize>();
    let passed = occurrences >= min_occurrences;
    let summary = if passed {
        format!(
            "Found `{text}` {occurrences} time(s) in region {}",
            format_region(region)
        )
    } else {
        format!(
            "Expected `{text}` at least {min_occurrences} time(s) in region {}, found {occurrences}",
            format_region(region)
        )
    };

    Ok(ScreenCheckOutcome {
        index,
        check_type: "text_present".to_owned(),
        name,
        passed,
        summary,
        detail: json!({
            "text": text,
            "caseSensitive": case_sensitive,
            "region": region,
            "minOccurrences": min_occurrences,
            "occurrences": occurrences,
            "matches": matches.iter().map(|(row, count)| json!({ "row": row, "count": count })).collect::<Vec<_>>()
        }),
    })
}

fn evaluate_text_absent_check(
    index: usize,
    name: Option<String>,
    check: &Map<String, Value>,
    capture: &PtyVteCaptureResult,
) -> Result<ScreenCheckOutcome> {
    let text = required_check_string(check, "text", index)?;
    if text.is_empty() {
        bail!("check #{} field `text` must not be empty", index + 1);
    }
    let case_sensitive = check
        .get("caseSensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let region = region_from_check(check, capture, index, false, "text_absent")?;
    let matches = text_matches(capture, region, &text, case_sensitive)?;
    let occurrences = matches.iter().map(|(_, count)| *count).sum::<usize>();
    let passed = occurrences == 0;
    let summary = if passed {
        format!(
            "Confirmed `{text}` is absent from region {}",
            format_region(region)
        )
    } else {
        format!(
            "Expected `{text}` to be absent from region {}, found {occurrences} time(s)",
            format_region(region)
        )
    };

    Ok(ScreenCheckOutcome {
        index,
        check_type: "text_absent".to_owned(),
        name,
        passed,
        summary,
        detail: json!({
            "text": text,
            "caseSensitive": case_sensitive,
            "region": region,
            "occurrences": occurrences,
            "matches": matches.iter().map(|(row, count)| json!({ "row": row, "count": count })).collect::<Vec<_>>()
        }),
    })
}

fn evaluate_color_at_check(
    index: usize,
    name: Option<String>,
    check: &Map<String, Value>,
    capture: &PtyVteCaptureResult,
) -> Result<ScreenCheckOutcome> {
    let row = required_check_u16(check, "row", index)?;
    let column = required_check_u16(check, "column", index)?;
    let fg = check.get("fg");
    let bg = check.get("bg");
    if fg.is_none() && bg.is_none() {
        bail!(
            "check #{} `color_at` requires `fg`, `bg`, or both",
            index + 1
        );
    }

    let cell = capture_cell(capture, row, column)?;
    let actual_fg = ScreenColorMatch {
        raw: &cell.fg,
        resolved: &cell.resolved_fg,
    };
    let actual_bg = ScreenColorMatch {
        raw: &cell.bg,
        resolved: &cell.resolved_bg,
    };
    let fg_passed = fg
        .map(|expected| color_matches(expected, actual_fg, "fg", index))
        .transpose()?
        .unwrap_or(true);
    let bg_passed = bg
        .map(|expected| color_matches(expected, actual_bg, "bg", index))
        .transpose()?
        .unwrap_or(true);
    let passed = fg_passed && bg_passed;
    let summary = if passed {
        format!("Color check passed at row {row}, column {column}")
    } else {
        format!("Color check failed at row {row}, column {column}")
    };

    Ok(ScreenCheckOutcome {
        index,
        check_type: "color_at".to_owned(),
        name,
        passed,
        summary,
        detail: json!({
            "row": row,
            "column": column,
            "cellText": cell.text,
            "expected": {
                "fg": fg.cloned(),
                "bg": bg.cloned()
            },
            "actual": {
                "fg": { "raw": cell.fg, "resolved": cell.resolved_fg },
                "bg": { "raw": cell.bg, "resolved": cell.resolved_bg }
            }
        }),
    })
}

fn evaluate_border_intact_check(
    index: usize,
    name: Option<String>,
    check: &Map<String, Value>,
    capture: &PtyVteCaptureResult,
) -> Result<ScreenCheckOutcome> {
    let region = region_from_check(check, capture, index, true, "border_intact")?;
    let width = region_width(region);
    let height = region_height(region);
    if width < 2 || height < 2 {
        bail!(
            "check #{} `border_intact` region {} must be at least 2x2",
            index + 1,
            format_region(region)
        );
    }

    let mut issues = Vec::new();
    let corners = [
        ("topLeft", region.top, region.left),
        ("topRight", region.top, region.right),
        ("bottomLeft", region.bottom, region.left),
        ("bottomRight", region.bottom, region.right),
    ];
    for (label, row, column) in corners {
        if cell_text(capture_cell(capture, row, column)?)
            .trim()
            .is_empty()
        {
            issues.push(
                json!({ "edge": label, "row": row, "column": column, "reason": "corner is blank" }),
            );
        }
    }
    for row in (region.top + 1)..region.bottom {
        if cell_text(capture_cell(capture, row, region.left)?)
            .trim()
            .is_empty()
        {
            issues.push(json!({ "edge": "left", "row": row, "column": region.left, "reason": "left border is blank" }));
        }
        if cell_text(capture_cell(capture, row, region.right)?)
            .trim()
            .is_empty()
        {
            issues.push(json!({ "edge": "right", "row": row, "column": region.right, "reason": "right border is blank" }));
        }
    }

    let top_stats = horizontal_edge_stats(capture, region.top, region.left, region.right)?;
    let bottom_stats = horizontal_edge_stats(capture, region.bottom, region.left, region.right)?;
    if top_stats.max_blank_run > DEFAULT_BORDER_MAX_BLANK_RUN {
        issues.push(json!({
            "edge": "top",
            "reason": format!("top border has blank run {} (> {})", top_stats.max_blank_run, DEFAULT_BORDER_MAX_BLANK_RUN)
        }));
    }
    if bottom_stats.max_blank_run > DEFAULT_BORDER_MAX_BLANK_RUN {
        issues.push(json!({
            "edge": "bottom",
            "reason": format!("bottom border has blank run {} (> {})", bottom_stats.max_blank_run, DEFAULT_BORDER_MAX_BLANK_RUN)
        }));
    }

    let passed = issues.is_empty();
    let summary = if passed {
        format!("Border is intact for region {}", format_region(region))
    } else {
        format!(
            "Border is not intact for region {} ({} issue(s))",
            format_region(region),
            issues.len()
        )
    };

    Ok(ScreenCheckOutcome {
        index,
        check_type: "border_intact".to_owned(),
        name,
        passed,
        summary,
        detail: json!({
            "region": region,
            "topEdge": top_stats,
            "bottomEdge": bottom_stats,
            "issues": issues
        }),
    })
}

fn evaluate_layout_columns_check(
    index: usize,
    name: Option<String>,
    check: &Map<String, Value>,
    capture: &PtyVteCaptureResult,
) -> Result<ScreenCheckOutcome> {
    let count = required_check_usize(check, "count", index)?;
    let min_gap = optional_check_usize(check, "minGap", index)?.unwrap_or(DEFAULT_LAYOUT_MIN_GAP);
    let region = region_from_check(check, capture, index, false, "layout_columns")?;
    let columns = detect_layout_columns(capture, region, min_gap)?;
    let passed = columns.len() == count;
    let summary = if passed {
        format!(
            "Detected {count} layout column(s) in region {}",
            format_region(region)
        )
    } else {
        format!(
            "Expected {count} layout column(s) in region {}, found {}",
            format_region(region),
            columns.len()
        )
    };

    Ok(ScreenCheckOutcome {
        index,
        check_type: "layout_columns".to_owned(),
        name,
        passed,
        summary,
        detail: json!({
            "region": region,
            "count": count,
            "minGap": min_gap,
            "detectedColumns": columns
        }),
    })
}

fn evaluate_no_overlap_check(
    index: usize,
    name: Option<String>,
    check: &Map<String, Value>,
    capture: &PtyVteCaptureResult,
) -> Result<ScreenCheckOutcome> {
    let regions_value = check
        .get("regions")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("check #{} `no_overlap` requires `regions`", index + 1))?;
    if regions_value.len() < 2 {
        bail!(
            "check #{} `no_overlap` requires at least two regions",
            index + 1
        );
    }

    let regions = regions_value
        .iter()
        .enumerate()
        .map(|(region_index, value)| named_region_from_value(value, capture, index, region_index))
        .collect::<Result<Vec<_>>>()?;
    let mut overlaps = Vec::new();
    for (left_index, (left_name, left_region)) in regions.iter().enumerate() {
        for (right_name, right_region) in regions.iter().skip(left_index + 1) {
            if let Some(overlap) = overlapping_region(*left_region, *right_region) {
                overlaps.push(json!({
                    "left": left_name,
                    "right": right_name,
                    "overlap": overlap
                }));
            }
        }
    }

    let passed = overlaps.is_empty();
    let summary = if passed {
        format!("Confirmed {} regions do not overlap", regions.len())
    } else {
        format!("Detected {} overlapping region pair(s)", overlaps.len())
    };

    Ok(ScreenCheckOutcome {
        index,
        check_type: "no_overlap".to_owned(),
        name,
        passed,
        summary,
        detail: json!({
            "regions": regions.iter().map(|(region_name, region)| json!({
                "name": region_name,
                "region": region
            })).collect::<Vec<_>>(),
            "overlaps": overlaps
        }),
    })
}

fn evaluate_baseline_compare_check(
    index: usize,
    name: Option<String>,
    check: &Map<String, Value>,
    capture: &PtyVteCaptureResult,
) -> Result<ScreenCheckOutcome> {
    let baseline_path = check
        .get("baselinePath")
        .or_else(|| check.get("baselineSidecarPath"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            anyhow!(
                "check #{} `baseline_compare` requires `baselinePath` or `baselineSidecarPath`",
                index + 1
            )
        })?;
    let (baseline_sidecar_path, baseline_capture) = load_screen_capture_sidecar(&baseline_path)?;
    let current_cells = comparable_capture_cells(capture);
    let baseline_cells = comparable_capture_cells(&baseline_capture);
    let compared_positions = current_cells
        .keys()
        .chain(baseline_cells.keys())
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut changed_cells = Vec::new();
    let mut sample_diffs = Vec::new();
    let mut row_diff_counts = BTreeMap::new();

    for (row, column) in &compared_positions {
        let actual = current_cells.get(&(*row, *column));
        let baseline = baseline_cells.get(&(*row, *column));
        if actual == baseline {
            continue;
        }

        changed_cells.push(json!({ "row": row, "column": column }));
        *row_diff_counts.entry(*row).or_insert(0_usize) += 1;
        if sample_diffs.len() < 20 {
            sample_diffs.push(BaselineCompareDiff {
                row: *row,
                column: *column,
                kind: baseline_compare_diff_kind(actual, baseline),
                baseline: baseline.cloned(),
                actual: actual.cloned(),
            });
        }
    }

    let diff_count = changed_cells.len();
    let total_cells_compared = compared_positions.len();
    let matched_cells = total_cells_compared.saturating_sub(diff_count);
    let match_percent = if total_cells_compared == 0 {
        100.0
    } else {
        ((matched_cells as f64 / total_cells_compared as f64) * 10_000.0).round() / 100.0
    };
    let dimensions_match = capture.screen_rows == baseline_capture.screen_rows
        && capture.screen_columns == baseline_capture.screen_columns;
    let difference_summary = baseline_difference_summary(
        diff_count,
        &row_diff_counts,
        dimensions_match,
        capture,
        &baseline_capture,
    );
    let passed = diff_count == 0 && dimensions_match;
    let summary = if passed {
        format!(
            "Baseline compare matched {matched_cells}/{total_cells_compared} cells ({match_percent:.2}%)"
        )
    } else if dimensions_match {
        format!(
            "Baseline compare found {diff_count} diff(s) across {total_cells_compared} cells ({match_percent:.2}% match)"
        )
    } else {
        format!(
            "Baseline compare failed: {diff_count} diff(s) and screen dimensions changed (current {}x{}, baseline {}x{})",
            capture.screen_rows,
            capture.screen_columns,
            baseline_capture.screen_rows,
            baseline_capture.screen_columns
        )
    };

    Ok(ScreenCheckOutcome {
        index,
        check_type: "baseline_compare".to_owned(),
        name,
        passed,
        summary,
        detail: json!({
            "baselinePath": baseline_path,
            "baselineSidecarPath": baseline_sidecar_path.display().to_string(),
            "currentScreen": {
                "rows": capture.screen_rows,
                "columns": capture.screen_columns,
                "cellCount": current_cells.len()
            },
            "baselineScreen": {
                "rows": baseline_capture.screen_rows,
                "columns": baseline_capture.screen_columns,
                "cellCount": baseline_cells.len()
            },
            "dimensionsMatch": dimensions_match,
            "changedCells": changed_cells,
            "diffCount": diff_count,
            "matchedCells": matched_cells,
            "totalCellsCompared": total_cells_compared,
            "matchRatio": {
                "matched": matched_cells,
                "total": total_cells_compared
            },
            "matchPercent": match_percent,
            "differenceSummary": difference_summary,
            "rowDiffs": row_diff_counts.iter().map(|(row, count)| json!({
                "row": row,
                "count": count
            })).collect::<Vec<_>>(),
            "sampleDiffs": sample_diffs
        }),
    })
}

fn required_check_string(check: &Map<String, Value>, key: &str, index: usize) -> Result<String> {
    check
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("check #{} is missing string field `{key}`", index + 1))
}

fn required_check_u16(check: &Map<String, Value>, key: &str, index: usize) -> Result<u16> {
    optional_check_u16(check, key, index)?
        .ok_or_else(|| anyhow!("check #{} is missing integer field `{key}`", index + 1))
}

fn required_check_usize(check: &Map<String, Value>, key: &str, index: usize) -> Result<usize> {
    optional_check_usize(check, key, index)?
        .ok_or_else(|| anyhow!("check #{} is missing integer field `{key}`", index + 1))
}

fn optional_check_u16(check: &Map<String, Value>, key: &str, index: usize) -> Result<Option<u16>> {
    match check.get(key) {
        None => Ok(None),
        Some(value) => {
            let raw = value
                .as_u64()
                .ok_or_else(|| anyhow!("check #{} field `{key}` must be an integer", index + 1))?;
            Ok(Some(checked_u16(raw, key)?))
        }
    }
}

fn optional_check_usize(
    check: &Map<String, Value>,
    key: &str,
    index: usize,
) -> Result<Option<usize>> {
    match check.get(key) {
        None => Ok(None),
        Some(value) => {
            let raw = value
                .as_u64()
                .ok_or_else(|| anyhow!("check #{} field `{key}` must be an integer", index + 1))?;
            Ok(Some(checked_usize(raw, key)?))
        }
    }
}

fn region_from_check(
    check: &Map<String, Value>,
    capture: &PtyVteCaptureResult,
    index: usize,
    required: bool,
    check_type: &str,
) -> Result<ScreenRegion> {
    match check.get("region") {
        Some(value) => region_from_value(value, capture, index, "region"),
        None if required => bail!(
            "check #{} `{check_type}` requires a `region` object",
            index + 1
        ),
        None => {
            let row = optional_check_u16(check, "row", index)?;
            Ok(ScreenRegion {
                top: row.unwrap_or(0),
                bottom: row.unwrap_or_else(|| capture.screen_rows.saturating_sub(1)),
                left: 0,
                right: capture.screen_columns.saturating_sub(1),
            })
        }
    }
}

fn region_from_value(
    value: &Value,
    capture: &PtyVteCaptureResult,
    index: usize,
    label: &str,
) -> Result<ScreenRegion> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("check #{} field `{label}` must be an object", index + 1))?;
    let max_row = capture.screen_rows.saturating_sub(1);
    let max_column = capture.screen_columns.saturating_sub(1);
    let top = region_bound(object, "top", 0, max_row, index, label)?;
    let left = region_bound(object, "left", 0, max_column, index, label)?;
    let bottom = region_bound(object, "bottom", max_row, max_row, index, label)?;
    let right = region_bound(object, "right", max_column, max_column, index, label)?;
    if top > bottom {
        bail!(
            "check #{} field `{label}` has top {} greater than bottom {}",
            index + 1,
            top,
            bottom
        );
    }
    if left > right {
        bail!(
            "check #{} field `{label}` has left {} greater than right {}",
            index + 1,
            left,
            right
        );
    }
    Ok(ScreenRegion {
        top,
        left,
        bottom,
        right,
    })
}

fn region_bound(
    object: &Map<String, Value>,
    key: &str,
    default: u16,
    max: u16,
    index: usize,
    label: &str,
) -> Result<u16> {
    let Some(value) = object.get(key) else {
        return Ok(default);
    };
    let raw = value.as_u64().ok_or_else(|| {
        anyhow!(
            "check #{} field `{label}.{key}` must be an integer",
            index + 1
        )
    })?;
    let parsed = checked_u16(raw, key)?;
    if parsed > max {
        bail!(
            "check #{} field `{label}.{key}`={} is outside the screen bounds (max {})",
            index + 1,
            parsed,
            max
        );
    }
    Ok(parsed)
}

fn named_region_from_value(
    value: &Value,
    capture: &PtyVteCaptureResult,
    check_index: usize,
    region_index: usize,
) -> Result<(String, ScreenRegion)> {
    let object = value.as_object().ok_or_else(|| {
        anyhow!(
            "check #{} `regions[{}]` must be an object",
            check_index + 1,
            region_index
        )
    })?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("region_{}", region_index + 1));
    Ok((
        name,
        region_from_value(
            value,
            capture,
            check_index,
            &format!("regions[{region_index}]"),
        )?,
    ))
}

fn format_region(region: ScreenRegion) -> String {
    format!(
        "[top={}, left={}, bottom={}, right={}]",
        region.top, region.left, region.bottom, region.right
    )
}

fn region_width(region: ScreenRegion) -> usize {
    usize::from(region.right - region.left + 1)
}

fn region_height(region: ScreenRegion) -> usize {
    usize::from(region.bottom - region.top + 1)
}

fn text_matches(
    capture: &PtyVteCaptureResult,
    region: ScreenRegion,
    needle: &str,
    case_sensitive: bool,
) -> Result<Vec<(u16, usize)>> {
    let mut matches = Vec::new();
    for row in region.top..=region.bottom {
        let line = row_text_in_region(capture, row, region.left, region.right)?;
        let count = substring_count(&line, needle, case_sensitive);
        if count > 0 {
            matches.push((row, count));
        }
    }
    Ok(matches)
}

fn row_text_in_region(
    capture: &PtyVteCaptureResult,
    row: u16,
    left: u16,
    right: u16,
) -> Result<String> {
    let mut rendered = String::new();
    for column in left..=right {
        rendered.push_str(cell_text(capture_cell(capture, row, column)?));
    }
    Ok(rendered)
}

fn substring_count(haystack: &str, needle: &str, case_sensitive: bool) -> usize {
    if case_sensitive {
        haystack.match_indices(needle).count()
    } else {
        haystack
            .to_lowercase()
            .match_indices(&needle.to_lowercase())
            .count()
    }
}

fn capture_cell(
    capture: &PtyVteCaptureResult,
    row: u16,
    column: u16,
) -> Result<&PtyVteCaptureCell> {
    let row_data = capture.rows.get(usize::from(row)).ok_or_else(|| {
        anyhow!(
            "screen capture is missing row {} (screen rows {})",
            row,
            capture.screen_rows
        )
    })?;
    row_data
        .cells
        .iter()
        .find(|cell| cell.column == column)
        .or_else(|| row_data.cells.get(usize::from(column)))
        .ok_or_else(|| anyhow!("screen capture row {} is missing column {}", row, column))
}

fn cell_text(cell: &PtyVteCaptureCell) -> &str {
    if cell.text.is_empty() {
        " "
    } else {
        &cell.text
    }
}

fn comparable_capture_cells(
    capture: &PtyVteCaptureResult,
) -> BTreeMap<(u16, u16), ComparableScreenCell> {
    let mut cells = BTreeMap::new();
    for (fallback_row, row) in capture.rows.iter().enumerate() {
        let row_number = capture_row_number(row, fallback_row);
        for cell in &row.cells {
            cells.insert((row_number, cell.column), comparable_screen_cell(cell));
        }
    }
    cells
}

fn capture_row_number(row: &PtyVteCaptureRow, fallback_row: usize) -> u16 {
    row.row.unwrap_or_else(|| {
        if row.index > 0 || fallback_row == 0 {
            row.index
        } else {
            u16::try_from(fallback_row).unwrap_or(u16::MAX)
        }
    })
}

fn comparable_screen_cell(cell: &PtyVteCaptureCell) -> ComparableScreenCell {
    ComparableScreenCell {
        text: cell_text(cell).to_owned(),
        fg: comparable_color_label(&cell.fg, &cell.resolved_fg),
        bg: comparable_color_label(&cell.bg, &cell.resolved_bg),
        bold: cell.bold,
        italics: cell.italics,
        underscore: cell.underscore,
        strikethrough: cell.strikethrough,
        blink: cell.blink,
        reverse: cell.reverse,
    }
}

fn comparable_color_label(raw: &str, resolved: &[u8]) -> String {
    if resolved.len() == 3 {
        return format!("rgb({},{},{})", resolved[0], resolved[1], resolved[2]);
    }
    normalize_color_name(raw)
}

fn baseline_compare_diff_kind(
    actual: Option<&ComparableScreenCell>,
    baseline: Option<&ComparableScreenCell>,
) -> &'static str {
    match (actual, baseline) {
        (Some(_), Some(_)) => "changed",
        (Some(_), None) => "added",
        (None, Some(_)) => "missing",
        (None, None) => "unchanged",
    }
}

fn baseline_difference_summary(
    diff_count: usize,
    row_diff_counts: &BTreeMap<u16, usize>,
    dimensions_match: bool,
    current: &PtyVteCaptureResult,
    baseline: &PtyVteCaptureResult,
) -> String {
    if diff_count == 0 && dimensions_match {
        return "No grid differences detected".to_owned();
    }

    let mut parts = Vec::new();
    if !dimensions_match {
        parts.push(format!(
            "dimensions {}x{} -> {}x{}",
            baseline.screen_rows,
            baseline.screen_columns,
            current.screen_rows,
            current.screen_columns
        ));
    }
    if diff_count > 0 {
        let row_summary = row_diff_counts
            .iter()
            .take(5)
            .map(|(row, count)| format!("row {row} ({count})"))
            .collect::<Vec<_>>()
            .join(", ");
        let overflow = row_diff_counts.len().saturating_sub(5);
        if overflow > 0 {
            parts.push(format!(
                "{diff_count} cell diff(s) across {} row(s): {row_summary}, +{overflow} more row(s)",
                row_diff_counts.len()
            ));
        } else {
            parts.push(format!(
                "{diff_count} cell diff(s) across {} row(s): {row_summary}",
                row_diff_counts.len()
            ));
        }
    }

    parts.join("; ")
}

#[derive(Debug, Serialize)]
struct HorizontalEdgeStats {
    filled: usize,
    total: usize,
    max_blank_run: usize,
}

fn horizontal_edge_stats(
    capture: &PtyVteCaptureResult,
    row: u16,
    left: u16,
    right: u16,
) -> Result<HorizontalEdgeStats> {
    let mut filled = 0;
    let mut blank_run = 0;
    let mut max_blank_run = 0;
    for column in left..=right {
        let is_blank = cell_text(capture_cell(capture, row, column)?)
            .trim()
            .is_empty();
        if is_blank {
            blank_run += 1;
            max_blank_run = max_blank_run.max(blank_run);
        } else {
            filled += 1;
            blank_run = 0;
        }
    }
    Ok(HorizontalEdgeStats {
        filled,
        total: usize::from(right - left + 1),
        max_blank_run,
    })
}

fn detect_layout_columns(
    capture: &PtyVteCaptureResult,
    region: ScreenRegion,
    min_gap: usize,
) -> Result<Vec<ScreenRegion>> {
    let mut occupied = vec![false; region_width(region)];
    for row in region.top..=region.bottom {
        for column in region.left..=region.right {
            let cell = capture_cell(capture, row, column)?;
            if !cell_text(cell).trim().is_empty() {
                occupied[usize::from(column - region.left)] = true;
            }
        }
    }

    let raw_runs = occupied_runs(&occupied, region.left);
    if raw_runs.len() <= 1 || min_gap <= 1 {
        return Ok(raw_runs
            .into_iter()
            .map(|(left, right)| ScreenRegion {
                top: region.top,
                bottom: region.bottom,
                left,
                right,
            })
            .collect());
    }

    let mut merged = Vec::new();
    for (left, right) in raw_runs {
        match merged.last_mut() {
            Some((_, previous_right)) if usize::from(left - *previous_right - 1) < min_gap => {
                *previous_right = right;
            }
            _ => merged.push((left, right)),
        }
    }

    Ok(merged
        .into_iter()
        .map(|(left, right)| ScreenRegion {
            top: region.top,
            bottom: region.bottom,
            left,
            right,
        })
        .collect())
}

fn occupied_runs(occupied: &[bool], base_column: u16) -> Vec<(u16, u16)> {
    let mut runs = Vec::new();
    let mut current_start = None;
    for (offset, is_occupied) in occupied.iter().copied().enumerate() {
        match (current_start, is_occupied) {
            (None, true) => current_start = Some(offset),
            (Some(start), false) => {
                runs.push((
                    base_column + u16::try_from(start).unwrap_or(0),
                    base_column + u16::try_from(offset.saturating_sub(1)).unwrap_or(0),
                ));
                current_start = None;
            }
            _ => {}
        }
    }
    if let Some(start) = current_start {
        runs.push((
            base_column + u16::try_from(start).unwrap_or(0),
            base_column + u16::try_from(occupied.len().saturating_sub(1)).unwrap_or(0),
        ));
    }
    runs
}

fn overlapping_region(left: ScreenRegion, right: ScreenRegion) -> Option<ScreenRegion> {
    let top = left.top.max(right.top);
    let left_column = left.left.max(right.left);
    let bottom = left.bottom.min(right.bottom);
    let right_column = left.right.min(right.right);
    (top <= bottom && left_column <= right_column).then_some(ScreenRegion {
        top,
        left: left_column,
        bottom,
        right: right_column,
    })
}

fn color_matches(
    expected: &Value,
    actual: ScreenColorMatch<'_>,
    field: &str,
    index: usize,
) -> Result<bool> {
    match expected {
        Value::String(value) => {
            let normalized = normalize_color_name(value);
            if normalized == normalize_color_name(actual.raw) {
                return Ok(true);
            }
            if let Some(rgb) = parse_hex_color(value).or_else(|| ansi_color_rgb(&normalized)) {
                return Ok(actual.resolved == rgb);
            }
            Ok(false)
        }
        Value::Array(values) => {
            Ok(actual.resolved == parse_rgb_triplet(values, field, index)?.as_slice())
        }
        _ => bail!(
            "check #{} field `{field}` must be a color string or RGB array",
            index + 1
        ),
    }
}

fn parse_rgb_triplet(values: &[Value], field: &str, index: usize) -> Result<[u8; 3]> {
    if values.len() != 3 {
        bail!(
            "check #{} field `{field}` must contain exactly 3 RGB values",
            index + 1
        );
    }
    let mut rgb = [0_u8; 3];
    for (slot, value) in rgb.iter_mut().zip(values.iter()) {
        let component = value.as_u64().ok_or_else(|| {
            anyhow!(
                "check #{} field `{field}` must contain only integers",
                index + 1
            )
        })?;
        *slot = u8::try_from(component).map_err(|_| {
            anyhow!(
                "check #{} field `{field}` RGB values must be between 0 and 255",
                index + 1
            )
        })?;
    }
    Ok(rgb)
}

fn parse_hex_color(value: &str) -> Option<[u8; 3]> {
    let normalized = value.trim().trim_start_matches('#');
    if normalized.len() != 6 || !normalized.chars().all(|char| char.is_ascii_hexdigit()) {
        return None;
    }
    let red = u8::from_str_radix(&normalized[0..2], 16).ok()?;
    let green = u8::from_str_radix(&normalized[2..4], 16).ok()?;
    let blue = u8::from_str_radix(&normalized[4..6], 16).ok()?;
    Some([red, green, blue])
}

fn normalize_color_name(value: &str) -> String {
    value
        .chars()
        .filter(|char| !matches!(char, '_' | '-' | ' '))
        .flat_map(char::to_lowercase)
        .collect()
}

fn ansi_color_rgb(value: &str) -> Option<[u8; 3]> {
    Some(match value {
        "black" => [12, 12, 12],
        "red" => [205, 49, 49],
        "green" => [13, 188, 121],
        "brown" | "yellow" => [229, 229, 16],
        "blue" => [36, 114, 200],
        "magenta" => [188, 63, 188],
        "cyan" => [17, 168, 205],
        "white" => [229, 229, 229],
        "brightblack" => [102, 102, 102],
        "brightred" => [241, 76, 76],
        "brightgreen" => [35, 209, 139],
        "brightyellow" => [245, 245, 67],
        "brightblue" => [59, 142, 234],
        "brightmagenta" => [214, 112, 214],
        "brightcyan" => [41, 184, 219],
        "brightwhite" => [255, 255, 255],
        _ => return None,
    })
}

fn parse_session_id(value: &str) -> Result<SessionId> {
    SessionId::parse(value).map_err(|error| anyhow!(error.to_string()))
}

fn parse_branch_id(value: &str) -> Result<BranchId> {
    BranchId::parse(value).map_err(|error| anyhow!(error.to_string()))
}

fn parse_message_id(value: &str) -> Result<MessageId> {
    MessageId::parse(value).map_err(|error| anyhow!(error.to_string()))
}

fn parse_swipe_group_id(value: &str) -> Result<SwipeGroupId> {
    SwipeGroupId::parse(value).map_err(|error| anyhow!(error.to_string()))
}

fn now_timestamp_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn parse_prefixed_field(text: &str, prefix: &str) -> Option<String> {
    text.lines().find_map(|line| {
        line.strip_prefix(prefix)
            .map(str::trim)
            .map(ToOwned::to_owned)
    })
}

fn sanitize_prefix(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

fn probe_session_lock(repo: &SqliteRepository, session_id: &SessionId) -> Result<Value> {
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

fn session_summary_json(session: &ozone_persist::SessionSummary) -> Value {
    json!({
        "sessionId": session.session_id,
        "name": session.name,
        "characterName": session.character_name,
        "createdAt": session.created_at,
        "lastOpenedAt": session.last_opened_at,
        "messageCount": session.message_count,
        "dbSizeBytes": session.db_size_bytes,
        "tags": session.tags
    })
}

fn branch_record_json(record: &BranchRecord) -> Value {
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

fn message_json(message: &ConversationMessage) -> Value {
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

fn pinned_memory_record_json(record: &ozone_persist::PinnedMemoryRecord) -> Value {
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

fn pinned_memory_view_json(view: &PinnedMemoryView) -> Value {
    json!({
        "record": pinned_memory_record_json(&view.record),
        "isActive": view.is_active,
        "turnsElapsed": view.turns_elapsed,
        "remainingTurns": view.remaining_turns
    })
}

fn swipe_group_json(group: &SwipeGroup) -> Value {
    json!({
        "swipeGroupId": group.swipe_group_id,
        "parentMessageId": group.parent_message_id,
        "parentContextMessageId": group.parent_context_message_id,
        "activeOrdinal": group.active_ordinal
    })
}

fn swipe_candidate_json(candidate: &SwipeCandidate) -> Value {
    json!({
        "swipeGroupId": candidate.swipe_group_id,
        "ordinal": candidate.ordinal,
        "messageId": candidate.message_id,
        "state": candidate.state.as_str(),
        "partialContent": candidate.partial_content,
        "tokensGenerated": candidate.tokens_generated
    })
}

fn render_transcript_text(export: &ozone_persist::TranscriptExport) -> String {
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
        capturable_screen_journey_builders, mock_user_capture_settings, read_message,
        screenshot_capture_config, tool_definitions, MockUserAction, OzoneMcpServer, Sandbox,
    };
    use serde_json::json;
    use std::fs;
    use std::io::{BufReader, Cursor};
    use std::path::{Path, PathBuf};
    use uuid::Uuid;

    #[test]
    fn mcp_messages_parse_with_content_length_framing() {
        let payload = br#"{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}"#;
        let mut framed = format!("Content-Length: {}\r\n\r\n", payload.len()).into_bytes();
        framed.extend_from_slice(payload);
        let mut reader = BufReader::new(Cursor::new(framed));
        let request = read_message(&mut reader).unwrap().unwrap();
        assert_eq!(request.jsonrpc, "2.0");
        assert_eq!(request.method, "ping");
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
            .any(|marker| marker == ":memories"));
        assert!(matches!(
            journey.steps[5].action,
            MockUserAction::Text { .. }
        ));
        assert!(journey.steps[5]
            .expect_any
            .iter()
            .any(|marker| marker == "INSERT"));
        assert!(journey.steps[7]
            .expect_any
            .iter()
            .any(|marker| marker == "koboldcpp backend"));
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
            ("base_settings", "ozone", "Frontend"),
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
            ("base_ozone_plus_shell", "ozone", "Ctrl+K pin"),
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
            json!(["sandboxId", "target", "outputDir"])
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
