/// Manage mock backend for sandboxes (start/stop mock HTTP servers)
use anyhow::Context;

pub fn mock_backend_tool(server: &mut crate::OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<crate::ToolReply> {
    match super::required_string(args, "action")? {
        "start" => start_mock_backend(server, args),
        "stop" => stop_mock_backend(server, args),
        other => Ok(crate::ToolReply::error(
            "Mock backend action failed".to_owned(),
            serde_json::json!({ "error": format!("unsupported mock backend action `{other}`") }),
        )),
    }
}

fn start_mock_backend(server: &mut crate::OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<crate::ToolReply> {
    let sandbox_id = super::required_string(args, "sandboxId")?;
    let port = super::optional_u64(args, "port").unwrap_or(5001) as u16;
    let model_name = super::optional_string(args, "modelName")
        .unwrap_or("mock-model.gguf");
    let sandbox = server
        .sandboxes
        .get_mut(&sandbox_id.to_owned())
        .ok_or_else(|| anyhow::anyhow!("sandbox `{sandbox_id}` was not found"))?;
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
    std::fs::write(&script_path, script)?;

    let log_file = std::fs::File::create(&log_path)?;
    let child = std::process::Command::new("python3")
        .arg(&script_path)
        .stdout(std::process::Stdio::from(log_file.try_clone()?))
        .stderr(std::process::Stdio::from(log_file))
        .spawn()
        .with_context(|| "failed to launch python3 mock backend")?;
    std::thread::sleep(std::time::Duration::from_millis(300));

    let base_url = format!("http://127.0.0.1:{port}");
    let pid = child.id();
    sandbox.backend = Some(crate::ManagedBackend {
        child,
        port,
        model_name: model_name.to_string(),
        base_url: base_url.clone(),
        log_path: log_path.clone(),
    });

    Ok(crate::ToolReply::success(
        "Started mock backend".to_owned(),
        serde_json::json!({
            "sandboxId": sandbox_id,
            "pid": pid,
            "port": port,
            "baseUrl": base_url,
            "modelName": model_name,
            "logPath": log_path
        }),
    ))
}

fn stop_mock_backend(server: &mut crate::OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<crate::ToolReply> {
    let sandbox_id = super::required_string(args, "sandboxId")?;
    let sandbox = server
        .sandboxes
        .get_mut(&sandbox_id.to_owned())
        .ok_or_else(|| anyhow::anyhow!("sandbox `{sandbox_id}` was not found"))?;
    let stopped = sandbox.stop_backend()?;
    Ok(crate::ToolReply::success(
        "Stopped mock backend".to_owned(),
        serde_json::json!({
            "sandboxId": sandbox_id,
            "stopped": stopped
        }),
    ))
}
