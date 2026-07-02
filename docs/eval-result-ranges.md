# Eval Result Ranges

This note covers the shipped `ozone eval` probes and how to read their scores.

All of the metrics below are normalized fractions on a `0.0` to `1.0` scale. Higher is better.

| Probe | Metric(s) | Range | What it means |
| --- | --- | --- | --- |
| GSM8K (`lm-eval`, `gsm8k`) | `exact_match,strict-match` and `exact_match,flexible-extract` | `0.0` to `1.0` | Fraction of sampled math questions answered correctly after the selected answer-extraction filter. `1.0` means every scored sample was correct. |
| IFEval (instruction-following, `lm-eval`, `leaderboard_ifeval`) | `prompt_level_strict_acc`, `inst_level_strict_acc`, `prompt_level_loose_acc`, `inst_level_loose_acc` | `0.0` to `1.0` | `prompt_level_*` measures whether the whole prompt satisfied every instruction. `inst_level_*` averages the individual instruction checks. `strict` requires exact compliance; `loose` allows semantically acceptable variation. |
| Math hard (`lm-eval`, `leaderboard_math_hard`) | `exact_match` and `exact_match_original` | `0.0` to `1.0` | Fraction of parsed answers that match the reference solution. The grouped `leaderboard_math_hard` score is the mean across its seven subtasks. |
| HumanEval / EvalPlus (`evalplus.evaluate`) | `pass@1`, `pass@10`, `pass@100` | `0.0` to `1.0` | Fraction of tasks for which at least one of the `k` samples passes the hidden test suite. `pass@1` is the single-sample success rate. |

## Reading the numbers

- `0.0` means nothing in the scored set met the benchmark criterion.
- `0.5` means roughly half of the scored items met it.
- `1.0` means a perfect score on that suite.
- Multiply by 100 if you want a percentage-style reading.
- Compare scores within the same suite, not across suites. A `0.43` on GSM8K does not mean the same thing as a `0.43` on `leaderboard_math_hard`.

## Ozone-specific note

The current `ozone eval humaneval` command only generates the sample JSONL. The actual EvalPlus score appears later, when you run `evalplus.evaluate` against that JSONL and read the `pass@k` summary.