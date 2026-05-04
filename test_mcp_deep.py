#!/usr/bin/env python3
"""Deep MCP test for ozone-rs - systematic testing of ALL tools"""
import subprocess
import json
import os
import sys
import time

os.chdir(os.path.expanduser("~/projects/ozone-rs"))

print("Building workspace...")
subprocess.run(["cargo", "build", "-p", "ozone-mcp-app", "-q"], capture_output=True)
print("Done building.\n")

class MCPClient:
    def __init__(self, binary_path="./target/debug/ozone-mcp"):
        self.proc = subprocess.Popen(
            [binary_path],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE
        )
        self._id = 0

    def _send_raw(self, obj):
        body = json.dumps(obj).encode('utf-8')
        header = f"Content-Length: {len(body)}\r\n\r\n".encode()
        self.proc.stdin.write(header + body)
        self.proc.stdin.flush()

    def initialize(self):
        self._send_raw({
            "jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": {"protocolVersion": "2024-11-05", "capabilities": {},
                       "clientInfo": {"name": "test", "version": "0.1"}}
        })
        self._read()
        self._send_raw({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def _read(self):
        headers = {}
        while True:
            line = self.proc.stdout.readline()
            if line in (b'\r\n', b'\n', b''):
                break
            if b':' in line:
                k, v = line.decode().split(':', 1)
                headers[k.strip()] = v.strip()
        cl = int(headers.get('Content-Length', '0'))
        body = self.proc.stdout.read(cl)
        try:
            return json.loads(body.decode('utf-8'))
        except:
            return {"raw_error": body[:200]}

    def call(self, name, args=None):
        self._id += 1
        self._send_raw({
            "jsonrpc": "2.0", "id": self._id, "method": "tools/call",
            "params": {"name": name, "arguments": args or {}}
        })
        return self._read()

    def get_json(self, resp):
        """Try to extract JSON from MCP text response"""
        if not resp or "result" not in resp:
            return None
        content = resp.get("result", {}).get("content", [])
        for c in content:
            if isinstance(c, dict) and "text" in c:
                try:
                    # Find JSON block
                    text = c["text"]
                    idx = text.find("{")
                    if idx >= 0:
                        return json.loads(text[idx:])
                except:
                    pass
        return None

    def cleanup(self):
        self.proc.kill()

client = MCPClient()
client.initialize()

bugs = []
warnings = []
passes = []

def mark(tool, status, detail, severity="bug"):
    entry = {"tool": tool, "status": status, "detail": detail, "severity": severity}
    if severity == "bug":
        bugs.append(entry)
    elif severity == "warning":
        warnings.append(entry)
    else:
        passes.append(entry)
    marker = "BUG" if severity == "bug" else "WARN" if severity == "warning" else "OK"
    print(f"  [{marker}] {tool}: {status} | {detail[:100]}")

# ============================================================
# TEST SUITE A: Session Management
# ============================================================
print("=" * 60)
print("A. SESSION MANAGEMENT")
print("=" * 60)

# A1: List sessions
print("\nA1: session_tool list")
resp = client.call("session_tool", {"action": "list"})
data = client.get_json(resp)
if data:
    count = data.get("found", 0)
    sessions = data.get("sessions", [])
    print(f"  Found {count} sessions, {len(sessions)} in array")
    
    # Check field completeness
    if sessions:
        s = sessions[0]
        missing = [k for k in ["sessionId", "name", "messageCount", "createdAt"] if k not in s]
        if missing:
            mark("session_tool list", "incomplete", f"Missing fields: {missing}", "warning")
        else:
            mark("session_tool list", "pass", f"All fields present")
        
        # Check for lastMessagePreview
        if "lastMessagePreview" not in s and "last_message_preview" not in s:
            mark("session_tool list", "missing", "no last message preview in list", "warning")
        else:
            mark("session_tool list", "pass", "last message preview present")
    else:
        mark("session_tool list", "empty", "No sessions returned")
else:
    mark("session_tool list", "fail", "No JSON in response", "bug")

# A2: Search for the new session we created
print("\nA2: session_tool list - verify MCP-Auto-Test exists")
resp = client.call("session_tool", {"action": "list"})
data = client.get_json(resp)
if data:
    sessions = data.get("sessions", [])
    found = [s for s in sessions if "MCP-Auto-Test" in s.get("name", "")]
    if found:
        mark("session_tool list", "pass", f"MCP-Auto-Test found: {found[0]['sessionId']}")
    else:
        mark("session_tool list", "missing", "MCP-Auto-Test not in list", "bug")

# A3: session_tool get (should this work?)
print("\nA3: session_tool get (details)")
resp = client.call("session_tool", {"action": "get", "sessionId": "00000000-4001", "test": True})
data = client.get_json(resp) or {}
error = data.get("error", "")
if "unsupported" in error.lower():
    mark("session_tool get", "missing", "session_tool has no 'get' action - user can't inspect a session's details", "warning")
else:
    mark("session_tool get", "unexpected", f"Got: {error[:100]}")

# A4: session_tool rename
print("\nA4: session_tool rename")
resp = client.call("session_tool", {"action": "rename", "sessionId": "00000000-0000-4001-98ab-8d876a638efd", "newName": "Renamed-By-MCP"})
text = resp.get("result", {}).get("content", [{}])[0].get("text", "")
if "error" in text.lower():
    mark("session_tool rename", "broken", text[:100], "bug")
else:
    mark("session_tool rename", "pass", "Rename succeeded")

# A5: session_tool delete
print("\nA5: session_tool delete (dry run)")
# Don't actually delete - just test the action exists
resp = client.call("session_tool", {"action": "delete", "sessionId": "nonexistent-test-id"})
text = resp.get("result", {}).get("content", [{}])[0].get("text", "")
if "unsupported" in text.lower():
    mark("session_tool delete", "missing", "No delete action exists", "warning")
else:
    mark("session_tool delete", "exists", text[:100])

# A6: Get session with many messages (Launcher Session)
print("\nA6: session_tool - get Launcher Session details")
resp = client.call("session_tool", {"action": "list"}) 
data = client.get_json(resp) or {}
sessions = data.get("sessions", [])
launcher = [s for s in sessions if "Launcher Session" in s.get("name", "")]
if launcher:
    ls = launcher[0]
    msg_count = ls.get("messageCount", 0)
    print(f"  Launcher Session has {msg_count} messages")
    if msg_count > 0:
        mark("session_list", "ok", f"Message count tracking works ({msg_count} msgs)")
    else:
        mark("session_list", "bug", "Message count is 0 despite having messages", "bug")

# ============================================================
# TEST SUITE B: Message Operations  
# ============================================================
print("\n" + "=" * 60)
print("B. MESSAGE OPERATIONS")
print("=" * 60)

# B1: message_tool actions
print("\nB1: message_tool available actions")
for action in ["list", "get", "send", "delete"]:
    resp = client.call("message_tool", {"action": action, "sessionId": "00000000-0000-4001-98af-69defe9ac153"})
    text = resp.get("result", {}).get("content", [{}])[0].get("text", "")
    if "unsupported" in text.lower():
        mark("message_tool " + action, "missing", f"Action '{action}' not supported", "warning")
    else:
        short = text[:100].replace("\n", " ")
        mark("message_tool " + action, "ok", short)

# B2: Get messages from session with content
print("\nB2: Get messages from Greeting Verification Test (11 messages)")
resp = client.call("message_tool", {"action": "get_messages", "sessionId": "00000000-0000-4001-98ab-7d1fd66fd9c1"})
text = resp.get("result", {}).get("content", [{}])[0].get("text", "")
if "error" in text.lower() or "fail" in text.lower():
    mark("message_tool get_messages", "broken", text[:100], "bug")
else:
    # Count messages in response
    if "</antThinking>" in text:
        mark("message_tool get_messages", "leak", "Messages contain </antThinking> tokens!", "bug")
    elif "antThinking>" in text:
        mark("message_tool get_messages", "leak", "Messages contain antThinking tokens!", "bug")
    else:
        mark("message_tool get_messages", "ok", "Messages retrieved cleanly")

# ============================================================
# TEST SUITE C: Memory System
# ============================================================
print("\n" + "=" * 60)
print("C. MEMORY SYSTEM") 
print("=" * 60)

session_id = "00000000-0000-4001-98af-69defe9ac153"

# C1: Pin memory
print("\nC1: memory_tool pin")
resp = client.call("memory_tool", {"action": "pin", "sessionId": session_id,
    "messageId": "00000000-0000-4001-98a1-000000000001", "text": "Test MCP pin"})
text = resp.get("result", {}).get("content", [{}])[0].get("text", "")
if "error" in text.lower() or "missing" in text.lower():
    mark("memory_tool pin", "error", text[:100], "warning")
else:
    mark("memory_tool pin", "ok", text[:80])

# C2: List pinned memories
print("\nC2: memory_tool list pinned")
resp = client.call("memory_tool", {"action": "list", "sessionId": session_id})
text = resp.get("result", {}).get("content", [{}])[0].get("text", "")
if "error" in text.lower():
    mark("memory_tool list", "broken", text[:100], "bug")
else:
    mark("memory_tool list", "ok", text[:80])

# C3: Note memory
print("\nC3: memory_tool note")
resp = client.call("memory_tool", {"action": "note", "sessionId": session_id,
    "text": "MCP test note", "keywords": "test,mcp"})
text = resp.get("result", {}).get("content", [{}])[0].get("text", "")
if "error" in text.lower():
    mark("memory_tool note", "error", text[:100])
else:
    mark("memory_tool note", "ok", text[:80])

# ============================================================
# TEST SUITE D: Branch/Swipe
# ============================================================
print("\n" + "=" * 60)
print("D. BRANCH/SWIPE")
print("=" * 60)

# D1: Branch list
print("\nD1: branch_tool list")
resp = client.call("branch_tool", {"action": "list", "sessionId": session_id})
data = client.get_json(resp) or {}
branches = data.get("branches", [])
if branches:
    mark("branch_tool list", "ok", f"Found {len(branches)} branches")
    b = branches[0]
    missing = [k for k in ["branchId", "name", "isActive"] if k not in b]
    if missing:
        mark("branch_tool", "incomplete", f"Branch missing fields: {missing}", "warning")
else:
    mark("branch_tool list", "empty", "No branches (ok)")

# D2: Swipe list
print("\nD2: swipe_tool list")  
resp = client.call("swipe_tool", {"action": "list", "sessionId": session_id})
data = client.get_json(resp) or {}
swipes = data.get("swipes", [])
mark("swipe_tool list", "ok", f"Found {len(swipes)} swipes")

# ============================================================
# TEST SUITE E: Search
# ============================================================
print("\n" + "=" * 60)
print("E. SEARCH")
print("=" * 60)

print("\nE1: search_tool global")
resp = client.call("search_tool", {"action": "global", "query": "conversation"})
data = client.get_json(resp) or {}
hits = data.get("hits", 0)
mode = data.get("mode", "unknown")
status = data.get("status", "unknown")
mark("search_tool global", "ok", f"{hits} hits, mode={mode}, status={status[:40]}")

print("\nE2: search_tool session")
resp = client.call("search_tool", {"action": "session", "sessionId": session_id, "query": "hello"})
data = client.get_json(resp) or {}
hits = data.get("hits", 0)
mark("search_tool session", "ok", f"{hits} hits")

# ============================================================
# TEST SUITE F: Character Cards
# ============================================================
print("\n" + "=" * 60)
print("F. CHARACTER CARDS")
print("=" * 60)

print("\nF1: import_card with fixture")
fixture_path = os.path.expanduser("~/projects/ozone-rs/crates/ozone-mcp/tests/fixtures/screen-check-fixture.json")
if os.path.exists(fixture_path):
    resp = client.call("import_card", {"action": "card", "path": fixture_path})
    text = resp.get("result", {}).get("content", [{}])[0].get("text", "")
    if "error" in text.lower() or "fail" in text.lower():
        mark("import_card", "broken", text[:100], "bug")
    else:
        mark("import_card", "ok", text[:80])
else:
    mark("import_card", "skip", f"No fixture at {fixture_path}")

# ============================================================
# TEST SUITE G: Export
# ============================================================
print("\n" + "=" * 60)
print("G. EXPORT")
print("=" * 60)

print("\nG1: export_tool session")
resp = client.call("export_tool", {"action": "session", "sessionId": session_id})
text = resp.get("result", {}).get("content", [{}])[0].get("text", "")
if "error" in text.lower():
    mark("export_tool session", "broken", text[:100], "bug")
else:
    # Check for leaked tokens
    if "</antThinking>" in text:
        mark("export_tool session", "leak", "Export contains </antThinking>!", "bug")
    else:  
        mark("export_tool session", "ok", "Clean export")

# ============================================================
# TEST SUITE H: System State
# ============================================================
print("\n" + "=" * 60)
print("H. SYSTEM STATE")
print("=" * 60)

print("\nH1: workspace_status")
resp = client.call("workspace_status", {})
data = client.get_json(resp) or {}
prefs_path = data.get("defaultPaths", {}).get("prefsPath", "N/A")
models_dir = data.get("defaultPaths", {}).get("modelsDir", "N/A")
mark("workspace_status", "ok", f"prefs={prefs_path}, models={models_dir}")

print("\nH2: preferences_get")
resp = client.call("preferences_get", {})
data = client.get_json(resp) or {}
parsed = data.get("parsed", {})
backend = parsed.get("preferred_backend", "not set")
frontend = parsed.get("preferred_frontend", "not set")
print(f"  Backend: {backend}, Frontend: {frontend}")
mark("preferences_get", "ok", f"backend={backend}, frontend={frontend}")

print("\nH3: catalog_list")
resp = client.call("catalog_list", {"modelDir": ""})
data = client.get_json(resp) or {}
models = data.get("models", [])
broken = [m for m in models if m.get("isBrokenSymlink")]
print(f"  Models: {len(models)}, Broken symlinks: {len(broken)}")
if broken:
    for b in broken[:3]:
        print(f"    Broken: {b['name']}")

client.cleanup()

# ============================================================
# CLI VERIFICATION
# ============================================================
print("\n" + "=" * 60)
print("I. CLI VERIFICATION (ozone-plus commands)")
print("=" * 60)

cmd_tests = [
    ("create", ["./target/release/ozone-plus", "create", "CLI-Verify-Session"]),
    ("list", ["./target/release/ozone-plus", "list", "--json"]),
    ("transcript", ["./target/release/ozone-plus", "transcript", "00000000-0000-4001-98af-69defe9ac153"]),
    ("memory-list", ["./target/release/ozone-plus", "memory", "list", "00000000-0000-4001-98af-69defe9ac153"]),
    ("branch-list", ["./target/release/ozone-plus", "branch", "list", "00000000-0000-4001-98af-69defe9ac153"]),
    ("search-global", ["./target/release/ozone-plus", "search", "global", "test"]),
]

for name, cmd in cmd_tests:
    print(f"\nI.{name}:", " ".join(cmd[:3]) + "...")
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=30,
                           cwd=os.path.expanduser("~/projects/ozone-rs"))
    if result.returncode != 0:
        mark(f"cli {name}", "fail", result.stderr[:100], "bug")
    elif "</antThinking>" in result.stdout or "</antThinking>" in result.stderr:
        mark(f"cli {name}", "leak", "antThinking token in output!", "bug")
    else:
        lines = result.stdout.strip().split("\n")
        mark(f"cli {name}", "ok", f"exit={result.returncode}, {len(lines)} lines output")

# ============================================================
# FINAL REPORT
# ============================================================
print("\n" + "=" * 60)
print("FINAL REPORT")
print("=" * 60)

print(f"\nBugs found: {len(bugs)}")
for b in bugs:
    print(f"  [BUG] {b['tool']}: {b['detail']}")

print(f"\nWarnings: {len(warnings)}")
for w in warnings:
    print(f"  [WARN] {w['tool']}: {w['detail']}")

print(f"\nPassed: {len(passes)}")

bug_tools = set(b['tool'] for b in bugs)
print(f"\nUnique tools/features with issues: {len(bug_tools)}")
print(f"Unique tools with warnings: {len(set(w['tool'] for w in warnings))}")

# Save detailed report
report = {
    "bugs": bugs,
    "warnings": warnings,
    "passes": passes,
    "summary": {
        "total_tests": len(bugs) + len(warnings) + len(passes),
        "bugs": len(bugs),
        "warnings": len(warnings),
        "passes": len(passes)
    }
}

report_path = os.path.expanduser("~/projects/ozone-rs/plans/mcp-test-report.json")
with open(report_path, "w") as f:
    json.dump(report, f, indent=2)
print(f"\nDetailed report saved to: {report_path}")
