#!/usr/bin/env python3
"""Debug MCP communication step by step."""

import subprocess
import json
import os
import fcntl

BINARY = "/home/eric/projects/ozone-rs/target/debug/ozone-mcp"

def set_nonblock(pipe):
    orig = fcntl.fcntl(pipe, fcntl.F_GETFL)
    fcntl.fcntl(pipe, fcntl.F_SETFL, orig | os.O_NONBLOCK)
    return orig

def read_stdout_raw(proc, timeout=10):
    """Read from stdout with timeout."""
    import time
    set_nonblock(proc.stdout)
    deadline = time.time() + timeout
    total = b""
    while time.time() < deadline:
        try:
            chunk = proc.stdout.read(65536)
            if chunk is None or chunk == b"":
                time.sleep(0.05)
                continue
            total += chunk
        except BlockingIOError:
            time.sleep(0.05)
    return total

def read_stderr_raw(proc, timeout=2):
    """Read from stderr with timeout."""
    import time
    set_nonblock(proc.stderr)
    deadline = time.time() + timeout
    total = b""
    while time.time() < deadline:
        try:
            chunk = proc.stderr.read(65536)
            if chunk is None or chunk == b"":
                time.sleep(0.05)
                continue
            total += chunk
        except BlockingIOError:
            time.sleep(0.05)
    return total

def send_message(proc, method, params=None, msg_id=1):
    """Send a JSON-RPC request to the MCP server."""
    request = {"jsonrpc": "2.0", "id": msg_id, "method": method}
    if params is not None:
        request["params"] = params
    body = json.dumps(request)
    header = f"Content-Length: {len(body)}\r\n\r\n"
    raw = (header + body).encode("utf-8")
    proc.stdin.write(raw)
    proc.stdin.flush()
    print(f"SENT {len(body)} bytes: {json.dumps(request)[:150]}")

def parse_messages(raw):
    """Parse Content-Length encoded messages from raw bytes."""
    messages = []
    pos = 0
    while pos < len(raw):
        crlf_pos = raw.find(b"\r\n\r\n", pos)
        if crlf_pos == -1:
            break
        header = raw[pos:crlf_pos].decode("utf-8")
        content_length = int(header.split(":")[1].strip())
        body_start = crlf_pos + 4
        body_end = body_start + content_length
        if body_end > len(raw):
            break
        body = raw[body_start:body_end].decode("utf-8")
        try:
            msg = json.loads(body)
        except:
            msg = {"parse_error": True, "raw": body[:200]}
        messages.append(msg)
        pos = body_end
    return messages

proc = subprocess.Popen(
    [BINARY],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
)
print(f"Started MCP server PID={proc.pid}")

# Step 1: Initialize
print("\n--- Step 1: Initialize ---")
send_message(proc, "initialize", {
    "protocolVersion": "2024-11-05",
    "capabilities": {},
    "clientInfo": {"name": "test", "version": "1.0"}
}, msg_id=1)

raw = read_stdout_raw(proc, timeout=10)
print(f"  Got {len(raw)} bytes stderr: {read_stderr_raw(proc).decode('utf-8', errors='replace')[:500]}")
msgs = parse_messages(raw)
for m in msgs:
    print(f"  RESPONSE: {json.dumps(m, indent=2)[:500]}")

# Step 2: List tools
print("\n--- Step 2: Lists tools")
send_message(proc, "tools/list", msg_id=2)

raw = read_stdout_raw(proc, timeout=10)
print(f"  Got {len(raw)} bytes raw output")
msgs = parse_messages(raw)
for m in msgs:
    result = m.get("result", {})
    tools = result.get("tools", [])
    print(f"  TOOLS ({len(tools)} total): {[t['name'] for t in tools]}")

# Step 3: Workspace status
print("\n--- Step 3: workspace_status ---")
send_message(proc, "tools/call", {
    "name": "workspace_status",
    "arguments": {}
}, msg_id=3)

time.sleep(2)
raw = read_stdout_raw(proc, timeout=10)
print(f"  Got {len(raw)} bytes")
msgs = parse_messages(raw)
for m in msgs:
    result = m.get("result", {})
    print(f"  isError: {result.get('isError')}")
    content = result.get("content", [])
    for c in content:
        print(f"  TEXT: {c.get('text', '')[:300]}")

# Step 4: Session list (no sandbox)
print("\n--- Step 4: session_tool list (no sandbox) ---")
send_message(proc, "tools/call", {
    "name": "session_tool",
    "arguments": {
        "action": "list"
    }
}, msg_id=4)

raw = read_stdout_raw(proc, timeout=30)  # Longer timeout for SQLite operations
print(f"  Got {len(raw)} bytes")
msgs = parse_messages(raw)
for m in msgs:
    result = m.get("result", {})
    print(f"  isError: {result.get('isError')}")
    content = result.get("content", [])
    for c in content:
        print(f"  TEXT: {c.get('text', '')[:500]}")

# Step 5: Create sandbox
print("\n--- Step 5: sandbox_tool create ---")
send_message(proc, "tools/call", {
    "name": "sandbox_tool",
    "arguments": {
        "action": "create",
        "namePrefix": "test-debug"
    }
}, msg_id=5)

raw = read_stdout_raw(proc, timeout=10)
print(f"  Got {len(raw)} bytes")
msgs = parse_messages(raw)
for m in msgs:
    result = m.get("result", {})
    print(f"  isError: {result.get('isError')}")
    struct = result.get("structuredContent", {}).get("data", {})
    print(f"  structuredData: {json.dumps(struct)[:300]}")
    content = result.get("content", [])
    for c in content:
        print(f"  TEXT: {c.get('text', '')[:300]}")

proc.kill()
proc.wait()
print("\nDone - killed MCP server")
