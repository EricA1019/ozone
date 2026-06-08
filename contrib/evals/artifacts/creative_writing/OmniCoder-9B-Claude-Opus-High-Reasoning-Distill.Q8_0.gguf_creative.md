# Creative Writing Report — OmniCoder-9B-Claude-Opus-High-Reasoning-Distill.Q8_0.gguf

## Per-Temperature Score Summary

| Temperature | Avg Distinct-2 | Avg Self-BLEU | Avg Repetition Ratio | Avg Length |
|------------|---------------|---------------|---------------------|------------|
| T=0    | 0.4310 | 0.8879 | 0.6857 | 209.7 |
| T=0.4  | 0.3505 | 1.8676 | 0.7347 | 198.1 |
| T=0.7  | 0.4501 | 3.1119 | 0.6559 | 200.6 |
| T=1    | 0.4711 | 2.5316 | 0.6300 | 183.8 |

## Interpretation

- **Distinct-2** (higher is better): Bigram diversity. A high value means the model uses a varied vocabulary.
- **Self-BLEU** (lower is better): Self-similarity across sentences. High values indicate repetitive output.
- **Repetition Ratio** (lower is better): Fraction of repeated words. High values suggest the model is stuck in loops.
- **Length** (higher is better for open-ended prompts): Generation length in tokens. Very short may mean truncation.

The sweet-spot temperature maximizes distinct-2 while minimizing self-BLEU and repetition.
