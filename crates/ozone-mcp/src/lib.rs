use std::{
    collections::BTreeMap,
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
            "mock_user_tool" => self.mock_user_tool(&arguments)?,
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
        let repo_root = self.repo_root.to_string_lossy().into_owned();
        let live_refresh_path = serde_json::to_string(
            &refresh_model_path.map(|path| path.to_string_lossy().into_owned()),
        )?;
        let script = format!(
            r#"import json
import os
import pty
import re
import select
import signal
import subprocess
import time
import fcntl
import struct
import termios
import fcntl
import struct
import termios

REPO_ROOT = {repo_root}
LIVE_REFRESH_PATH = {live_refresh_path}
ENTER_COUNT = {enter_count}

master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
proc = subprocess.Popen(
    ["cargo", "run", "--quiet", "--", "--mode", "base", "--frontend", "ozonePlus", "--no-browser"],
    cwd=REPO_ROOT,
    stdin=slave,
    stdout=slave,
    stderr=slave,
    start_new_session=True,
    env=os.environ.copy(),
)
os.close(slave)
buffer = bytearray()

def pump(seconds):
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
            buffer.extend(chunk)

def send_enter():
    os.write(master, b"\r")

pump(5.5)
if LIVE_REFRESH_PATH:
    open(LIVE_REFRESH_PATH, "ab").close()
    pump(2.5)

for index in range(ENTER_COUNT):
    send_enter()
    pump(3.0 if index + 1 == ENTER_COUNT else 1.0)

text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", buffer.decode("utf-8", errors="ignore"))
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

print(json.dumps({{
    "bufferTail": text[-1200:],
    "sawLiveRefreshModel": bool(LIVE_REFRESH_PATH and os.path.basename(LIVE_REFRESH_PATH) in text),
}}))
"#,
            repo_root = serde_json::to_string(&repo_root)?,
            live_refresh_path = live_refresh_path,
            enter_count = enter_count,
        );
        let mut command = Command::new("python3");
        command.arg("-c").arg(script).current_dir(&self.repo_root);
        command.envs(sandbox.command_env());
        let output = command
            .output()
            .context("failed to run launcher smoke helper")?;
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
        let journey_name = required_string(args, "journey")?;
        let journey = self.build_mock_user_journey(&journey_name, args)?;
        let data = self.run_mock_user_journey(&sandbox_id, &journey)?;
        let success = data
            .get("success")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Ok(if success {
            ToolReply::success(
                format!("Completed mock-user journey `{journey_name}`"),
                data,
            )
        } else {
            ToolReply::error(format!("Mock-user journey `{journey_name}` failed"), data)
        })
    }

    fn build_mock_user_journey(
        &self,
        journey_name: &str,
        args: &Value,
    ) -> Result<MockUserJourneySpec> {
        let repo_root = self.repo_root.to_string_lossy().into_owned();
        let ozone_base_command = self.front_door_binary_command("ozone", &["--mode", "base"]);
        match journey_name {
            "launcher_monitor_roundtrip" => Ok(MockUserJourneySpec {
                name: journey_name.to_owned(),
                cwd: repo_root,
                command: append_args(
                    &ozone_base_command,
                    &["--frontend", "sillyTavern", "--no-browser"],
                ),
                steps: vec![
                    MockUserJourneyStep::wait("splash settle", 5500),
                    MockUserJourneyStep::key("advance splash", "enter", 1000, []),
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
                    MockUserJourneyStep::text(
                        "return to launcher",
                        "r",
                        1200,
                        ["Launch", "Open ozone+", "Settings"],
                    ),
                ],
            }),
            "launcher_to_ozone_plus" => Ok(MockUserJourneySpec {
                name: journey_name.to_owned(),
                cwd: repo_root,
                command: append_args(
                    &ozone_base_command,
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
            }),
            "ozone_plus_chat_journey" => {
                let prompt = optional_string(args, "prompt")
                    .unwrap_or_else(|| "Check the observatory key".to_owned());
                Ok(MockUserJourneySpec {
                    name: journey_name.to_owned(),
                    cwd: repo_root,
                    command: append_args(
                        &ozone_base_command,
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
                            2500,
                            [
                                ":memories",
                                "context transcript-fallback",
                                "0 turns via",
                                "Ctrl+K pin",
                            ],
                        ),
                        MockUserJourneyStep::text("enter insert mode", "i", 400, ["INSERT"]),
                        MockUserJourneyStep::text("type prompt", &prompt, 400, []),
                        MockUserJourneyStep::key(
                            "send prompt",
                            "enter",
                            3500,
                            ["koboldcpp backend", "observatory", "logged", "mock backend"],
                        ),
                    ],
                })
            }
            other => bail!("unsupported mock-user journey `{other}`"),
        }
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
    ) -> Result<Value> {
        let sandbox = self
            .sandboxes
            .get(sandbox_id)
            .ok_or_else(|| anyhow!("sandbox `{sandbox_id}` was not found"))?;
        let spec_json = serde_json::to_string(journey)?;
        let script_template = r#"import json
import os
import pty
import re
import select
import signal
import subprocess
import time
import fcntl
import struct
import termios

SPEC = json.loads(__SPEC_JSON__)
ANSI_RE = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
KEY_BYTES = {
    "enter": b"\r",
    "esc": b"\x1b",
    "up": b"\x1b[A",
    "down": b"\x1b[B",
    "right": b"\x1b[C",
    "left": b"\x1b[D",
    "tab": b"\t",
}

master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
child_env = os.environ.copy()
child_env.setdefault("TERM", "xterm-color")
child_env["LINES"] = "40"
child_env["COLUMNS"] = "120"
proc = subprocess.Popen(
    SPEC["command"],
    cwd=SPEC["cwd"],
    stdin=slave,
    stdout=slave,
    stderr=slave,
    start_new_session=True,
    env=child_env,
)
os.close(slave)
buffer = bytearray()

def pump(seconds):
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
            buffer.extend(chunk)
        if proc.poll() is not None and not ready:
            break

def cleaned_text(data=None):
    if data is None:
        data = buffer
    text = bytes(data).decode("utf-8", errors="ignore")
    text = ANSI_RE.sub("", text)
    return text.replace("\r", "\n")

results = []
for step in SPEC["steps"]:
    start_len = len(buffer)
    action = step["action"]
    if action["kind"] == "wait":
        pump(action["ms"] / 1000.0)
    elif action["kind"] == "key":
        os.write(master, KEY_BYTES[action["key"]])
        pump(step["settleMs"] / 1000.0)
    elif action["kind"] == "text":
        os.write(master, action["text"].encode("utf-8"))
        pump(step["settleMs"] / 1000.0)
    else:
        raise RuntimeError("unsupported action kind " + action["kind"])

    snapshot = cleaned_text()
    delta_snapshot = cleaned_text(buffer[start_len:])
    window_snapshot = snapshot[-1600:]
    matched = [marker for marker in step.get("expectAny", []) if marker in window_snapshot]
    ok = True if not step.get("expectAny") else bool(matched)
    results.append({
        "name": step["name"],
        "action": action["kind"],
        "ok": ok,
        "matchedMarkers": matched,
        "tail": (delta_snapshot or window_snapshot)[-1200:],
    })

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

final_text = cleaned_text()
all_ok = all(step["ok"] for step in results)
visible_markers = sorted({marker for step in results for marker in step["matchedMarkers"]})

print(json.dumps({
    "journey": SPEC["name"],
    "command": SPEC["command"],
    "success": all_ok,
    "rawBytes": len(buffer),
    "steps": results,
    "visibleMarkersReached": visible_markers,
    "processExitedBeforeStop": process_exited,
    "exitCode": exit_code,
    "finalTail": final_text[-1600:],
}))
"#;
        let script = script_template.replace("__SPEC_JSON__", &serde_json::to_string(&spec_json)?);
        let mut command = Command::new("python3");
        command.arg("-c").arg(script).current_dir(&self.repo_root);
        command.envs(sandbox.command_env());
        let output = command
            .output()
            .context("failed to run mock-user PTY helper")?;
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

#[derive(Debug, Serialize)]
struct MockUserJourneySpec {
    name: String,
    cwd: String,
    command: Vec<String>,
    steps: Vec<MockUserJourneyStep>,
}

#[derive(Debug, Serialize)]
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
        Self {
            name: name.to_owned(),
            action: MockUserAction::Wait { ms },
            settle_ms: 0,
            expect_any: Vec::new(),
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

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum MockUserAction {
    Wait { ms: u64 },
    Key { key: String },
    Text { text: String },
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
            name: "mock_user_tool",
            description: "Play through named front-door terminal journeys in real ozone / ozone-plus binaries using PTY input only.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "sandboxId": { "type": "string" },
                    "journey": {
                        "type": "string",
                        "enum": [
                            "launcher_monitor_roundtrip",
                            "launcher_to_ozone_plus",
                            "ozone_plus_chat_journey"
                        ]
                    },
                    "prompt": { "type": "string" }
                },
                "required": ["sandboxId", "journey"],
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
    use super::{read_message, MockUserAction, OzoneMcpServer};
    use serde_json::json;
    use std::io::{BufReader, Cursor};

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
}
