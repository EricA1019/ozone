#!/usr/bin/env python3
"""Ozone evaluation runner — the single Python entry point for model evals.

Boundary: this file (plus extract_gguf_tokenizer.py) is ALL the Python ozone
needs. Rust owns: CLI, server lifecycle, result storage, comparison.

Accepts model path + task list from Rust via CLI args. For each task:
  - loglikelihood (MMLU, HellaSwag, BBH): loads model directly via
    llama-cpp-python with logits_all=True for correct per-token logprobs
  - generate (GSM8K, MATH, etc.): uses the running HTTP server (started by
    Rust) via lm-eval's local-completions backend

Output: JSON results to stdout, parseable by Rust.
"""

import argparse
import json
import os
import subprocess
import sys
import time
import requests
from pathlib import Path

import numpy as np

# ── Server lifecycle helpers ──────────────────────────────────────────────────

def _server_port(base_url: str) -> int:
    from urllib.parse import urlparse
    return urlparse(base_url).port or 8989


def _check_server(base_url: str) -> bool:
    """Return True if server is running at base_url."""
    try:
        resp = requests.get(f"{base_url}/health", timeout=3)
        return resp.status_code == 200
    except Exception:
        return False


def _kill_server(base_url: str):
    """Kill any process listening on the server port."""
    port = _server_port(base_url)
    print(f"  Killing any process on port {port}...", file=sys.stderr)
    subprocess.run(["fuser", "-k", f"{port}/tcp"],
                   capture_output=True, timeout=5)
    time.sleep(2)
    if _check_server(base_url):
        print("  WARNING: server still alive after kill attempt", file=sys.stderr)


def _start_server(gguf_path: str, base_url: str, ctx_size: int = 8192):
    """Start llama-server for the given model. Returns True on success."""
    port = _server_port(base_url)
    server_dir = Path(os.environ.get(
        "LLAMA_CPP_DIR",
        str(Path.home() / "servers/llama.cpp-cuda-latest/install/bin"),
    ))
    server_bin = server_dir / "llama-server"
    lib_dir = server_dir.parent / "build" / "bin"

    cmd = [
        str(server_bin),
        "--model", gguf_path,
        "--n-gpu-layers", "99",
        "--flash-attn", "on",
        "--parallel", "1",
        "--ctx-size", str(ctx_size),
        "--host", "127.0.0.1",
        "--port", str(port),
        "--log-disable",
    ]
    env = os.environ.copy()
    env["LD_LIBRARY_PATH"] = str(lib_dir)

    print(f"  Starting server on port {port}...", file=sys.stderr)
    subprocess.Popen(cmd, env=env, stdout=subprocess.DEVNULL,
                     stderr=subprocess.DEVNULL)

    for _ in range(30):
        time.sleep(1)
        if _check_server(base_url):
            print(f"  Server ready on port {port}", file=sys.stderr)
            return True
    print(f"  Server failed to start on port {port}", file=sys.stderr)
    return False


# ── Task definitions ──────────────────────────────────────────────────────────

# Tasks that need loglikelihood scoring (direct model loading)
# Substring match: any preset whose lm-eval task name contains one of these.
LOGPROB_TASKS = {
    "mmlu", "mmlu_pro", "hellaswag", "bbh", "leaderboard_bbh",
    "arc_challenge", "gpqa", "hendrycks_ethics",
}

# All supported presets (short name → lm-eval task name)
PRESETS = {
    # -- existing --
    "mmlu": "mmlu",
    "hellaswag": "hellaswag",
    "bbh": "bbh",
    "gsm8k": "gsm8k",
    "math": "hendrycks_math",
    "instruction": "ifeval",
    "truthfulqa": "truthfulqa_gen",
    # -- new: general knowledge --
    "mmlu_pro": "mmlu_pro",
    "arc_challenge": "arc_challenge",
    # -- new: philosophy & ethics --
    "mmlu_philosophy": "mmlu_philosophy",
    "hendrycks_ethics": "hendrycks_ethics",
    "bbh_formal_fallacies": "bbh_fewshot_formal_fallacies",
    "bbh_causal_judgement": "bbh_fewshot_causal_judgement",
    # -- new: coding --
    "mbpp": "mbpp",
    # -- new: reading comprehension --
    "drop": "drop",
    # -- new: hard (graduate-level, opt-in) --
    "gpqa": "gpqa_main_zeroshot",
}

# Reverse mapping so we can accept lm-eval task names directly
TASK_TO_PRESET = {v: k for k, v in PRESETS.items()}


# ── Suite / Sweep definitions ────────────────────────────────────────────────

# A suite is a named group of presets.  A sweep is a named group of suites.
# The CLI accepts --suite <name> and --sweep <name> as shortcuts for --presets.

SUITES = {
    "baseline":          ["hellaswag", "arc_challenge",
                          "bbh_formal_fallacies", "bbh_causal_judgement"],
    "general":           ["mmlu", "mmlu_pro"],
    "philosophy-ethics": ["mmlu_philosophy", "hendrycks_ethics",
                          "bbh_formal_fallacies", "bbh_causal_judgement"],
    "reasoning":         ["bbh", "drop"],
    "math":              ["gsm8k", "math"],
    "coding":            ["mbpp"],
    "safety":            ["truthfulqa", "instruction"],
    "hard":              ["gpqa"],
}

# A sweep is a group of suites run in order.
SWEEPS = {
    "baseline": ["baseline"],
    "quick":    ["general", "philosophy-ethics"],
    "full":     ["general", "philosophy-ethics", "reasoning",
                  "math", "coding", "safety"],
    "code":     ["coding", "math"],
    "all":      ["general", "philosophy-ethics", "reasoning",
                  "math", "coding", "safety", "hard"],
}


# ── compute_loglikelihood ─────────────────────────────────────────────────────

def compute_loglikelihood(llm, context: str, continuation: str):
    """P(continuation | context) from per-token logits."""
    ctx_ids = llm.tokenize(context.encode("utf-8"))
    cont_ids = llm.tokenize(continuation.encode("utf-8"))
    if cont_ids and ctx_ids and ctx_ids[0] == cont_ids[0]:
        cont_ids = cont_ids[1:]
    if not cont_ids:
        return (0.0, True)

    full_ids = ctx_ids + cont_ids
    llm.reset()
    llm.eval(full_ids)

    scores = llm._scores
    logprobs = llm.logits_to_logprobs(scores)
    ctx_len = len(ctx_ids)

    total = 0.0
    greedy = True
    for i, cid in enumerate(cont_ids):
        total += float(logprobs[ctx_len + i, cid])
        if int(scores[ctx_len + i].argmax()) != cid:
            greedy = False
    del scores, logprobs  # prevent VRAM leak over many samples
    return (total, greedy)


# ── LogprobsModel (correct loglikelihood via llama-cpp-python) ────────────────

def _register_logprobs_model():
    """Register gguf-logits model class with lm-eval registry (idempotent)."""
    from lm_eval.api.registry import MODEL_REGISTRY
    if "gguf-logits" in MODEL_REGISTRY:
        return
    from lm_eval.api.model import LM
    from lm_eval.api.registry import register_model

    @register_model("gguf-logits")
    class LogProbsModel(LM):
        def __init__(self, model=None, temperature=0.0, max_length=8192,
                     max_gen_toks=2048, batch_size=1, seed=1234,
                     gpu_layers=16, **kw):
            super().__init__()
            self._temp = temperature
            self._seed = seed
            self._max_gen = max_gen_toks
            self.max_length = max_length
            self._model_path = model
            self._gpu_layers = gpu_layers
            self._reload_interval = kw.get("reload_interval", 1000)
            self._sample_count = 0
            self._llm = None
            self._ensure_loaded()

        def _ensure_loaded(self):
            """Load or reload the model, clearing CUDA memory pool."""
            if self._llm is not None:
                del self._llm
                import gc; gc.collect()
            from llama_cpp import Llama
            self._llm = Llama(
                model_path=self._model_path, n_gpu_layers=self._gpu_layers,
                verbose=False, logits_all=True, n_ctx=self.max_length,
            )
            self._sample_count = 0

        def loglikelihood(self, requests, disable_tqdm=False):
            from tqdm import tqdm
            results = []
            for ctx, cont in tqdm([r.args for r in requests],
                                  disable=disable_tqdm):
                if self._sample_count >= self._reload_interval:
                    self._ensure_loaded()
                results.append(compute_loglikelihood(self._llm, ctx, cont))
                self._sample_count += 1
            return results

        def loglikelihood_rolling(self, reqs, disable_tqdm=False):
            raise NotImplementedError

        def generate_until(self, requests, disable_tqdm=False):
            from tqdm import tqdm
            res = []
            for r in tqdm(requests, disable=disable_tqdm):
                inp = r.args[0]
                args = r.args[1]
                until = args.get("until", ["</s>"])
                resp = self._llm.create_completion(
                    prompt=inp, stop=until, temperature=self._temp,
                    max_tokens=args.get("max_gen_toks", self._max_gen),
                )
                res.append(
                    resp["choices"][0].get("text", "").strip()
                    if resp and "choices" in resp else ""
                )
            return res


# ── Task runner ───────────────────────────────────────────────────────────────

def run_logprob_task(gguf_path: str, task: str, limit: int,
                     temperature: float, max_length: int,
                     gpu_layers: int) -> dict:
    """Run a loglikelihood task using direct llama-cpp-python loading."""
    _register_logprobs_model()
    # Create model instance directly via the registry
    import lm_eval
    from lm_eval import evaluator

    model_args = f"model={gguf_path},temperature={temperature},max_length={max_length},gpu_layers={gpu_layers}"
    model = lm_eval.api.registry.get_model("gguf-logits") \
        .create_from_arg_string(model_args)
    results = evaluator.simple_evaluate(
        model=model, tasks=[task], num_fewshot=None, batch_size=1, limit=limit,
    )
    return results


def run_generate_task(model_name: str, task: str, limit: int,
                      base_url: str, temperature: float) -> dict:
    """Run a generate_until task using the HTTP server."""
    import subprocess
    import requests

    # Verify server is alive before starting
    try:
        resp = requests.get(f"{base_url}/health", timeout=5)
        if resp.status_code != 200:
            print(f"  Server not healthy: {resp.status_code}", file=sys.stderr)
            return {}
    except requests.ConnectionError:
        print(f"  Server not reachable at {base_url}. Start it first.", file=sys.stderr)
        print(f"  Example: llama-server --model <model.gguf> --port 8989", file=sys.stderr)
        return {}

    lm_eval_bin = str(
        Path(__file__).resolve().parent.parent / ".venv" / "bin" / "lm-eval"
    )

    cmd = [
        lm_eval_bin, "run",
        "--model", "local-completions",
        "--model_args",
        f"model={model_name},base_url={base_url}/v1/completions,"
        f"tokenizer_backend=None,temperature={temperature}",
        "--tasks", task,
        "--limit", str(limit),
        "--output_path", "results/eval",
    ]
    env = os.environ.copy()
    env["OPENAI_API_KEY"] = "none"
    result = subprocess.run(cmd, env=env, capture_output=True, text=True)
    if result.returncode != 0:
        print(f"  Task {task} failed (exit {result.returncode})", file=sys.stderr)
        if result.stderr:
            print(f"  Last error: {result.stderr.strip().split(chr(10))[-1]}", file=sys.stderr)
        return {}


# ── Main entry ────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description="Ozone eval runner")
    parser.add_argument("model_or_path", help="Model name or path to GGUF")
    parser.add_argument("--presets", nargs="+",
                        default=["mmlu", "hellaswag", "bbh",
                                 "gsm8k", "math", "instruction", "truthfulqa"],
                        help="Presets or lm-eval task names (default: all)")
    parser.add_argument("--limit", type=int, default=50,
                        help="Samples per task")
    parser.add_argument("--base-url", default="http://127.0.0.1:8989",
                        help="HTTP server for generate tasks")
    parser.add_argument("--temperature", type=float, default=0.0)
    parser.add_argument("--max-length", type=int, default=8192)
    parser.add_argument("--skip-extract-tokenizer", action="store_true",
                        help="Skip auto tokenizer extraction")
    parser.add_argument("--thinking", type=str, default=None,
                        choices=["on", "off"],
                        help="Thinking mode for models that support it (on/off)")
    parser.add_argument("--gpu-layers", type=int, default=16,
                        help="GPU layers to offload for logprob tasks (0=CPU only)")
    parser.add_argument("--suite", type=str, default=None,
                        choices=list(SUITES.keys()),
                        help="Run a predefined suite of presets")
    parser.add_argument("--sweep", type=str, default=None,
                        choices=list(SWEEPS.keys()),
                        help="Run a predefined sweep (group of suites)")
    args = parser.parse_args()

    # Resolve model path: if it looks like a name, check ~/models/
    gguf_path = args.model_or_path
    if not os.path.exists(gguf_path):
        home_models = Path.home() / "models" / f"{gguf_path}.gguf"
        if home_models.exists():
            gguf_path = str(home_models)
        else:
            print(f"Error: model not found: {gguf_path} (also tried {home_models})",
                  file=sys.stderr)
            sys.exit(1)

    # Resolve presets: accept both short names & lm-eval task names
    # --suite / --sweep expand to --presets before this point
    resolved_presets = []
    for p in args.presets:
        if p in PRESETS:
            resolved_presets.append(p)
        elif p in TASK_TO_PRESET:
            resolved_presets.append(TASK_TO_PRESET[p])
        else:
            print(f"Unknown preset/task: {p}", file=sys.stderr)
            sys.exit(1)

    model_name = Path(gguf_path).stem
    results_all = {}
    start_total = time.time()

    # ── Expand --suite / --sweep into presets ──
    if args.sweep:
        suite_names = SWEEPS[args.sweep]
        flat_presets = []
        for s in suite_names:
            flat_presets.extend(SUITES[s])
        args.presets = flat_presets
    elif args.suite:
        args.presets = SUITES[args.suite]

    # Re-resolve after potential suite/sweep expansion
    resolved_presets = []
    for p in args.presets:
        if p in PRESETS:
            resolved_presets.append(p)
        elif p in TASK_TO_PRESET:
            resolved_presets.append(TASK_TO_PRESET[p])
        else:
            print(f"Unknown preset/task: {p} (from suite/sweep)", file=sys.stderr)
            sys.exit(1)

    # ── Split presets into logprob (direct GPU) and generate (HTTP server) ──
    logprob_presets = [p for p in resolved_presets
                       if any(k in PRESETS[p] for k in LOGPROB_TASKS)]
    gen_presets = [p for p in resolved_presets
                   if p not in logprob_presets]

    # ── Logprob tasks: MUST NOT have server running (VRAM contention) ──
    if logprob_presets:
        if _check_server(args.base_url):
            print("\n⚠ Server is running — killing it to free VRAM for logprob eval",
                  file=sys.stderr)
            _kill_server(args.base_url)

    for preset in logprob_presets:
        task = PRESETS[preset]
        print(f"\n{'='*60}", file=sys.stderr)
        print(f"  {preset} (loglikelihood)",
              file=sys.stderr)
        print(f"{'='*60}", file=sys.stderr)

        start = time.time()

        try:
            # Auto-extract tokenizer if needed
            tokenizer_dir = Path("results/tokenizers") / model_name
            if (not args.skip_extract_tokenizer and
                    not (tokenizer_dir / "tokenizer.json").exists()):
                print(f"  Extracting tokenizer...", file=sys.stderr)
                subprocess.run(
                    [sys.executable,
                     str(Path(__file__).parent / "extract_gguf_tokenizer.py"),
                     gguf_path, str(tokenizer_dir)],
                    check=True,
                )

            results = run_logprob_task(
                gguf_path, task, args.limit,
                args.temperature, args.max_length,
                args.gpu_layers,
            )

            elapsed = time.time() - start
            results_all[preset] = results.get("results", {})
            print(f"  Done ({elapsed:.0f}s)", file=sys.stderr)

        except Exception as e:
            elapsed = time.time() - start
            print(f"  FAILED after {elapsed:.0f}s: {e}", file=sys.stderr)
            results_all[preset] = {}  # empty results for this preset, continue

    # ── Generate tasks: MUST have server running ──
    if gen_presets:
        if not _check_server(args.base_url):
            print("\n  Starting HTTP server for generate tasks...",
                  file=sys.stderr)
            if not _start_server(gguf_path, args.base_url, args.max_length):
                print("  ABORT: cannot run generate tasks without server",
                      file=sys.stderr)
        else:
            print(f"\n  Using existing server at {args.base_url}",
                  file=sys.stderr)

    for preset in gen_presets:
        task = PRESETS[preset]
        print(f"\n{'='*60}", file=sys.stderr)
        print(f"  {preset} (generate)",
              file=sys.stderr)
        print(f"{'='*60}", file=sys.stderr)

        start = time.time()

        try:
            results = run_generate_task(
                model_name, task, args.limit,
                args.base_url, args.temperature,
            )

            elapsed = time.time() - start
            results_all[preset] = results.get("results", {})
            print(f"  Done ({elapsed:.0f}s)", file=sys.stderr)

        except Exception as e:
            elapsed = time.time() - start
            print(f"  FAILED after {elapsed:.0f}s: {e}", file=sys.stderr)
            results_all[preset] = {}

    total_elapsed = time.time() - start_total

    # Build summary
    summary = {
        "model": model_name,
        "gguf_path": gguf_path,
        "limit": args.limit,
        "thinking": args.thinking or "N/A",
        "elapsed_seconds": round(total_elapsed, 1),
        "scores": {},
    }

    for preset, r in results_all.items():
        task = PRESETS[preset]
        for tname, metrics in r.items():
            for mname, val in metrics.items():
                if isinstance(val, (int, float)) and 0 <= val <= 1:
                    key = f"{preset}.{mname}"
                    summary["scores"][key] = round(val * 100, 1)

    # Print summary to stdout (parseable by Rust)
    print(json.dumps(summary, indent=2))

    # Also save to disk for the HTML leaderboard
    scores_dir = Path("results/ozone_scores")
    scores_dir.mkdir(parents=True, exist_ok=True)
    score_file = scores_dir / f"{model_name}.json"
    with open(score_file, "w") as f:
        json.dump(summary, f, indent=2)
    print(f"\n  Scores saved to {score_file}", file=sys.stderr)


if __name__ == "__main__":
    main()
