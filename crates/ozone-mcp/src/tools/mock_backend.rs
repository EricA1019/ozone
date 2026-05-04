     1|/// MCP tool: mock backend.
     2|use crate::OzoneMcpServer;
     3|use crate::ToolReply;
     4|use anyhow::Result;
     5|use serde_json::Value;
     6|
     7|pub fn mock_backend_tool(server: &mut OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
     8|    match required_string(args, "action")?.as_str() {
     9|        "start" => server.start_mock_backend(args),
    10|        "stop" => server.stop_mock_backend(args),
    11|        other => Ok(ToolReply::error(
    12|            "Mock backend action failed".to_owned(),
    13|            json!({ "error": format!("unsupported mock backend action `{other}`") }),
    14|        )),
    15|    }
    16|}
    17|
    18|fn start_mock_backend(&mut self, args: &Value) -> Result<ToolReply> {
    19|    let sandbox_id = required_string(args, "sandboxId")?;
    20|    let port = optional_u64(args, "port").unwrap_or(5001) as u16;
    21|    let model_name =
    22|        optional_string(args, "modelName").unwrap_or_else(|| "mock-model.gguf".to_owned());
    23|    let sandbox = self
    24|        .sandboxes
    25|        .get_mut(&sandbox_id)
    26|        .ok_or_else(|| anyhow!("sandbox `{sandbox_id}` was not found"))?;
    27|    sandbox.stop_backend()?;
    28|
    29|    let script_path = sandbox.root.join("mock_kobold.py");
    30|    let log_path = sandbox.root.join("mock_kobold.log");
    31|    let script = format!(
    32|        r#"from http.server import BaseHTTPRequestHandler, HTTPServer
    33|import json
    34|import time
    35|
    36|MODEL_NAME = {model_name:?}
    37|PORT = {port}
    38|
    39|class Handler(BaseHTTPRequestHandler):
    40|def log_message(self, fmt, *args):
    41|    pass
    42|
    43|def _json(self, payload, code=200):
    44|    data = json.dumps(payload).encode("utf-8")
    45|    server.send_response(code)
    46|    server.send_header("Content-Type", "application/json")
    47|    server.send_header("Content-Length", str(len(data)))
    48|    server.end_headers()
    49|    server.wfile.write(data)
    50|
    51|def do_GET(self):
    52|    if server.path == "/api/v1/model":
    53|        return server._json({{"result": MODEL_NAME}})
    54|    if server.path == "/api/v1/config/max_context_length":
    55|        return server._json({{"value": 8192}})
    56|    if server.path == "/api/extra/perf":
    57|        return server._json({{"last_process_speed": 12.5, "last_eval_speed": 18.0}})
    58|    return server._json({{"error": "not found", "path": server.path}}, code=404)
    59|
    60|def do_POST(self):
    61|    if server.path != "/api/extra/generate/stream":
    62|        return server._json({{"error": "not found", "path": server.path}}, code=404)
    63|
    64|    length = int(server.headers.get("Content-Length", "0") or 0)
    65|    payload = server.rfile.read(length) if length else b""
    66|    prompt = ""
    67|    if payload:
    68|        try:
    69|            prompt = json.loads(payload.decode("utf-8")).get("prompt", "")
    70|        except Exception:
    71|            prompt = ""
    72|    prompt = (prompt or "").lower()
    73|    if "observatory" in prompt:
    74|        tokens = ["The", " observatory", " key", " is", " logged."]
    75|    elif "second" in prompt:
    76|        tokens = ["Second", " reply", " confirmed."]
    77|    else:
    78|        tokens = ["Hello", " from", " mock", " backend."]
    79|
    80|    server.send_response(200)
    81|    server.send_header("Content-Type", "text/event-stream")
    82|    server.send_header("Cache-Control", "no-cache")
    83|    server.end_headers()
    84|    for token in tokens:
    85|        server.wfile.write(f"data: {{json.dumps({{'token': token}})}}\n\n".encode("utf-8"))
    86|        server.wfile.flush()
    87|        time.sleep(0.02)
    88|    server.wfile.write(b'data: {{"done": true}}\n\n')
    89|    server.wfile.flush()
    90|
    91|HTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
    92|"#,
    93|    );
    94|    fs::write(&script_path, script)?;
    95|
    96|    let log_file = fs::File::create(&log_path)?;
    97|    let child = Command::new("python3")
    98|        .arg(&script_path)
    99|        .stdout(Stdio::from(log_file.try_clone()?))
   100|        .stderr(Stdio::from(log_file))
   101|        .spawn()
   102|        .with_context(|| "failed to launch python3 mock backend")?;
   103|    thread::sleep(Duration::from_millis(300));
   104|
   105|    let base_url = format!("http://127.0.0.1:{port}");
   106|    let pid = child.id();
   107|    sandbox.backend = Some(ManagedBackend {
   108|        child,
   109|        port,
   110|        model_name: model_name.clone(),
   111|        base_url: base_url.clone(),
   112|        log_path: log_path.clone(),
   113|    });
   114|
   115|    Ok(ToolReply::success(
   116|        "Started mock backend".to_owned(),
   117|        json!({
   118|            "sandboxId": sandbox_id,
   119|            "pid": pid,
   120|            "port": port,
   121|            "baseUrl": base_url,
   122|            "modelName": model_name,
   123|            "logPath": log_path
   124|        }),
   125|    ))
   126|}
   127|
   128|fn stop_mock_backend(&mut self, args: &Value) -> Result<ToolReply> {
   129|    let sandbox_id = required_string(args, "sandboxId")?;
   130|    let sandbox = self
   131|        .sandboxes
   132|        .get_mut(&sandbox_id)
   133|        .ok_or_else(|| anyhow!("sandbox `{sandbox_id}` was not found"))?;
   134|    let stopped = sandbox.stop_backend()?;
   135|    Ok(ToolReply::success(
   136|        "Stopped mock backend".to_owned(),
   137|        json!({
   138|            "sandboxId": sandbox_id,
   139|            "stopped": stopped
   140|        }),
   141|    ))
   142|}
   143|
   144|