"""Tests for scores.py — score extraction, merging, and persistence."""

import json
import tempfile
from pathlib import Path

import pytest

from scores import ScoreFile, extract_scores


# ── Fixtures ──────────────────────────────────────────────────────────────────

@pytest.fixture
def tmp_scores_dir(tmp_path: Path) -> Path:
    d = tmp_path / "ozone_scores"
    d.mkdir()
    return d


# ── extract_scores ────────────────────────────────────────────────────────────

class TestExtractScores:
    """Converting lm-eval raw results to flat score dict."""

    def test_extracts_percentiles(self):
        raw = {"mmlu": {"mmlu": {"acc,none": 0.35}}}
        presets = {"mmlu": "mmlu"}
        result = extract_scores(raw, presets)
        assert result["mmlu.acc,none"] == 35.0

    def test_skips_non_float(self):
        raw = {"mmlu": {"mmlu": {"acc,none": "N/A"}}}
        result = extract_scores(raw, {"mmlu": "mmlu"})
        assert result == {}

    def test_skips_out_of_range(self):
        raw = {"mmlu": {"mmlu": {"acc,none": 999.0}}}
        result = extract_scores(raw, {"mmlu": "mmlu"})
        assert result == {}

    def test_multiple_metrics(self):
        raw = {
            "gsm8k": {"gsm8k": {
                "exact_match,strict-match": 0.78,
                "exact_match,flexible-extract": 0.78,
            }}
        }
        result = extract_scores(raw, {"gsm8k": "gsm8k"})
        assert len(result) == 2
        assert result["gsm8k.exact_match,strict-match"] == 78.0


# ── ScoreFile ─────────────────────────────────────────────────────────────────

class TestScoreFile:
    """Read, merge, write cycle."""

    def test_save_creates_file(self, tmp_scores_dir: Path):
        sf = ScoreFile("test-model", tmp_scores_dir)
        sf.save({"preset.metric": 50.0}, elapsed=10.0)
        assert sf.path.exists()

    def test_load_existing_empty_when_missing(self, tmp_scores_dir: Path):
        sf = ScoreFile("missing", tmp_scores_dir)
        assert sf.load_existing() == {}

    def test_merge_preserves_existing(self, tmp_scores_dir: Path):
        sf = ScoreFile("merge-test", tmp_scores_dir)
        # First save
        sf.save({"a.m1": 50.0}, elapsed=5.0)
        # Second save with different key
        sf.save({"b.m2": 75.0}, elapsed=3.0)
        # Both should be present
        loaded = sf.load_existing()
        assert loaded["a.m1"] == 50.0
        assert loaded["b.m2"] == 75.0

    def test_elapsed_accumulates(self, tmp_scores_dir: Path):
        sf = ScoreFile("elapsed-test", tmp_scores_dir)
        summary1 = sf.save({"a.m1": 50.0}, elapsed=10.0)
        summary2 = sf.save({"b.m2": 75.0}, elapsed=20.0)
        # Total elapsed should be ~30 (previous + new)
        assert summary2["elapsed_seconds"] > 29.0
        assert summary2["elapsed_seconds"] < 31.0

    def test_later_value_overwrites(self, tmp_scores_dir: Path):
        sf = ScoreFile("overwrite-test", tmp_scores_dir)
        sf.save({"x.val": 50.0}, elapsed=1.0)
        sf.save({"x.val": 80.0}, elapsed=1.0)
        loaded = sf.load_existing()
        assert loaded["x.val"] == 80.0

    def test_save_output_structure(self, tmp_scores_dir: Path):
        sf = ScoreFile("struct-test", tmp_scores_dir)
        metadata = {"gguf_path": "/models/test.gguf", "limit": 50}
        summary = sf.save({"a.val": 30.0}, elapsed=5.0, metadata=metadata)
        assert summary["model"] == "struct-test"
        assert summary["gguf_path"] == "/models/test.gguf"
        assert summary["scores"]["a.val"] == 30.0

    def test_corrupt_file_treated_as_empty(self, tmp_scores_dir: Path):
        sf = ScoreFile("corrupt", tmp_scores_dir)
        # Write invalid JSON
        sf.path.write_text("not-json{")
        assert sf.load_existing() == {}
