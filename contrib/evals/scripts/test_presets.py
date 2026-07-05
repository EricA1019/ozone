"""Tests for presets.py — data definitions and resolution logic."""

from presets import (
    PRESETS, SUITES, SWEEPS, LOGPROB_TASKS,
    resolve_presets, expand_suite, expand_sweep, is_logprob_task,
)


class TestPresetsData:
    """Data integrity: every preset appears in at least one suite."""

    def test_all_presets_in_at_least_one_suite(self):
        """Every PRESETS key should be in at least one SUITES list."""
        in_suites = {p for suite in SUITES.values() for p in suite}
        for key in PRESETS:
            assert key in in_suites, f"{key} not found in any suite"

    def test_all_suite_members_are_valid_presets(self):
        """Every entry in every suite must be a known PRESETS key."""
        for suite_name, members in SUITES.items():
            for m in members:
                assert m in PRESETS, f"{m} in suite {suite_name} but not in PRESETS"

    def test_sweep_members_are_valid_suites(self):
        """Every entry in every sweep must be a known SUITES key."""
        for sweep_name, suite_names in SWEEPS.items():
            for s in suite_names:
                assert s in SUITES, f"{s} in sweep {sweep_name} but not in SUITES"

    def test_all_presets_have_non_empty_task(self):
        """Every PRESETS value must be a non-empty string."""
        for key, val in PRESETS.items():
            assert val and isinstance(val, str), f"{key} has invalid task name: {val!r}"

    def test_logprob_tasks_are_valid_substrings(self):
        """Every LOGPROB_TASKS entry must match at least one PRESETS task name."""
        for log_key in LOGPROB_TASKS:
            matches = [p for p in PRESETS.values() if log_key in p]
            assert matches, f"{log_key} does not match any PRESETS task name"


class TestPresetsResolution:
    """Resolution logic — resolve_presets, expand_suite, expand_sweep."""

    def test_resolve_short_name(self):
        assert resolve_presets(["mmlu"]) == ["mmlu"]

    def test_resolve_long_name(self):
        assert resolve_presets(["hendrycks_math"]) == ["math"]

    def test_resolve_multiple(self):
        result = resolve_presets(["mmlu", "hellaswag"])
        assert result == ["mmlu", "hellaswag"]

    def test_resolve_unknown_raises(self):
        try:
            resolve_presets(["nonexistent_task"])
            assert False, "Should have raised ValueError"
        except ValueError:
            pass

    def test_expand_suite_baseline(self):
        members = expand_suite("baseline")
        assert "hellaswag" in members
        assert "arc_challenge" in members

    def test_expand_suite_unknown(self):
        try:
            expand_suite("made_up_suite")
            assert False, "Should have raised ValueError"
        except ValueError:
            pass

    def test_expand_sweep_full(self):
        """Full sweep should combine multiple suites."""
        presets = expand_sweep("full")
        assert "mmlu" in presets          # from general
        assert "gsm8k" in presets         # from math
        assert "mbpp" in presets          # from coding

    def test_expand_sweep_baseline(self):
        presets = expand_sweep("baseline")
        assert presets == expand_suite("baseline")

    def test_is_logprob_task_true(self):
        assert is_logprob_task("mmlu") is True
        assert is_logprob_task("hellaswag") is True

    def test_is_logprob_task_false(self):
        assert is_logprob_task("gsm8k") is False
        assert is_logprob_task("instruction") is False

    def test_is_logprob_task_by_task_name(self):
        """Indirect match via task name substring."""
        assert is_logprob_task("hendrycks_ethics") is True
