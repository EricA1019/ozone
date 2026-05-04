#!/usr/bin/env python3
"""Comprehensive MCP tool tester for ozone-mcp server."""

import subprocess
import json
import os
import fcntl
import time
import select
import sys

BINARY = "/home/eric/projects/ozone-rs/target/debug/ozone-mcp"
passed = 0
failed = 0
errors = []

class McpProc:
    def __init__(self):
        self.proc = subprocess.Popen(
            [BINARY],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            bufsize=0,
        )
        self._id = 0
        self.all_stderr = b""

    def _nonblock_read(self, fd, timeout):
        """Read all available bytes from fd within timeout."""
        flags = fcntl.fcntl(fd, fcntl.F_GETFL)
        fcntl.fcntl(fd, fcntl.F_SETFL, flags | os.O_NONBLOCK)
        start = time.time()
        total = b""
        while time.time() - start < timeout:
            try:
                chunk = os.read(fd, 131072)
                if not chunk:
                    if time.time() - start > 0.1:
                        break
                    time.sleep(0.02)
                    continue
                total += chunk
            except BlockingIOError:
                time.sleep(0.02)
            except OSError:
                break
        fcntl.fcntl(fd, fcntl.F_SETFL, flags)
        return total

    def _drain_stderr(self):
        data = self._nonblock_read(self.proc.stderr.fileno(), 0.3)
        if data:
            self.all_stderr += data
        return data

    def send(self, method, params=None):
        self._id += 1
        rid = self._id
        msg = {"jsonrpc": "2.0", "id": rid, "method": method}
        if params is not None:
            msg["params"] = params
        body = json.dumps(msg).encode("utf-8")
        header = (f"Content-Length: {len(body)}\r\n\r\n").encode("utf-8")
        try:
            written = os.write(self.proc.stdin.fileno(), header + body)
        except Exception as e:
            print(f"  WRITE ERROR: {e}")
            return None
        return rid

    def read_response(self, timeout=30):
        self._drain_stderr()
        raw = self._nonblock_read(self.proc.stdout.fileno(), timeout)
        err = self._drain_stderr()
        msgs = self._parse(raw)
        return msgs, len(raw)

    def _parse(self, raw):
        messages = []
        pos = 0
        while pos < len(raw):
            crlf = raw.find(b"\r\n\r\n", pos)
            if crlf == -1:
                break
            header = raw[pos:crlf].decode("utf-8", errors="replace")
            try:
                cl = int(header.split(":")[1].strip())
            except:
                break
            bs = crlf + 4
            if bs + cl > len(raw):
                break
            body = raw[bs:bs + cl].decode("utf-8", errors="replace")
            try:
                msg = json.loads(body)
            except json.JSONDecodeError:
                msg = {"parse_error": True, "raw": body[:200]}
            messages.append(msg)
            pos = bs + cl
        return messages

    def call(self, name, args=None):
        rid = self.send("tools/call", {"name": name, "arguments": args or {}})
        return self.read_response()

    def close(self):
        try:
            self.proc.stdin.close()
        except:
            pass
        self.proc.kill()
        self.proc.wait(timeout=5)


def t(client, name, method, params=None, timeout=30):
    """Run a test step and track results."""
    global passed, failed, errors
    client.send(method, params)
    msgs, raw_len = client.read_response(timeout=timeout)
    print(f"\n{'='*70}")
    print(f"TEST: {name}")
    print(f"{'='*70}")

    if msgs is None:
        print(f"  STATUS: FAILED - no response from server")
        failed += 1
        errors.append(name)
        return []

    print(f"  RECEIVED {len(msgs)} message(s), {raw_len} raw bytes")

    for m in msgs:
        if "error" in m:
            err = m["error"]
            print(f"  [JSONRPC ERROR] code={err.get('code')}, message={err.get('message')}")
            failed += 1
            errors.append(name)
            continue

        result = m.get("result", {})
        is_err = result.get("isError", False)
        content = result.get("content", [])
        struct = result.get("structuredContent", {})
        data = struct.get("data", {})

        if is_err:
            print(f"  STATUS: TOOL ERROR")
            for c in content:
                print(f"    TEXT: {c.get('text', '')[:600]}")
            if data:
                print(f"    DATA: {json.dumps(data)[:500]}")
            failed += 1
            errors.append(name)
        else:
            print(f"  STATUS: SUCCESS")
            for c in content:
                txt = c.get("text", "")
                if len(txt) > 600:
                    print(f"    TEXT: {txt[:600]}... [total {len(txt)} chars]")
                else:
                    print(f"    TEXT: {txt}")
            if data:
                s = json.dumps(data)
                if len(s) > 600:
                    print(f"    DATA: {s[:600]}... [total {len(s)} chars]")
                else:
                    print(f"    DATA: {s}")
            passed += 1

    return msgs


def run_tests():
    global passed, failed, errors

    client = McpProc()
    print(f"Started MCP server PID={client.proc.pid}")

    # 1. Initialize
    t(client, "01. Initialize", "initialize", {
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "clientInfo": {"name": "test", "version": "1.0"}
    })

    # 2. Initialized notification
    client.send("notifications/initialized")

    # 3. Tools list
    t(client, "02. tools/list", "tools/list")

    # 4. workspace_status
    t(client, "03. workspace_status", "tools/call", {
        "name": "workspace_status", "arguments": {}
    })

    # 5. catalog_list (global)
    t(client, "04. catalog_list (global)", "tools/call", {
        "name": "catalog_list", "arguments": {}
    })

    # 6. preferences_get (global)
    t(client, "05. preferences_get (global)", "tools/call", {
        "name": "preferences_get", "arguments": {}
    })

    # 7. Create sandbox
    msgs, _ = t(client, "06. sandbox_tool create", "tools/call", {
        "name": "sandbox_tool",
        "arguments": {
            "action": "create",
            "namePrefix": "test-full",
            "models": ["test-model.gguf"],
            "preferences": {"backendUrl": "http://localhost:5001", "modelName": "test-model.gguf"}
        }
    })
    sandbox_id = None
    if msgs and msgs[0].get("result"):
        data = msgs[0]["result"].get("structuredContent", {}).get("data", {})
        sandbox_id = data.get("sandboxId")
        print(f"\n  *** CAPTURED sandbox_id: {sandbox_id} ***")

    if not sandbox_id:
        print("\n  FATAL: Could not create sandbox. Aborting.")
        client.close()
        return

    # 8. catalog_list (sandboxed)
    t(client, "07. catalog_list (sandboxed)", "tools/call", {
        "name": "catalog_list",
        "arguments": {"sandboxId": sandbox_id}
    })

    # 9. preferences_get (sandboxed)
    t(client, "08. preferences_get (sandboxed)", "tools/call", {
        "name": "preferences_get",
        "arguments": {"sandboxId": sandbox_id}
    })

    # =============================================================
    # SESSION MANAGEMENT - CRITICAL
    # =============================================================

    # 10. List sessions (should be empty)
    t(client, "09. session_tool list (empty)", "tools/call", {
        "name": "session_tool",
        "arguments": {"action": "list", "sandboxId": sandbox_id}
    })

    # 11. Create session 1
    msgs, _ = t(client, "10. session_tool create (Session 1)", "tools/call", {
        "name": "session_tool",
        "arguments": {
            "action": "create",
            "name": "Test Session 1",
            "characterName": "TestChar",
            "tags": ["test", "automation"],
            "sandboxId": sandbox_id
        }
    })
    sid1 = None
    if msgs and msgs[0].get("result"):
        data = msgs[0]["result"].get("structuredContent", {}).get("data", {})
        sid1 = data.get("session", {}).get("sessionId")
        print(f"\n  *** CAPTURED session_id_1: {sid1} ***")

    # 12. Create session 2
    msgs, _ = t(client, "11. session_tool create (Session 2)", "tools/call", {
        "name": "session_tool",
        "arguments": {
            "action": "create",
            "name": "Test Session 2",
            "sandboxId": sandbox_id
        }
    })
    sid2 = None
    if msgs and msgs[0].get("result"):
        data = msgs[0]["result"].get("structuredContent", {}).get("data", {})
        sid2 = data.get("session", {}).get("sessionId")
        print(f"\n  *** CAPTURED session_id_2: {sid2} ***")

    # 13. Create session 3
    msgs, _ = t(client, "12. session_tool create (Session 3)", "tools/call", {
        "name": "session_tool",
        "arguments": {
            "action": "create",
            "name": "Test Session 3",
            "tags": ["extra"],
            "sandboxId": sandbox_id
        }
    })
    sid3 = None
    if msgs and msgs[0].get("result"):
        data = msgs[0]["result"].get("structuredContent", {}).get("data", {})
        sid3 = data.get("session", {}).get("sessionId")
        print(f"\n  *** CAPTURED session_id_3: {sid3} ***")

    # 14. List sessions (should show 3)
    msgs, _ = t(client, "13. session_tool list (should show 3)", "tools/call", {
        "name": "session_tool",
        "arguments": {"action": "list", "sandboxId": sandbox_id}
    })
    session_count = None
    if msgs and msgs[0].get("result"):
        data = msgs[0]["result"].get("structuredContent", {}).get("data", {})
        session_count = data.get("found")
        print(f"\n  *** LISTED SESSION COUNT: {session_count} ***")

    # 15. Session metadata
    if sid1:
        t(client, "14. session_tool metadata (session 1)", "tools/call", {
            "name": "session_tool",
            "arguments": {"action": "metadata", "sessionId": sid1, "sandboxId": sandbox_id}
        })

    # 16. Session transcript
    if sid1:
        t(client, "15. session_tool transcript (session 1)", "tools/call", {
            "name": "session_tool",
            "arguments": {"action": "transcript", "sessionId": sid1, "sandboxId": sandbox_id}
        })

    # 17. Session "load" action - implemented but NOT in schema
    if sid1:
        t(client, "16. session_tool load (HIDDEN - missing from schema)", "tools/call", {
            "name": "session_tool",
            "arguments": {"action": "load", "sessionId": sid1, "sandboxId": sandbox_id}
        })

    # 18. Session "rename" action - implemented but NOT in schema
    if sid1:
        t(client, "17. session_tool rename (HIDDEN action)", "tools/call", {
            "name": "session_tool",
            "arguments": {
                "action": "rename",
                "sessionId": sid1,
                "newName": "Renamed Session 1",
                "sandboxId": sandbox_id
            }
        })

    # 19. Session "delete" action - implemented but NOT in schema
    if sid3:
        t(client, "18. session_tool delete (HIDDEN action)", "tools/call", {
            "name": "session_tool",
            "arguments": {"action": "delete", "sessionId": sid3, "sandboxId": sandbox_id}
        })

    # 20. List sessions (should show 2)
    t(client, "19. session_tool list (after delete, should show 2)", "tools/call", {
        "name": "session_tool",
        "arguments": {"action": "list", "sandboxId": sandbox_id}
    })

    # 21. Session "open" action - NOT implemented (should error)
    if sid1:
        t(client, "20. session_tool open (unsupported action)", "tools/call", {
            "name": "session_tool",
            "arguments": {"action": "open", "sessionId": sid1, "sandboxId": sandbox_id}
        })

    # 22. Non-existent sandbox
    t(client, "21. session_tool list (non-existent sandbox)", "tools/call", {
        "name": "session_tool",
        "arguments": {"action": "list", "sandboxId": "nonexistent-sandbox-id"}
    })

    # 23. Missing required field
    t(client, "22. session_tool create (missing name)", "tools/call", {
        "name": "session_tool",
        "arguments": {"action": "create", "sandboxId": sandbox_id}
    })

    # 24. Invalid JSON sessionId format
    if sid1:
        t(client, "23. session_tool metadata (invalid sessionId)", "tools/call", {
            "name": "session_tool",
            "arguments": {"action": "metadata", "sessionId": "not-a-valid-uuid", "sandboxId": sandbox_id}
        })

    # 25. Non-existent sessionId
    t(client, "24. session_tool metadata (non-existent sessionId)", "tools/call", {
        "name": "session_tool",
        "arguments": {"action": "metadata", "sessionId": "00000000-0000-0000-0000-000000000000", "sandboxId": sandbox_id}
    })

    # 26. message_tool (send to session)
    if sid1:
        t(client, "25. message_tool send", "tools/call", {
            "name": "message_tool",
            "arguments": {
                "action": "send",
                "sessionId": sid1,
                "content": "Hello from MCP test",
                "sandboxId": sandbox_id
            }
        })

    # 27. memory_tool note
    if sid1:
        t(client, "26. memory_tool note", "tools/call", {
            "name": "memory_tool",
            "arguments": {
                "action": "note",
                "sessionId": sid1,
                "content": "Test memory note",
                "sandboxId": sandbox_id
            }
        })

    # 28. memory_tool list
    if sid1:
        t(client, "27. memory_tool list", "tools/call", {
            "name": "memory_tool",
            "arguments": {"action": "list", "sessionId": sid1, "sandboxId": sandbox_id}
        })

    # 29. search_tool global
    t(client, "28. search_tool global", "tools/call", {
        "name": "search_tool",
        "arguments": {"action": "global", "query": "test", "sandboxId": sandbox_id}
    })

    # 30. branch_tool list
    if sid1:
        t(client, "29. branch_tool list", "tools/call", {
            "name": "branch_tool",
            "arguments": {"action": "list", "sessionId": sid1, "sandboxId": sandbox_id}
        })

    # 31. branch_tool create
    if sid1:
        msgs_31, _ = t(client, "30. branch_tool create", "tools/call", {
            "name": "branch_tool",
            "arguments": {
                "action": "create",
                "sessionId": sid1,
                "name": "Test Branch",
                "sandboxId": sandbox_id
            }
        })

    # 32. swipe_tool list
    if sid1:
        t(client, "31. swipe_tool list", "tools/call", {
            "name": "swipe_tool",
            "arguments": {"action": "list", "sessionId": sid1, "sandboxId": sandbox_id}
        })

    # 33. export_tool
    if sid1:
        t(client, "32. export_tool session", "tools/call", {
            "name": "export_tool",
            "arguments": {
                "action": "session",
                "sessionId": sid1,
                "format": "json",
                "sandboxId": sandbox_id
            }
        })

    # 34. import_card from JSON string
    card = json.dumps({"name": "TestImport", "description": "Test description", "personality": "Test"})
    t(client, "33. import_card (from JSON)", "tools/call", {
        "name": "import_card",
        "arguments": {
            "cardJson": card,
            "sessionName": "Imported Session",
            "sandboxId": sandbox_id
        }
    })

    # 35. Unknown tool
    t(client, "34. unknown_tool (error handling)", "tools/call", {
        "name": "nonexistent_tool_xyz"
    })

    # 36. Mock backend
    t(client, "35. mock_backend_tool start", "tools/call", {
        "name": "mock_backend_tool",
        "arguments": {"action": "start", "sandboxId": sandbox_id, "port": 5999}
    })

    # 37. Stop mock backend
    t(client, "36. mock_backend_tool stop", "tools/call", {
        "name": "mock_backend_tool",
        "arguments": {"action": "stop", "sandboxId": sandbox_id}
    })

    # 38. screen_nav_targets
    t(client, "37. screen_nav_targets", "tools/call", {
        "name": "screen_nav_targets",
        "arguments": {}
    })

    # 39. screen_check_tool (no artifact - should error)
    t(client, "38. screen_check_tool (no artifact)", "tools/call", {
        "name": "screen_check_tool",
        "arguments": {"checks": [{"type": "text_present", "text": "test"}]}
    })

    # 40. Destroy sandbox
    t(client, "39. sandbox_tool destroy", "tools/call", {
        "name": "sandbox_tool",
        "arguments": {"action": "destroy", "sandboxId": sandbox_id}
    })

    # 41. Ping
    t(client, "40. ping", "ping")

    client.close()
    print(f"\nMCP server exited. stderr: {client.all_stderr.decode('utf-8', errors='replace')[:1000]}")


    # =============================================
    # CLI TESTS
    # =============================================
    print(f"\n{'='*70}")
    print("CLI TESTS: ozone-plus")
    print(f"{'='*70}")

    cli_tests = [
        ("01. ozone-plus list", ["/home/eric/projects/ozone-rs/target/debug/ozone-plus", "list"], 60),
        ("02. ozone-plus create", ["/home/eric/projects/ozone-rs/target/debug/ozone-plus", "create", "CLI Test Session"], 60),
        ("03. ozone-plus list (after create)", ["/home/eric/projects/ozone-rs/target/debug/ozone-plus", "list"], 60),
        ("04. ozone-plus version", ["/home/eric/projects/ozone-rs/target/debug/ozone-plus", "--version"], 10),
    ]

    cli_session_id = None
    for name, cmd, timeout_sec in cli_tests:
        print(f"\n  Running: {' '.join(cmd)}")
        try:
            r = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout_sec)
            print(f"  EXIT: {r.returncode}")
            print(f"  STDOUT: {r.stdout[:800] if r.stdout else '(empty)'}")
            print(f"  STDERR: {r.stderr[:800] if r.stderr else '(empty)'}")
            if r.returncode == 0:
                passed += 1
            else:
                failed += 1
                errors.append(f"CLI: {name}")
        except subprocess.TimeoutExpired:
            print(f"  TIMEOUT after {timeout_sec}s")
            failed += 1
            errors.append(f"CLI: {name} (timeout)")

    # =============================================
    # SCHEMA ANALYSIS
    # =============================================
    print(f"\n{'='*70}")
    print("SCHEMA ANALYSIS")
    print(f"{'='*70}")

    src_path = "/home/eric/projects/ozone-rs/crates/ozone-mcp/src/lib.rs"
    with open(src_path) as f:
        source = f.read()

    # Find all tool names in tool_definitions
    import re
    tool_names = re.findall(r'name: "(\w+)"', source)
    print(f"\n  All tool names in source: {[t for t in tool_names if t not in ['tools_list_result', 'workspace_status']]}")

    # Find session_tool handler actions
    session_section = source[source.find("fn session_tool"):]
    session_section = session_section[:session_section.find("fn message_tool")]
    implemented_actions = [m for m in re.findall(r'"(\w+)" =>\s', session_section)]
    print(f"  session_tool IMPLEMENTED actions: {implemented_actions}")

    # Find session_tool schema
    session_schema = source[source.find('"name": "session_tool"'):]
    session_schema = session_schema[:session_schema.find('"name": "message_tool"')]
    enum_match = re.search(r'"enum": \[([^\]]+)\]', session_schema)
    if enum_match:
        declared_actions = [a.strip().strip('"') for a in enum_match.group(1).split(",")]
        print(f"  session_tool DECLARED actions: {declared_actions}")
        missing = set(implemented_actions) - set(declared_actions)
        extra = set(declared_actions) - set(implemented_actions)
        if missing:
            print(f"  *** BUG: Implemented but NOT in schema: {missing}")
        if extra:
            print(f"  *** Note: In schema but NOT implemented: {extra}")

    # Find session_tool input_schema
    print(f"\n  Checking session_tool 'open' action existence:")
    if '"open"' in session_section:
        print("    'open' IS implemented (found in match arm)")
    else:
        print("    'open' is NOT implemented")

    # Final summary
    print(f"\n{'='*70}")
    print("FINAL SUMMARY")
    print(f"{'='*70}")
    print(f"  PASSED: {passed}")
    print(f"  FAILED: {failed}")
    if errors:
        print(f"\n  FAILED TESTS:")
        for e in errors:
            print(f"    - {e}")


if __name__ == "__main__":
    run_tests()
