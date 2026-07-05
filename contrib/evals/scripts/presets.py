"""Eval preset definitions — single source of truth for tasks, suites, sweeps.

Imported by both ozone_eval.py and generate_leaderboard.py.
No runtime imports; pure data only.
"""

# Tasks that use direct loglikelihood scoring (load model directly via
# llama-cpp-python).  Substring match: if any key is found in the task name,
# the task is treated as a logprob task.
LOGPROB_TASKS: set[str] = {
    "mmlu",
    "mmlu_pro",
    "hellaswag",
    "bbh",
    "arc_challenge",
    "gpqa",
    "hendrycks_ethics",
}

# Short name → lm-eval task name
PRESETS: dict[str, str] = {
    # Baseline
    "mmlu": "mmlu",
    "hellaswag": "hellaswag",
    "bbh": "bbh",
    "gsm8k": "gsm8k",
    "math": "hendrycks_math",
    "instruction": "ifeval",
    "truthfulqa": "truthfulqa_gen",
    # General knowledge
    "mmlu_pro": "mmlu_pro",
    "arc_challenge": "arc_challenge",
    # Philosophy & ethics
    "mmlu_philosophy": "mmlu_philosophy",
    "hendrycks_ethics": "hendrycks_ethics",
    "bbh_formal_fallacies": "bbh_fewshot_formal_fallacies",
    "bbh_causal_judgement": "bbh_fewshot_causal_judgement",
    # Coding
    "mbpp": "mbpp",
    # Reading comprehension
    "drop": "drop",
    # Hard / graduate-level
    "gpqa": "gpqa_main_zeroshot",
}

# Reverse mapping: lm-eval task name → short name
TASK_TO_PRESET: dict[str, str] = {v: k for k, v in PRESETS.items()}

# ── Suites (named groups of presets) ──────────────────────────────────────────
SUITES: dict[str, list[str]] = {
    "baseline": [
        "hellaswag", "arc_challenge",
        "bbh_formal_fallacies", "bbh_causal_judgement",
    ],
    "general": ["mmlu", "mmlu_pro"],
    "philosophy-ethics": [
        "mmlu_philosophy", "hendrycks_ethics",
        "bbh_formal_fallacies", "bbh_causal_judgement",
    ],
    "reasoning": ["bbh", "drop"],
    "math": ["gsm8k", "math"],
    "coding": ["mbpp"],
    "safety": ["truthfulqa", "instruction"],
    "hard": ["gpqa"],
}

# ── Sweeps (groups of suites, run in order) ───────────────────────────────────
SWEEPS: dict[str, list[str]] = {
    "baseline": ["baseline"],
    "quick": ["general", "philosophy-ethics"],
    "full": [
        "general", "philosophy-ethics", "reasoning",
        "math", "coding", "safety",
    ],
    "code": ["coding", "math"],
    "all": [
        "general", "philosophy-ethics", "reasoning",
        "math", "coding", "safety", "hard",
    ],
}


def resolve_presets(preset_names: list[str]) -> list[str]:
    """Convert list of short names / lm-eval names to short names."""
    resolved = []
    for p in preset_names:
        if p in PRESETS:
            resolved.append(p)
        elif p in TASK_TO_PRESET:
            resolved.append(TASK_TO_PRESET[p])
        else:
            raise ValueError(f"Unknown preset/task: {p}")
    return resolved


def expand_suite(suite_name: str) -> list[str]:
    """Expand a suite name to its list of preset short names."""
    if suite_name not in SUITES:
        raise ValueError(f"Unknown suite: {suite_name} (choose from {list(SUITES.keys())})")
    return list(SUITES[suite_name])


def expand_sweep(sweep_name: str) -> list[str]:
    """Expand a sweep name to flat list of preset short names."""
    if sweep_name not in SWEEPS:
        raise ValueError(f"Unknown sweep: {sweep_name} (choose from {list(SWEEPS.keys())})")
    flat = []
    for s in SWEEPS[sweep_name]:
        flat.extend(SUITES[s])
    return flat


def is_logprob_task(preset_name: str) -> bool:
    """Return True if the preset uses direct loglikelihood scoring."""
    task_name = PRESETS.get(preset_name, preset_name)
    return any(k in task_name for k in LOGPROB_TASKS)
