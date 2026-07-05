"""Score persistence — build, merge, save, and trigger leaderboard update.

Single responsibility: manage score files in results/ozone_scores/.
No eval orchestration or server lifecycle here.
"""

import json
import os
import subprocess
import sys
from pathlib import Path

from constants import SCORE_PERCENTILE_FACTOR


class ScoreFile:
    """A single model's score file on disk, with merge-on-save semantics."""

    def __init__(self, model_name: str, scores_dir: Path):
        self._model_name = model_name
        self._scores_dir = scores_dir
        self._scores_dir.mkdir(parents=True, exist_ok=True)
        self._path = scores_dir / f"{model_name}.json"

    # ── Public API ──────────────────────────────────────────────────────────

    @property
    def path(self) -> Path:
        return self._path

    def load_existing(self) -> dict:
        """Return existing scores dict, or empty dict if file missing/corrupt."""
        if not self._path.exists():
            return {}
        try:
            with open(self._path) as f:
                return json.load(f).get("scores", {})
        except Exception:
            return {}

    def save(self, new_scores: dict, elapsed: float, metadata: dict | None = None) -> dict:
        """Merge new_scores into existing, write to disk, return final summary.

        Preserves any scores from previous runs that aren't in new_scores.
        Accumulates elapsed_seconds across runs.
        """
        existing_scores = {}
        prev_elapsed = 0.0

        if self._path.exists():
            try:
                with open(self._path) as f:
                    prev = json.load(f)
                    existing_scores = prev.get("scores", {})
                    prev_elapsed = prev.get("elapsed_seconds", 0.0)
            except Exception:
                pass

        merged_scores = {**existing_scores, **new_scores}

        summary = {
            "model": self._model_name,
            "gguf_path": (metadata or {}).get("gguf_path", ""),
            "limit": (metadata or {}).get("limit", 50),
            "thinking": (metadata or {}).get("thinking", "N/A"),
            "elapsed_seconds": round(prev_elapsed + elapsed, 1),
            "scores": merged_scores,
        }

        with open(self._path, "w") as f:
            json.dump(summary, f, indent=2)
        print(f"\n  Scores saved to {self._path}", file=sys.stderr)

        # Auto-regenerate leaderboard
        _try_refresh_leaderboard(self._scores_dir.parent.parent)

        return summary


# ── Helpers ──────────────────────────────────────────────────────────────────

def extract_scores(results_all: dict, presets: dict) -> dict[str, float]:
    """Flatten lm-eval results dict into {preset.metric: score_percent}.

    Only includes metrics with float values in [0, 1] range.
    """
    flat = {}
    for preset_name, r in results_all.items():
        task_name = presets.get(preset_name, preset_name)
        for tname, metrics in r.items():
            for mname, val in metrics.items():
                if isinstance(val, (int, float)) and 0 <= val <= 1:
                    key = f"{preset_name}.{mname}"
                    flat[key] = round(val * SCORE_PERCENTILE_FACTOR, 1)
    return flat


def _try_refresh_leaderboard(project_root: Path | str) -> None:
    """Call generate_leaderboard.py if it exists, ignore failures."""
    script = (
        Path(project_root)
        / "contrib" / "evals" / "scripts" / "generate_leaderboard.py"
    )
    if not script.exists():
        return
    result = subprocess.run(
        [sys.executable, str(script)],
        capture_output=True, text=True,
        cwd=str(project_root),
    )
    if result.returncode == 0:
        print("  Leaderboard updated", file=sys.stderr)
    else:
        print(f"  Leaderboard update failed: {result.stderr.strip()[-120:]}",
              file=sys.stderr)
