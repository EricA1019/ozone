"""Manage a llama.cpp HTTP server lifecycle for generate-task evals.

Single responsibility: start, check, and kill a llama-server process.
No eval logic here — just process management.
"""

import os
import subprocess
import time
from pathlib import Path

import requests

from constants import (
    DEFAULT_SERVER_PORT,
    HEALTH_CHECK_TIMEOUT,
    SERVER_READY_RETRIES,
    SERVER_READY_DELAY,
    DEFAULT_GPU_LAYERS,
    DEFAULT_CTX_SIZE,
)


def server_port(base_url: str) -> int:
    """Extract port from a base URL like http://127.0.0.1:8989."""
    from urllib.parse import urlparse
    return urlparse(base_url).port or DEFAULT_SERVER_PORT


def is_running(base_url: str) -> bool:
    """Return True if a server is responding at base_url."""
    try:
        resp = requests.get(f"{base_url}/health", timeout=HEALTH_CHECK_TIMEOUT)
        return resp.status_code == 200
    except Exception:
        return False


def kill(base_url: str) -> None:
    """Kill any process listening on the server port."""
    port = server_port(base_url)
    print(f"  Killing any process on port {port}...", file=__import__('sys').stderr)
    subprocess.run(
        ["fuser", "-k", f"{port}/tcp"],
        capture_output=True, timeout=5,
    )
    time.sleep(2)
    if is_running(base_url):
        print("  WARNING: server still alive after kill attempt",
              file=__import__('sys').stderr)


def start(
    gguf_path: str,
    base_url: str,
    ctx_size: int = DEFAULT_CTX_SIZE,
    gpu_layers: int = DEFAULT_GPU_LAYERS,
) -> bool:
    """Start llama-server for the given model. Returns True on success."""
    port = server_port(base_url)
    server_dir = Path(os.environ.get(
        "LLAMA_CPP_DIR",
        str(Path.home() / "servers/llama.cpp-cuda-latest/install/bin"),
    ))
    server_bin = server_dir / "llama-server"
    lib_dir = server_dir.parent / "build" / "bin"

    # When gpu_layers == -1 use 99 (all layers); llama.cpp clips to actual count
    effective_layers = gpu_layers if gpu_layers >= 0 else 99

    cmd = [
        str(server_bin),
        "--model", gguf_path,
        "--n-gpu-layers", str(effective_layers),
        "--flash-attn", "on",
        "--parallel", "1",
        "--ctx-size", str(ctx_size),
        "--host", "127.0.0.1",
        "--port", str(port),
        "--log-disable",
    ]
    env = os.environ.copy()
    env["LD_LIBRARY_PATH"] = str(lib_dir)

    print(f"  Starting server on port {port}...", file=__import__('sys').stderr)
    subprocess.Popen(cmd, env=env, stdout=subprocess.DEVNULL,
                     stderr=subprocess.DEVNULL)

    for _ in range(SERVER_READY_RETRIES):
        time.sleep(SERVER_READY_DELAY)
        if is_running(base_url):
            print(f"  Server ready on port {port}", file=__import__('sys').stderr)
            return True

    print(f"  Server failed to start on port {port}", file=__import__('sys').stderr)
    return False
