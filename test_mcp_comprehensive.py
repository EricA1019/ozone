#!/usr/bin/env python3
"""Comprehensive MCP tool tester for ozone-mcp server."""

import subprocess
import json
import sys
import os
import time

MCP_BINARY = "/home/eric/projects/ozone-rs/target/debug/ozone-mcp"

class McpClient:
    def __init__(self, binary_path):
        self.proc = subprocess.Popen(
            [binary_path],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        self.id_counter = 0
        self.stderr_data = ""

    def send_request(self, method, params=None):
        self.id_counter += 1
        msg_id = self.id_counter
        request = {
            "jsonrpc": "2.0",
            "id": msg_id,
            "method": method,
        }
        if params is not None:
            request["params"] = params
        body = json.dumps(request)
        header = f"Content-Length: {len(body)}\r\n\r\n"
        raw = (header + body).encode("utf-8")
        
        # Drain stderr
        self._drain_stderr()
        
        self.proc.stdin.write(raw)
        self.proc.stdin.flush()
        
        response = self._read_response()
        if response is None:
            return {"error": "NO_RESPONSE", "id": msg_id}
        return self._parse_response(response, msg_id)

    def _drain_stderr(self):
        # Non-blocking read of stderr buffer
        try:
            # Check if there's data to read
            import select
            if self.proc.stderr.fileno() != -1:
                # Read any available stderr data non-blocking
                import fcntl
                flags = fcntl.fcntl(self.proc.stderr.fileno(), fcntl.F_GETFL)
                fcntl.fcntl(self.proc.stderr.fileno(), fcntl.F_SETFL, flags | os.O_NONBLOCK)
                try:
                    data = self.proc.stderr.read()
                    if data:
                        self.stderr_data += data.decode("utf-8", errors="replace")
                except:
                    pass
                # Restore blocking mode
                fcntl.fcntl(self.proc.stderr.fileno(), fcntl.F_SETFL, flags)
        except:
            pass

    def _read_response(self):
        # Read Content-Length header
        header_data = b""
        while True:
            byte = self.proc.stdout.read(1)
            if not byte:
                return None
            header_data += byte
            if header_data.endswith(b"\r\n\r\n"):
                break
        
        header_text = header_data.decode("utf-8")
        content_length = int(header_text.split(":")[1].strip())
        
        # Read body
        body = b""
        while len(body) < content_length:
            chunk = self.proc.stdout.read(content_length - len(body))
            if not chunk:
                return None
            body += chunk
        
        return body.decode("utf-8")

    def _parse_response(self, raw, expected_id):
        try:
            return json.loads(raw)
        except json.JSONDecodeError:
            return {"error": "INVALID_JSON", "raw": raw}

    def call_tool(self, tool_name, arguments=None):
        return self.send_request("tools/call", {
            "name": tool_name,
            "arguments": arguments or {}
        })

    def close(self):
        try:
            self.proc.stdin.close()
        except:
            pass
        self.proc.wait(timeout=10)

    def get_stderr(self):
        return self.stderr_data


def print_result(title, response):
    """Pretty-print a test result."""
    print(f"\n{'='*80}")
    print(f"TEST: {title}")
    print(f"{'='*80}")
    
    if response is None:
        print("  STATUS: NO RESPONSE RECEIVED")
        return False
    
    if "error" in response:
        print(f"  STATUS: CLIENT ERROR - {response['error']}")
        return False
    
    result = response.get("result", {})
    is_error = result.get("isError", False)
    
    if is_error:
        print("  STATUS: TOOL ERROR")
    else:
        print("  STATUS: SUCCESS")
    
    # Print summary
    content = result.get("content", [])
    for item in content:
        if item.get("type") == "text":
            text = item["text"]
            # Print first 500 chars
            if len(text) > 500:
                print(text[:500] + "\n  ... [truncated]")
            else:
                print(text)
    
    # Print structured content
    struct = result.get("structuredContent", {})
    if struct:
        data = struct.get("data", {})
        if data:
            print(f"\n  STRUCTURED DATA:")
            print(f"    {json.dumps(data, indent=6)[:800]}")
    
    return not is_error


def main():
    results = {"passed": 0, "failed": 0, "errors": []}
    
    def check(title, response):
        ok = print_result(title, response)
        if ok:
            results["passed"] += 1
        else:
            results["failed"] += 1
            results["errors"].append(title)
        time.sleep(0.1)  # Small delay between requests
        return response

    client = McpClient(MCP_BINARY)

    print("Testing ozone-mcp server at", MCP_BINARY)
    print(f"Process PID: {client.proc.pid}")

    # 1. Initialize
    check("01. Initialize", client.send_request("initialize", {
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "clientInfo": {"name": "test-client", "version": "1.0"}
    }))

    # Send initialized notification
    client.send_request("notifications/initialized", None)

    # 2. Tools list
    resp = check("02. Tools/List", client.send_request("tools/list"))
    if resp and "result" in resp:
        tools = resp["result"].get("tools", [])
        print(f"\n  FOUND {len(tools)} tools:")
        for t in tools:
            print(f"    - {t['name']}")

    # 3. Workspace status
    resp = check("03. workspace_status", client.call_tool("workspace_status"))

    # 4. Catalog list
    resp = check("04. catalog_list", client.call_tool("catalog_list"))

    # 5. Preferences get
    resp = check("05. preferences_get", client.call_tool("preferences_get"))

    # =========================================================
    # SANDBOX-TESTED TOOLS BELOW
    # =========================================================

    # 6. Create sandbox
    resp = check("06. sandbox_tool (create)", client.call_tool("sandbox_tool", {
        "action": "create",
        "namePrefix": "test-comprehensive",
        "models": ["test-model.gguf"],
        "preferences": {
            "backendUrl": "http://localhost:5001",
            "modelName": "test-model.gguf"
        }
    }))
    
    sandbox_id = None
    if resp and "result" in resp:
        struct = resp["result"].get("structuredContent", {}).get("data", {})
        sandbox_id = struct.get("sandboxId")
        print(f"\n  SANDBOX_ID: {sandbox_id}")

    if not sandbox_id:
        print("\n  FATAL: Could not get sandbox ID. Skipping remaining sandbox tests.")
        client.close()
        print(f"\n{'='*80}")
        print(f"SUMMARY: {results['passed']} passed, {results['failed']} failed")
        if results["errors"]:
            print(f"FAILED TESTS: {', '.join(results['errors'])}")
        print(f"STDERR:\n{client.get_stderr()[:3000]}")
        return

    # 7. Catalog list (sandboxed)
    resp = check(f"07. catalog_list (sandbox={sandbox_id})", client.call_tool("catalog_list", {
        "sandboxId": sandbox_id
    }))

    # 8. Preferences get (sandboxed)
    resp = check(f"08. preferences_get (sandbox={sandbox_id})", client.call_tool("preferences_get", {
        "sandboxId": sandbox_id
    }))

    # =========================================================
    # SESSION MANAGEMENT - CRITICAL TESTS
    # =========================================================

    # 9. List sessions (should be empty)
    resp = check(f"09. session_tool list (empty)", client.call_tool("session_tool", {
        "action": "list",
        "sandboxId": sandbox_id
    }))

    # 10. Create session 1
    resp = check(f"10. session_tool create (Test Session 1)", client.call_tool("session_tool", {
        "action": "create",
        "name": "Test Session 1",
        "characterName": "TestChar",
        "tags": ["test", "automation"],
        "sandboxId": sandbox_id
    }))
    session_id_1 = None
    if resp and "result" in resp:
        struct = resp["result"].get("structuredContent", {}).get("data", {})
        session_info = struct.get("session", {})
        session_id_1 = session_info.get("sessionId")
        print(f"\n  SESSION_ID_1: {session_id_1}")

    # 11. Create session 2
    resp = check(f"11. session_tool create (Test Session 2)", client.call_tool("session_tool", {
        "action": "create",
        "name": "Test Session 2",
        "sandboxId": sandbox_id
    }))
    session_id_2 = None
    if resp and "result" in resp:
        struct = resp["result"].get("structuredContent", {}).get("data", {})
        session_info = struct.get("session", {})
        session_id_2 = session_info.get("sessionId")
        print(f"\n  SESSION_ID_2: {session_id_2}")

    # 12. Create session 3
    resp = check(f"12. session_tool create (Test Session 3)", client.call_tool("session_tool", {
        "action": "create",
        "name": "Test Session 3",
        "tags": ["extra"],
        "sandboxId": sandbox_id
    }))
    session_id_3 = None
    if resp and "result" in resp:
        struct = resp["result"].get("structuredContent", {}).get("data", {})
        session_info = struct.get("session", {})
        session_id_3 = session_info.get("sessionId")
        print(f"\n  SESSION_ID_3: {session_id_3}")

    # 13. List sessions (should show 3)
    resp = check(f"13. session_tool list (should show 3)", client.call_tool("session_tool", {
        "action": "list",
        "sandboxId": sandbox_id
    }))

    # 14. Session metadata
    if session_id_1:
        resp = check(f"14. session_tool metadata (session 1)", client.call_tool("session_tool", {
            "action": "metadata",
            "sessionId": session_id_1,
            "sandboxId": sandbox_id
        }))

    # 15. Session transcript (empty, new session)
    if session_id_1:
        resp = check(f"15. session_tool transcript (session 1)", client.call_tool("session_tool", {
            "action": "transcript",
            "sessionId": session_id_1,
            "sandboxId": sandbox_id
        }))

    # 16. Test "load" action (implemented but missing from schema!)
    if session_id_1:
        resp = check(f"16. session_tool load (HIDDEN action - missing from schema)", client.call_tool("session_tool", {
            "action": "load",
            "sessionId": session_id_1,
            "sandboxId": sandbox_id
        }))

    # 17. Test "rename" action (implemented but missing from schema!)
    if session_id_1:
        resp = check(f"17. session_tool rename (HIDDEN action)", client.call_tool("session_tool", {
            "action": "rename",
            "sessionId": session_id_1,
            "newName": "Renamed Session 1",
            "sandboxId": sandbox_id
        }))

    # 18. Test "delete" action (implemented but missing from schema!)
    if session_id_3:
        resp = check(f"18. session_tool delete (HIDDEN action)", client.call_tool("session_tool", {
            "action": "delete",
            "sessionId": session_id_3,
            "sandboxId": sandbox_id
        }))

    # 19. List sessions again (should show 2 after delete)
    resp = check(f"19. session_tool list (after delete, should show 2)", client.call_tool("session_tool", {
        "action": "list",
        "sandboxId": sandbox_id
    }))

    # 20. Test unsupported session action
    resp = check(f"20. session_tool 'open' (UNSUPPORTED - check error handling)", client.call_tool("session_tool", {
        "action": "open",
        "sessionId": session_id_1 or "fake",
        "sandboxId": sandbox_id
    }))

    # 21. Test non-existent sandbox
    resp = check(f"21. session_tool list (non-existent sandbox)", client.call_tool("session_tool", {
        "action": "list",
        "sandboxId": "nonexistent-sandbox"
    }))

    # 22. Test with missing required field
    resp = check(f"22. session_tool create (missing name)", client.call_tool("session_tool", {
        "action": "create",
        "sandboxId": sandbox_id
    }))

    # =========================================================
    # BRANCH OPERATIONS
    # =========================================================
    
    if session_id_1:
        # 23. Branch list (empty session)
        resp = check(f"23. branch_tool list (session 1)", client.call_tool("branch_tool", {
            "action": "list",
            "sessionId": session_id_1,
            "sandboxId": sandbox_id
        }))

        # 24. Branch create
        resp = check(f"24. branch_tool create (session 1)", client.call_tool("branch_tool", {
            "action": "create",
            "sessionId": session_id_1,
            "name": "Branch Alpha",
            "sandboxId": sandbox_id
        }))
        branch_id = None
        if resp and "result" in resp:
            struct = resp["result"].get("structuredContent", {}).get("data", {})
            branch_info = struct.get("branch", {})
            branch_id = branch_info.get("branchId")
            print(f"\n  BRANCH_ID: {branch_id}")

    # 25. Memory tool - create note
    if session_id_1:
        resp = check(f"25. memory_tool note (session 1)", client.call_tool("memory_tool", {
            "action": "note",
            "sessionId": session_id_1,
            "content": "Test memory note created by automation",
            "sandboxId": sandbox_id
        }))

    # 26. Memory tool - list
    if session_id_1:
        resp = check(f"26. memory_tool list (session 1)", client.call_tool("memory_tool", {
            "action": "list",
            "sessionId": session_id_1,
            "sandboxId": sandbox_id
        }))

    # 27. Search tool - empty index
    resp = check(f"27. search_tool global (empty)", client.call_tool("search_tool", {
        "action": "global",
        "query": "test",
        "sandboxId": sandbox_id
    }))

    # 28. Export tool
    if session_id_1:
        resp = check(f"28. export_tool session (session 1, JSON)", client.call_tool("export_tool", {
            "action": "session",
            "sessionId": session_id_1,
            "format": "json",
            "sandboxId": sandbox_id
        }))

    # 29. Import card (create test card JSON)
    test_card = json.dumps({
        "name": "TestCharacter",
        "description": "A test character for automation",
        "personality": "Friendly and helpful"
    })
    resp = check(f"29. import_card (from JSON string)", client.call_tool("import_card", {
        "cardJson": test_card,
        "sessionName": "Imported Session",
        "sandboxId": sandbox_id
    }))

    # 30. Swipe tool
    if session_id_1:
        resp = check(f"30. swipe_tool list (session 1)", client.call_tool("swipe_tool", {
            "action": "list",
            "sessionId": session_id_1,
            "sandboxId": sandbox_id
        }))

    # 31. Unknown tool
    resp = check(f"31. unknown_tool (error handling)", client.call_tool("nonexistent_tool"))

    # 32. Mock backend - start
    resp = check(f"32. mock_backend_tool start", client.call_tool("mock_backend_tool", {
        "action": "start",
        "sandboxId": sandbox_id,
        "port": 5999
    }))

    # 33. Mock backend - stop
    resp = check(f"33. mock_backend_tool stop", client.call_tool("mock_backend_tool", {
        "action": "stop",
        "sandboxId": sandbox_id
    }))

    # 34. Screen nav targets
    resp = check(f"34. screen_nav_targets", client.call_tool("screen_nav_targets"))

    # 35. Destroy sandbox
    resp = check(f"35. sandbox_tool (destroy)", client.call_tool("sandbox_tool", {
        "action": "destroy",
        "sandboxId": sandbox_id
    }))

    # 36. Ping
    resp = check(f"36. ping", client.send_request("ping"))

    client.close()

    # =============================================
    # CLI TESTS
    # =============================================
    
    print(f"\n{'='*80}")
    print("CLI TESTS: ozone-plus")
    print(f"{'='*80}")

    # CLI: list sessions
    print("\n  Running: ozone-plus list")
    result = subprocess.run(
        ["/home/eric/projects/ozone-rs/target/debug/ozone-plus", "list"],
        capture_output=True, text=True, timeout=60
    )
    print(f"  EXIT CODE: {result.returncode}")
    print(f"  STDOUT: {result.stdout[:1000] if result.stdout else '(empty)'}")
    print(f"  STDERR: {result.stderr[:1000] if result.stderr else '(empty)'}")
    if result.returncode != 0:
        results["failed"] += 1
        results["errors"].append("CLI: ozone-plus list")
    else:
        results["passed"] += 1

    # CLI: create session  
    print("\n  Running: ozone-plus create 'CLI Test Session'")
    result = subprocess.run(
        ["/home/eric/projects/ozone-rs/target/debug/ozone-plus", "create", "CLI Test Session"],
        capture_output=True, text=True, timeout=60
    )
    print(f"  EXIT CODE: {result.returncode}")
    print(f"  STDOUT: {result.stdout[:1000] if result.stdout else '(empty)'}")
    print(f"  STDERR: {result.stderr[:1000] if result.stderr else '(empty)'}")
    cli_session_id = None
    if result.returncode == 0:
        results["passed"] += 1
        # Try to extract session ID
        for line in result.stdout.split("\n"):
            if "id:" in line.lower() or "session" in line.lower():
                print(f"  LINE: {line.strip()}")
    else:
        results["failed"] += 1
        results["errors"].append("CLI: ozone-plus create")

    # CLI: list again
    print("\n  Running: ozone-plus list (after create)")
    result = subprocess.run(
        ["/home/eric/projects/ozone-rs/target/debug/ozone-plus", "list"],
        capture_output=True, text=True, timeout=60
    )
    print(f"  EXIT CODE: {result.returncode}")
    print(f"  STDOUT: {result.stdout[:1000] if result.stdout else '(empty)'}")
    print(f"  STDERR: {result.stderr[:1000] if result.stderr else '(empty)'}")
    if result.returncode != 0:
        results["failed"] += 1
        results["errors"].append("CLI: ozone-plus list (after create)")
    else:
        results["passed"] += 1

    # CLI: delete session (if we got one)
    if cli_session_id:
        print(f"\n  Running: ozone-plus delete {cli_session_id}")
        result = subprocess.run(
            ["/home/eric/projects/ozone-rs/target/debug/ozone-plus", "delete", cli_session_id],
            capture_output=True, text=True, timeout=60
        )
        print(f"  EXIT CODE: {result.returncode}")
        print(f"  STDOUT: {result.stdout[:500] if result.stdout else '(empty)'}")
        print(f"  STDERR: {result.stderr[:500] if result.stderr else '(empty)'}")

    # Clean up CLI test sessions
    print("\n  Running: ozone-plus delete (clean up CLI test sessions)")
    result = subprocess.run(
        ["/home/eric/projects/ozone-rs/target/debug/ozone-plus", "list", "--json"],
        capture_output=True, text=True, timeout=60
    )
    if result.returncode == 0 and result.stdout:
        try:
            sessions = json.loads(result.stdout)
            for s in sessions:
                sname = s.get("name", "")
                if "CLI Test" in sname or "Renamed Session" in sname:
                    sid = s.get("id") or s.get("sessionId")
                    if sid:
                        print(f"    Deleting: {sid} ({sname})")
                        subprocess.run(
                            ["/home/eric/projects/ozone-rs/target/debug/ozone-plus", "delete", sid],
                            capture_output=True, text=True, timeout=60
                        )
        except:
            pass

    # =============================================
    # SCREEN_CHECK_TOOL TEST WITH FIXTURES
    # =============================================

    print(f"\n{'='*80}")
    print("SCREEN_CHECK_TOOL TESTS")
    print(f"{'='*80}")

    client2 = McpClient(MCP_BINARY)
    
    # Initialize
    client2.send_request("initialize", {
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "clientInfo": {"name": "test-client", "version": "1.0"}
    })
    
    # Create fresh sandbox
    resp = client2.call_tool("sandbox_tool", {
        "action": "create",
        "namePrefix": "screen-test"
    })
    sandbox2 = None
    if resp and "result" in resp:
        struct = resp["result"].get("structuredContent", {}).get("data", {})
        sandbox2 = struct.get("sandboxId")

    # Look for test fixtures
    fixture_search = subprocess.run(
        "find /home/eric/projects/ozone-rs -name '*.png' -o -name '*screenshot*' -o -name '*baseline*' | head -20",
        shell=True, capture_output=True, text=True
    )
    print(f"\n  Found fixtures:\n{fixture_search.stdout[:500]}")

    # Run screen_check_tool with a test assertion (no artifact, should error)
    check(f"37. screen_check_tool (no artifact path)", client2.call_tool("screen_check_tool", {
        "checks": [
            {"type": "text_present", "text": "test"}
        ]
    }))

    # 38. screen_check_tool with non-existent artifact
    check(f"38. screen_check_tool (non-existent path)", client2.call_tool("screen_check_tool", {
        "artifactPath": "/tmp/nonexistent-12345.json",
        "checks": [
            {"type": "text_present", "text": "test"}
        ]
    }))

    # Clean up sandbox 2
    if sandbox2:
        check(f"39. sandbox_tool (destroy sandbox2)", client2.call_tool("sandbox_tool", {
            "action": "destroy",
            "sandboxId": sandbox2
        }))

    client2.close()

    # =============================================
    # SESSION TOOL SCHEMA ANALYSIS
    # =============================================

    print(f"\n{'='*80}")
    print("SCHEMA ANALYSIS: Checking for implementation/schema mismatches")
    print(f"{'='*80}")

    # Read the source to find actions implemented vs declared
    src_path = "/home/eric/projects/ozone-rs/crates/ozone-mcp/src/lib.rs"
    with open(src_path) as f:
        source = f.read()

    # Find session_tool schema enum
    import re
    
    # Check session_tool schema
    session_schema_match = re.search(r'"action": \{ "type": "string", "enum": \[([^\]]+)\] \}', source)
    if session_schema_match:
        schema_actions = [a.strip().strip('"') for a in session_schema_match.group(1).split(",")]
        print(f"\n  session_tool DECLARED actions: {schema_actions}")
    
    # Find all session_tool match arms  
    handler_match = re.search(r'fn session_tool.*?fn \w+_\w+\(', source, re.DOTALL)
    if handler_match:
        handler_src = handler_match.group(0)
        implemented_actions = re.findall(r'"([^"]+)" =>\s', handler_src)
        print(f"  session_tool IMPLEMENTED actions: {implemented_actions}")
        
        # Find mismatch
        declared_set = set(schema_actions) if session_schema_match else set()
        implemented_set = set(implemented_actions)
        missing_from_schema = implemented_set - declared_set
        missing_from_impl = declared_set - implemented_set
        if missing_from_schema:
            print(f"  BUG: Actions implemented but NOT in schema: {missing_from_schema}")
        if missing_from_impl:
            print(f"  BUG: Actions in schema but NOT implemented: {missing_from_impl}")

    # =============================================
    # FINAL SUMMARY
    # =============================================

    print(f"\n{'='*80}")
    print("FINAL SUMMARY")
    print(f"{'='*80}")
    print(f"  Passed: {results['passed']}")
    print(f"  Failed: {results['failed']}")
    if results["errors"]:
        print(f"  Failed tests: {', '.join(results['errors'])}")
    print(f"\n  STDERR from MCP server:\n{client.get_stderr()[:2000]}")

if __name__ == "__main__":
    main()
