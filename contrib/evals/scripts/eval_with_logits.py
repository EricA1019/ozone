#!/usr/bin/env python3
"""Run lm-eval tasks using llama-cpp-python with logits_all=True for correct
loglikelihood scoring.

The llama.cpp HTTP server (llama-server) does NOT return per-token prompt
logprobs — it only returns the generated token's logprob. For loglikelihood
tasks (MMLU, HellaSwag, BBH), this is fundamentally broken because we need
P(continuation | context), not P(next_token_after_continuation | context).

This script loads the GGUF model directly via llama-cpp-python with
logits_all=True, giving us full per-token logits for correct loglikelihood
computation.

Usage:
    python eval_with_logits.py /path/to/model.gguf --preset mmlu --limit 50
"""

import argparse
import json
import os
import sys
import time
from pathlib import Path
from typing import List, Tuple, Optional

import numpy as np


def compute_loglikelihood(llm, context: str, continuation: str):
    """Compute P(continuation | context) using per-token logits.

    Returns (total_logprob, is_greedy).
    """
    ctx_ids = llm.tokenize(context.encode("utf-8"))
    cont_ids = llm.tokenize(continuation.encode("utf-8"))

    # Strip BOS from continuation (llm.tokenize always adds BOS)
    if len(cont_ids) > 0 and len(ctx_ids) > 0 and ctx_ids[0] == cont_ids[0]:
        cont_ids = cont_ids[1:]

    if len(cont_ids) == 0:
        return (0.0, True)

    full_ids = ctx_ids + cont_ids
    ctx_len = len(ctx_ids)

    # Run model to get logits
    llm.reset()
    llm.eval(full_ids)

    # Extract logprobs for continuation positions
    scores = llm._scores  # raw logits: (n_tokens, vocab_size)
    logprobs_arr = llm.logits_to_logprobs(scores)

    total_logprob = 0.0
    is_greedy = True

    for i, cont_id in enumerate(cont_ids):
        token_idx = ctx_len + i
        row = logprobs_arr[token_idx]
        cont_logprob = float(row[cont_id])
        total_logprob += cont_logprob

        best_id = int(scores[token_idx].argmax())
        if best_id != cont_id:
            is_greedy = False

    return (total_logprob, is_greedy)


def run_eval(
    gguf_path: str,
    tasks: list,
    limit: int = 50,
    temperature: float = 0.0,
    max_length: int = 8192,
) -> dict:
    """Run lm-eval tasks with correct loglikelihood via llama-cpp-python."""
    import lm_eval
    from lm_eval.api.model import LM
    from lm_eval.api.registry import register_model
    from lm_eval.utils import make_table

    @register_model("gguf-logits")
    class GGUFPromptLogProbsLM(LM):
        def __init__(
            self,
            model: str = None,
            gguf_path: str = None,
            temperature: float = 0.0,
            max_length: int = 8192,
            max_gen_toks: int = 2048,
            batch_size: int = 1,
            seed: int = 1234,
            **kwargs,
        ):
            super().__init__()
            model_path = gguf_path or model
            self._temperature = temperature
            self._seed = seed
            self._batch_size = batch_size
            self._max_gen_toks = max_gen_toks
            self.max_length = max_length

            print(f"\n[gguf-logits] Loading: {model_path}", file=sys.stderr)
            sys.stderr.flush()
            from llama_cpp import Llama

            self._llm = Llama(
                model_path=model_path,
                n_gpu_layers=-1,
                verbose=False,
                logits_all=True,
                n_ctx=max_length,
            )
            vocab = self._llm._model.n_vocab()
            print(f"[gguf-logits] Loaded! Vocab={vocab}", file=sys.stderr)
            sys.stderr.flush()

        def loglikelihood(self, requests, disable_tqdm: bool = False):
            from tqdm import tqdm
            res = []
            for req in tqdm(requests, disable=disable_tqdm):
                context, continuation = req.args
                logprob, greedy = compute_loglikelihood(
                    self._llm, context, continuation
                )
                res.append((logprob, greedy))
            return res

        def loglikelihood_rolling(self, requests, disable_tqdm: bool = False):
            raise NotImplementedError("not yet supported")

        def generate_until(self, requests, disable_tqdm: bool = False):
            from tqdm import tqdm
            res = []
            for req in tqdm(requests, disable=disable_tqdm):
                inp = req.args[0]
                request_args = req.args[1]
                until = request_args.get("until", ["</s>"])
                max_tokens = request_args.get(
                    "max_gen_toks", self._max_gen_toks
                )
                response = self._llm.create_completion(
                    prompt=inp,
                    max_tokens=max_tokens,
                    stop=until,
                    temperature=self._temperature,
                )
                if response and "choices" in response:
                    text = response["choices"][0].get("text", "").strip()
                    res.append(text)
                else:
                    res.append("")
            return res

    from lm_eval import evaluator

    model = GGUFPromptLogProbsLM(
        model=gguf_path,
        temperature=temperature,
        max_length=max_length,
    )

    results = evaluator.simple_evaluate(
        model=model,
        tasks=tasks,
        num_fewshot=None,
        batch_size=1,
        limit=limit,
    )

    return results


def main():
    parser = argparse.ArgumentParser(
        description="Run lm-eval with correct logprobs via llama-cpp-python"
    )
    parser.add_argument("gguf_path", type=str, help="Path to GGUF model file")
    parser.add_argument(
        "--preset", type=str, default="mmlu",
        choices=["mmlu", "hellaswag", "bbh", "gsm8k", "math",
                 "instruction", "truthfulqa"],
        help="Evaluation preset",
    )
    parser.add_argument("--limit", type=int, default=50, help="Number of samples")
    parser.add_argument(
        "--output_dir", type=str, default="results/logprobs_eval",
        help="Output directory",
    )
    parser.add_argument("--temperature", type=float, default=0.0, help="Temperature")

    args = parser.parse_args()

    TASK_MAP = {
        "mmlu": "mmlu",
        "hellaswag": "hellaswag",
        "bbh": "leaderboard_bbh",
        "gsm8k": "leaderboard_gsm8k",
        "math": "leaderboard_math_hard",
        "instruction": "leaderboard_ifeval",
        "truthfulqa": "truthfulqa_gen",
    }

    task_name = TASK_MAP[args.preset]
    output_path = Path(args.output_dir)
    output_path.mkdir(parents=True, exist_ok=True)

    print(f"{'='*60}")
    print(f"  CORRECT LOGPROBS EVAL (logits_all=True)")
    print(f"{'='*60}")
    print(f"  Model:  {args.gguf_path}")
    print(f"  Task:   {task_name} (limit={args.limit})")
    print(f"  Output: {output_path}")
    print(f"{'='*60}\n")

    # Kill any running llama-server to free GPU memory
    os.system("pkill -f llama-server 2>/dev/null")
    time.sleep(2)

    start = time.time()
    results = run_eval(
        gguf_path=args.gguf_path,
        tasks=[task_name],
        limit=args.limit,
        temperature=args.temperature,
    )
    elapsed = time.time() - start

    if results:
        from lm_eval.utils import make_table

        print(f"\n{'='*60}")
        print(f"  RESULTS ({elapsed:.0f}s)")
        print(f"{'='*60}")
        print(make_table(results))

        results_file = output_path / f"{args.preset}_results.json"
        with open(results_file, "w") as f:
            json.dump(results, f, indent=2, default=str)
        print(f"\nResults saved to: {results_file}")

        if "results" in results:
            print(f"\n  Scores:")
            for task, metrics in results["results"].items():
                for metric, value in metrics.items():
                    if isinstance(value, (int, float)) and 0 <= value <= 1:
                        print(f"    {task}.{metric}: {value*100:.2f}%")
    else:
        print("Evaluation failed!", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
