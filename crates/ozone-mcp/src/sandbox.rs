use std::{
    collections::BTreeMap,
    env, fs,
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use serde_json::json;
use serde_json::Value;

use super::{
    default_preferences_json, merge_json_objects, normalize_preferences_json, optional_bool,
    optional_i64, optional_string, optional_string_array, optional_u64, required_string,
    sanitize_prefix, OzoneMcpServer, ToolReply,
};
use uuid::Uuid;

#[derive(Debug)]
pub struct Sandbox {
    pub id: String,
    pub root: PathBuf,
    pub data_home: PathBuf,
    pub home: PathBuf,
    pub models_dir: PathBuf,
    pub launcher_script: Option<PathBuf>,
    pub backend: Option<ManagedBackend>,
}

impl Sandbox {
    pub fn describe(&self) -> Value {
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

    pub fn env_overrides(&self) -> BTreeMap<String, String> {
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

    pub fn command_env(&self) -> BTreeMap<String, String> {
        use std::env;

        let mut env_map = self.env_overrides();
        if let Ok(value) = env::var("CARGO_HOME") {
            env_map.insert("CARGO_HOME".to_owned(), value);
        } else if let Some(value) = super::host_toolchain_dir(".cargo") {
            env_map.insert("CARGO_HOME".to_owned(), value);
        }
        if let Ok(value) = env::var("RUSTUP_HOME") {
            env_map.insert("RUSTUP_HOME".to_owned(), value);
        } else if let Some(value) = super::host_toolchain_dir(".rustup") {
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
                        .filter(|e| e.file_name().to_string_lossy().starts_with("python"))
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

    pub fn stop_backend(&mut self) -> Result<bool> {
        let Some(mut backend) = self.backend.take() else {
            return Ok(false);
        };
        let _ = backend.child.kill();
        let _ = backend.child.wait();
        Ok(true)
    }
}

#[derive(Debug)]
pub struct ManagedBackend {
    pub child: Child,
    pub port: u16,
    pub model_name: String,
    pub base_url: String,
    pub log_path: PathBuf,
}

impl ManagedBackend {
    pub fn describe(&self) -> Value {
        json!({
            "pid": self.child.id(),
            "port": self.port,
            "modelName": self.model_name,
            "baseUrl": self.base_url,
            "logPath": self.log_path
        })
    }
}

impl OzoneMcpServer {
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
}