# Eval Result Ranges

This note covers the shipped `ozone eval` probes and how to read their scores.

All of the metrics below are normalized fractions on a `0.0` to `1.0` scale. Higher is better.

| Probe | Metric(s) | Range | What it means |
| --- | --- | --- | --- |
| GSM8K (`lm-eval`, `gsm8k`) | `exact_match,strict-match` and `exact_match,flexible-extract` | `0.0` to `1.0` | Fraction of sampled math questions answered correctly after the selected answer-extraction filter. `1.0` means every scored sample was correct. |
| IFEval (instruction-following, `lm-eval`, `leaderboard_ifeval`) | `prompt_level_strict_acc`, `inst_level_strict_acc`, `prompt_level_loose_acc`, `inst_level_loose_acc` | `0.0` to `1.0` | `prompt_level_*` measures whether the whole prompt satisfied every instruction. `inst_level_*` averages the individual instruction checks. `strict` requires exact compliance; `loose` allows semantically acceptable variation. |
| Math hard (`lm-eval`, `leaderboard_math_hard`) | `exact_match` and `exact_match_original` | `0.0` to `1.0` | Fraction of parsed answers that match the reference solution. The grouped `leaderboard_math_hard` score is the mean across its seven subtasks. |
| HumanEval / EvalPlus (`evalplus.evaluate`) | `pass@1`, `pass@10`, `pass@100` | `0.0` to `1.0` | Fraction of tasks for which at least one of the `k` samples passes the hidden test suite. `pass@1` is the single-sample success rate. |
| MMLU (`lm-eval`, `mmlu`) | `acc` | `0.0` to `1.0` | Accuracy across 57 academic domains (multiple choice). A 0.25 random baseline. |
| MMLU-Pro (`lm-eval`, `mmlu_pro`) | `acc` | `0.0` to `1.0` | Harder MMLU variant with more challenging questions. |
| ARC-Challenge (`lm-eval`, `arc_challenge`) | `acc` | `0.0` to `1.0` | AI2 Reasoning Challenge — science multiple-choice. |
| HellaSwag (`lm-eval`, `hellaswag`) | `acc` | `0.0` to `1.0` | Commonsense reasoning with adversarial distractor sentences. |
| BBH (`lm-eval`, `leaderboard_bbh`) | `acc` | `0.0` to `1.0` | Big Bench Hard — multi-step logic across 23 tasks. |
| BBH Formal Fallacies (`lm-eval`, `bbh_formal_fallacies`) | `acc` | `0.0` to `1.0` | Logical fallacy detection (BBH sub-task). |
| BBH Causal Judgement (`lm-eval`, `bbh_causal_judgement`) | `acc` | `0.0` to `1.0` | Causal reasoning (BBH sub-task). |
| Hendrycks Ethics (`lm-eval`, `hendrycks_ethics`) | `acc` | `0.0` to `1.0` | Ethics benchmark covering commonsense, justice, deontology, virtue, utilitarianism. |
| MMLU Philosophy (`lm-eval`, `mmlu_philosophy`) | `acc` | `0.0` to `1.0` | MMLU philosophy sub-task. |
| DROP (`lm-eval`, `drop`) | `f1` and `em` | `0.0` to `1.0` | Discrete reasoning over paragraphs — reading comprehension + math. |
| MBPP (`lm-eval`, `mbpp`) | `pass@1` | `0.0` to `1.0` | Mostly Basic Python Programming — function completion tests. |
| GPQA (`lm-eval`, `gpqa_main_zeroshot`) | `acc` | `0.0` to `1.0` | Graduate-level physics Q&A (very hard, opt-in only). |

## Reading the numbers

- `0.0` means nothing in the scored set met the benchmark criterion.
- `0.5` means roughly half of the scored items met it.
- `1.0` means a perfect score on that suite.
- Multiply by 100 if you want a percentage-style reading.
- Compare scores within the same suite, not across suites. A `0.43` on GSM8K does not mean the same thing as a `0.43` on `leaderboard_math_hard`.

## Ozone-specific note

The current `ozone eval humaneval` command only generates the sample JSONL. The actual EvalPlus score appears later, when you run `evalplus.evaluate` against that JSONL and read the `pass@k` summary.