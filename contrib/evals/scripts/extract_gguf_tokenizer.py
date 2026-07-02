#!/usr/bin/env python3
"""Extract a GGUF model's tokenizer into a HuggingFace-compatible directory.

Uses llama-cpp-python as the primary extraction method (reliable for all GGUF
models). Falls back to gguf-py for metadata and BPE merge rules.

Usage:
    python extract_gguf_tokenizer.py /path/to/model.gguf [output_dir]

If output_dir is omitted, saves to results/tokenizers/{model_name}/
"""

import json
import os
import sys
from pathlib import Path


def get_gguf_merges(gguf_path):
    """Extract BPE merge rules from GGUF metadata via gguf-py.

    The GGUF format stores a string array as:
      [uint64 key_len][uint8 key][uint32 type][uint32 elem_type][uint64 count]
      followed by [uint64 str_len][uint8 str_bytes] pairs for each element.
    """
    try:
        from gguf import GGUFReader

        reader = GGUFReader(str(gguf_path))
        merges_f = reader.fields.get('tokenizer.ggml.merges')
        if merges_f is None:
            return []

        # The first 5 parts are the field header (key, type indicator, count).
        # After that, merges come as (uint64 length, uint8 data) pairs.
        parts = merges_f.parts
        if len(parts) <= 5:
            return []

        merges = []
        # Skip parts[0..4] (header), then process pairs
        i = 5
        while i + 1 < len(parts):
            # parts[i] should be uint64 (string length)
            # parts[i+1] should be uint8 (string bytes)
            if hasattr(parts[i], 'dtype') and parts[i].dtype.name == 'uint64':
                if hasattr(parts[i+1], 'dtype') and parts[i+1].dtype.name == 'uint8':
                    text = bytes(parts[i+1]).decode('utf-8', errors='replace').strip()
                    if text and len(text) > 1:
                        merges.append(text)
            i += 2

        return merges
    except Exception:
        pass
    return []


def read_gguf_metadata(gguf_path):
    """Read model type and scores from GGUF metadata."""
    result = {
        'tokenizer_model_type': 'Unknown',
        'merges': [],
        'scores': [],
        'architecture': '',
    }
    try:
        from gguf import GGUFReader

        reader = GGUFReader(str(gguf_path))

        # Read architecture (value is in the last uint8 part)
        f = reader.fields.get('general.architecture')
        if f:
            last_uint8 = None
            for p in reversed(f.parts):
                if hasattr(p, 'dtype') and p.dtype.name == 'uint8':
                    last_uint8 = p
                    break
            if last_uint8 is not None:
                result['architecture'] = bytes(last_uint8).decode('utf-8', errors='replace').rstrip('\x00')

        # Read tokenizer model type (value is in the last uint8 part)
        f = reader.fields.get('tokenizer.ggml.model')
        if f:
            last_uint8 = None
            for p in reversed(f.parts):
                if hasattr(p, 'dtype') and p.dtype.name == 'uint8':
                    last_uint8 = p
                    break
            if last_uint8 is not None:
                result['tokenizer_model_type'] = bytes(last_uint8).decode('utf-8', errors='replace').rstrip('\x00')

        # Read scores (for unigram models)
        f = reader.fields.get('tokenizer.ggml.scores')
        if f:
            result['scores'] = list(f.data)

        # Read merges
        result['merges'] = get_gguf_merges(gguf_path)

    except Exception as e:
        print(f"Note: gguf-py metadata read failed ({e})")

    return result


def main():
    gguf_path = Path(sys.argv[1]) if len(sys.argv) > 1 else None
    if not gguf_path or not gguf_path.exists():
        print(f"Usage: {sys.argv[0]} /path/to/model.gguf [output_dir]")
        print(f"Error: {gguf_path} not found")
        sys.exit(1)

    if len(sys.argv) > 2:
        out_dir = Path(sys.argv[2])
    else:
        out_dir = Path("results") / "tokenizers" / gguf_path.stem

    print(f"Extracting tokenizer from: {gguf_path}")
    print(f"Output directory: {out_dir}")

    out_dir.mkdir(parents=True, exist_ok=True)

    # ============================================================
    # Load tokenizer via llama-cpp-python (reliable for all GGUF models)
    # ============================================================
    from llama_cpp import Llama

    print("Loading model with llama-cpp-python (n_gpu_layers=0)...")
    llm = Llama(model_path=str(gguf_path), n_gpu_layers=0, verbose=False)
    n_vocab = llm._model.n_vocab()
    print(f"Model vocab size: {n_vocab}")

    tokens = []
    for i in range(n_vocab):
        try:
            t = llm._model.token_get_text(i)
            tokens.append(t if t is not None else f"<token_{i}>")
        except Exception:
            tokens.append(f"<token_{i}>")

    print(f"Extracted {len(tokens)} tokens")
    print(f"First 5: {tokens[:5]}")
    print(f"Last 5:  {tokens[-5:]}")

    def safe_get(fn, default):
        try:
            return fn()
        except Exception:
            return default

    bos_id = safe_get(lambda: llm._model.token_bos(), 1)
    eos_id = safe_get(lambda: llm._model.token_eos(), 2)
    pad_id = safe_get(lambda: llm._model.token_pad(), 0)

    # ============================================================
    # Read metadata and merges via gguf-py
    # ============================================================
    meta = read_gguf_metadata(str(gguf_path))
    print(f"Architecture: {meta['architecture']}")
    print(f"Tokenizer model type: {meta['tokenizer_model_type']}")
    print(f"BPE merge rules: {len(meta['merges'])}")

    # Determine tokenizer class for HF config
    tokenizer_class = 'PreTrainedTokenizerFast'
    arch = meta['architecture'].lower()
    if 'llama' in arch or 'mistral' in arch or 'qwen2' in arch:
        tokenizer_class = 'LlamaTokenizerFast'
    elif 'gpt' in arch or 'starcoder' in arch:
        tokenizer_class = 'GPT2Tokenizer'

    has_merges = len(meta['merges']) > 0
    has_scores = len(meta['scores']) > 0

    # ============================================================
    # Build tokenizer.json
    # ============================================================
    if has_merges:
        # BPE model
        model_config = {
            "type": "BPE",
            "dropout": None,
            "unk_token": None,
            "ignore_merges": False,
            "vocab": {t: i for i, t in enumerate(tokens)},
            "merges": meta['merges'],
        }
    elif has_scores:
        # Unigram model (SentencePiece)
        model_config = {
            "type": "Unigram",
            "unk_token": tokens[eos_id] if eos_id < len(tokens) else "<unk>",
            "vocab": [
                {"id": i, "piece": t, "score": float(meta['scores'][i]) if i < len(meta['scores']) else 0.0}
                for i, t in enumerate(tokens)
            ],
        }
    else:
        # Unknown - try BPE with empty merges (fallback)
        model_config = {
            "type": "BPE",
            "dropout": None,
            "unk_token": None,
            "ignore_merges": False,
            "vocab": {t: i for i, t in enumerate(tokens)},
            "merges": [],
        }

    tokenizer_json = {
        "version": "1.0",
        "truncation": None,
        "padding": None,
        "added_tokens": [],
        "normalizer": None,
        "pre_tokenizer": None,
        "post_processor": None,
        "decoder": None,
        "model": model_config,
    }

    with open(out_dir / "tokenizer.json", "w", encoding="utf-8") as f:
        json.dump(tokenizer_json, f, ensure_ascii=False, indent=2)
    size = os.path.getsize(out_dir / "tokenizer.json")
    print(f"Wrote tokenizer.json ({size} bytes)")

    # ============================================================
    # Write tokenizer_config.json
    # ============================================================
    config = {
        "add_prefix_space": False,
        "added_tokens_decoder": {},
        "bos_token": tokens[bos_id] if bos_id < len(tokens) else "<s>",
        "clean_up_tokenization_spaces": True,
        "eos_token": tokens[eos_id] if eos_id < len(tokens) else "</s>",
        "model_max_length": 1000000000000000019884624838656,
        "pad_token": tokens[pad_id] if pad_id < len(tokens) else "<pad>",
        "sp_model_kwargs": {},
        "tokenizer_class": tokenizer_class,
        "unk_token": tokens[eos_id] if eos_id < len(tokens) else "<unk>",
        "chat_template": None,
    }

    with open(out_dir / "tokenizer_config.json", "w", encoding="utf-8") as f:
        json.dump(config, f, ensure_ascii=False, indent=2)
    print("Wrote tokenizer_config.json")

    # ============================================================
    # Write special_tokens_map.json
    # ============================================================
    special_map = {
        "bos_token": tokens[bos_id] if bos_id < len(tokens) else "<s>",
        "eos_token": tokens[eos_id] if eos_id < len(tokens) else "</s>",
        "pad_token": tokens[pad_id] if pad_id < len(tokens) else "<pad>",
        "unk_token": tokens[eos_id] if eos_id < len(tokens) else "<unk>",
    }

    with open(out_dir / "special_tokens_map.json", "w", encoding="utf-8") as f:
        json.dump(special_map, f, ensure_ascii=False, indent=2)
    print("Wrote special_tokens_map.json")

    # ============================================================
    # Write vocab.json (token_id -> token_text)
    # ============================================================
    with open(out_dir / "vocab.json", "w", encoding="utf-8") as f:
        json.dump(dict(enumerate(tokens)), f, ensure_ascii=False, indent=2)
    print(f"Wrote vocab.json ({len(tokens)} tokens)")

    print(f"\nDone! Tokenizer saved to: {out_dir}")
    print(f"To use with lm-eval:\n  --tokenizer {out_dir}")


if __name__ == "__main__":
    main()
