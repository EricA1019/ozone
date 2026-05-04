#!/usr/bin/env python3
"""MCP tool tester for ozone-rs project"""
import subprocess
import json
import os
import sys

os.chdir(os.path.expanduser("~/projects/ozone-rs"))

# Build the binary first
print("Building ozone-mcp...")
subprocess.run(["cargo", "build", "-p", "ozone-mcp-app"], capture_output=True)
print("Done.\n")

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
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {"protocolVersion": "2024-11-05", "capabilities": {},
                       "clientInfo": {"name": "test", "version": "0.1"}}
        })
        self._read()
        # notifications/initialized
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
            print(f"  [parse error] raw bytes: {body[:200]}")
            return None

    def call(self, name, args=None):
        self._id += 1
        self._send_raw({
            "jsonrpc": "2.0", "id": self._id, "method": "tools/call",
            "params": {"name": name, "arguments": args or {}}
        })
        return self._read()

    def get_text(self, resp):
        if not resp or "result" not in resp:
            return "NO RESPONSE"
        content = resp.get("result", {}).get("content", [])
        texts = [c.get("text", "") for c in content if isinstance(c, dict)]
        return "\n".join(texts) if texts else "EMPTY"

    def cleanup(self):
        self.proc.kill()

client = MCPClient()
client.initialize()

def test(name, tool_name, args, checks=None):
    print(f"  [TEST] {name}")
    resp = client.call(tool_name, args)
    text = client.get_text(resp)
    short = text[:400] if len(text) > 400 else text
    print(f"    {short[:200]}")
    print(f"    isError: {resp.get('result', {}).get('isError', False)}")
    if checks:
        passed = False
        try:
            passed = checks(text)
        except Exception as e:
            print(f"    CHECK ERROR: {e}")
        if not passed:
            print(f"    [FAIL] check didn't pass")
    print()
    return text

results = []
all_ok = True

try:
    # 1. workspace_status
    t = test("workspace_status", "workspace_status", {})
    results.append(("workspace_status", "pass" if "repoRoot" in t else "fail"))
    
    # 2. catalog_list
    t = test("catalog_list (empty)", "catalog_list", {"modelDir": ""})
    results.append(("catalog_list", "pass" if "model" in t.lower() else "partial"))
    
    # 3. preferences_get
    t = test("preferences_get", "preferences_get", {})
    results.append(("preferences_get", "pass" if "pref" in t.lower() or "side" in t.lower() else "fail"))
    
    # 4. session_tool list
    t = test("session_tool list", "session_tool", {"action": "list"})
    session_count = t.count("id:")
    print(f"    -> {session_count} sessions found in response")
    results.append(("session_tool_list", "pass" if session_count > 0 else "fail"))
    
    # 5. session_tool get on existing session
    t = test("session_tool get existing", "session_tool", {"action": "get", "sessionId": "00000000-0000-4001-98af-69defe9ac153"})
    results.append(("session_tool_get", "pass" if "error" not in t.lower() else "fail"))
    
    # 6. session_tool create
    t = test("session_tool create", "session_tool", {"action": "create", "name": "MCP-Auto-Test"})
    has_id = "id:" in t or "Id:" in t
    results.append(("session_tool_create", "pass" if has_id else "fail"))
    
    # 7. session_tool list again - should have new session
    t = test("session_tool list (after create)", "session_tool", {"action": "list"})
    results.append(("session_tool_list_after_create", "pass" if "MCP-Auto-Test" in t else "partial"))
    
    # 8. session_tool open
    t = test("session_tool open", "session_tool", 
             {"action": "open", "sessionId": "00000000-0000-4001-98af-69defe9ac153"})
    results.append(("session_tool_open", "pass" if "opened" in t.lower() or "session" in t.lower() else "partial"))
    
    # 9. message_tool list
    t = test("message_tool list", "message_tool", {"action": "list", "sessionId": "00000000-0000-4001-98af-69defe9ac153"})
    results.append(("message_tool_list", "pass" if "error" not in t.lower() else "partial"))
    
    # 10. memory_tool list
    t = test("memory_tool list", "memory_tool", 
             {"action": "list", "sessionId": "00000000-0000-4001-98af-69defe9ac153"})
    results.append(("memory_tool_list", "pass" if "pin" in t.lower() or "memo" in t.lower() or "empty" in t.lower() else "partial"))
    
    # 11. memory_tool pin
    t = test("memory_tool pin", "memory_tool", 
             {"action": "pin", "sessionId": "00000000-0000-4001-98af-69defe9ac153", 
              "text": "MCP test pinned memory"})
    results.append(("memory_tool_pin", "pass" if "pin" in t.lower() or "success" in t.lower() or "error" not in t.lower() else "fail"))
    
    # 12. search_tool global
    t = test("search_tool global", "search_tool", {"action": "global", "query": "test"})
    results.append(("search_tool_global", "pass" if "result" in t.lower() or "found" in t.lower() or "empty" in t.lower() else "partial"))
    
    # 13. search_tool session
    t = test("search_tool session", "search_tool", 
             {"action": "session", "sessionId": "00000000-0000-4001-98af-69defe9ac153", "query": "test"})
    results.append(("search_tool_session", "pass" if "error" not in t.lower() or "empty" in t.lower() else "fail"))
    
    # 14. branch_tool list
    t = test("branch_tool list", "branch_tool", 
             {"action": "list", "sessionId": "00000000-0000-4001-98af-69defe9ac153"})
    results.append(("branch_tool_list", "pass" if "error" not in t.lower() else "fail"))
    
    # 15. swipe_tool list
    t = test("swipe_tool list", "swipe_tool", 
             {"action": "list", "sessionId": "00000000-0000-4001-98af-69defe9ac153"})
    results.append(("swipe_tool_list", "pass" if "error" not in t.lower() else "fail"))
    
    # 16. export_tool
    t = test("export_tool session JSON", "export_tool", 
             {"action": "session", "sessionId": "00000000-0000-4001-98af-69defe9ac153"})
    results.append(("export_tool", "pass" if "export" in t.lower() or "error" not in t.lower() else "partial"))
    
    # 17. import_card
    t = test("import_card (no file)", "import_card", {"action": "card"})
    results.append(("import_card_missing_file", "pass" if "error" in t.lower() or "required" in t.lower() else "partial"))

except Exception as e:
    print(f"  [EXCEPTION] {e}")
    all_ok = False

finally:
    client.cleanup()

print("=" * 60)
print("RESULTS:")
for name, status in results:
    marker = "OK" if status == "pass" else "!!" if status == "fail" else "??"
    print(f"  [{marker}] {name}: {status}")

total = len(results)
passed = sum(1 for _, s in results if s == "pass")
failed = sum(1 for _, s in results if s == "fail")
partial = sum(1 for _, s in results if s == "partial")
print(f"\nTotal: {total} | Passed: {passed} | Failed: {failed} | Partial: {partial}")
