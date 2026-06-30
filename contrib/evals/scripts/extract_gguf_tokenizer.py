#!/usr/bin/env python3
"""Extract a GGUF model's tokenizer into a HuggingFace-compatible directory.

Usage:
    python extract_gguf_tokenizer.py /path/to/model.gguf [output_dir]

If output_dir is omitted, saves to results/tokenizers/{model_name}/
"""

import json
import os
import sys
from pathlib import Path

GGUF_PATH = Path(sys.argv[1]) if len(sys.argv) > 1 else None
OUT_DIR = Path(sys.argv[2]) if len(sys.argv) > 2 else Path("results/tokenizers") / GGUF_PATH.stem if GGUF_PATH else None

if not GGUF_PATH or not GGUF_PATH.exists():
    print(f"Usage: {sys.argv[0]} /path/to/model.gguf [output_dir]")
    print(f"Error: {GGUF_PATH} not found")
    sys.exit(1)

print(f"Extracting tokenizer from: {GGUF_PATH}")
print(f"Output directory: {OUT_DIR}")

# ============================================================
# Load tokenizer data from GGUF using GGUFReader
# ============================================================
from gguf import GGUFReader, GGUFValueType

reader = GGUFReader(str(GGUF_PATH))

# Helper: decode a bytes field (e.g. "gpt2\0" -> "gpt2")
def decode_bytes_field(memmap):
    """Decode a numpy memmap of uint8 into a UTF-8 string."""
    return bytes(memmap).decode('utf-8', errors='replace').rstrip('\x00')

# Read scalar string fields
def get_str_field(reader, key):
    f = reader.fields.get(key)
    if f is None:
        return None
    # String fields have parts: [len, data...]
    data = None
    for p in f.parts:
        if hasattr(p, 'dtype') and p.dtype.name.startswith('uint'):
            data = bytes(p).decode('utf-8', errors='replace')
            break
    return data

# Read tokenizer model type
tokenizer_model = None
f = reader.fields.get('tokenizer.ggml.model')
if f:
    for p in f.parts:
        if hasattr(p, 'dtype') and p.dtype.name.startswith('uint'):
            tokenizer_model = bytes(p).decode('utf-8', errors='replace').rstrip('\x00')
            break

if not tokenizer_model:
    print("Error: Could not find tokenizer.ggml.model in GGUF")
    sys.exit(1)

print(f"Tokenizer model: {tokenizer_model}")

# Read the concatenated strings blob
# For KV_TOKENIZER_LIST, the 'data' field is a list of byte offsets
# and one of the 'parts' contains the concatenated strings as bytes
tokens_f = reader.fields.get('tokenizer.ggml.tokens')
if not tokens_f:
    print("Error: Could not find tokenizer.ggml.tokens")
    sys.exit(1)

# Find the concatenated strings in the parts
token_bytes = None
for p in tokens_f.parts:
    if hasattr(p, 'dtype') and p.dtype.name.startswith('uint') and len(p) > 10000:
        token_bytes = bytes(p)
        break

if token_bytes is None:
    # Try reading from the raw data
    print("Warning: Could not find concatenated token strings, trying alternate methods")
    sys.exit(1)

# The data field contains byte offsets (end positions of each token string)
offsets = list(tokens_f.data)
num_tokens = len(offsets)

print(f"Number of tokens: {num_tokens}")

# Decode tokens using offsets
tokens = []
start = 0
for end in offsets:
    token = token_bytes[start:end].decode('utf-8', errors='replace')
    tokens.append(token)
    start = end

print(f"Decoded {len(tokens)} tokens")
print(f"First 5: {tokens[:5]}")
print(f"Last 5:  {tokens[-5:]}")

# Read merges
merges = []
merges_f = reader.fields.get('tokenizer.ggml.merges')
if merges_f:
    merge_bytes = None
    for p in merges_f.parts:
        if hasattr(p, 'dtype') and p.dtype.name.startswith('uint') and len(p) > 100:
            merge_bytes = bytes(p)
            break
    if merge_bytes:
        merges_text = merge_bytes.decode('utf-8', errors='replace')
        merges = [m for m in merges_text.split('\n') if m.strip()]
    print(f"Found {len(merges)} merge rules")

# Read special token IDs
def get_int_field(reader, key):
    f = reader.fields.get(key)
    if f is None:
        return None
    for p in f.parts:
        if hasattr(p, 'dtype'):
            arr = p
            if len(arr) > 0:
                return int(arr[0])
    return None

bos_id = get_int_field(reader, 'tokenizer.ggml.bos_token_id') or 1
eos_id = get_int_field(reader, 'tokenizer.ggml.eos_token_id') or 2
pad_id = get_int_field(reader, 'tokenizer.ggml.padding_token_id') or 0

print(f"Special tokens: BOS={bos_id} ('{tokens[bos_id] if bos_id < len(tokens) else '?'}')")
print(f"                 EOS={eos_id} ('{tokens[eos_id] if eos_id < len(tokens) else '?'}')")
print(f"                 PAD={pad_id} ('{tokens[pad_id] if pad_id < len(tokens) else '?'}')")

# ============================================================
# Build HuggingFace tokenizer files
# ============================================================
OUT_DIR.mkdir(parents=True, exist_ok=True)

# 1. vocab.json (token -> id)
vocab = {t: i for i, t in enumerate(tokens)}
with open(OUT_DIR / "vocab.json", 'w', encoding='utf-8') as f:
    json.dump(vocab, f, ensure_ascii=False, indent=2)

# 2. merges.txt (BPE merge rules)
if merges:
    with open(OUT_DIR / "merges.txt", 'w', encoding='utf-8') as f:
        f.write('#version: 0.2\n')
        f.write('\n'.join(merges))

# 3. tokenizer_config.json
tokenizer_config = {
    "add_prefix_space": False,
    "bos_token": tokens[bos_id] if bos_id < len(tokens) else "<s>",
    "eos_token": tokens[eos_id] if eos_id < len(tokens) else "</s>",
    "pad_token": tokens[pad_id] if pad_id < len(tokens) else "<pad>",
    "unk_token": tokens[0] if len(tokens) > 0 else "<unk>",
    "model_max_length": 1000000000000000019884624838656,
    "tokenizer_class": "GPT2Tokenizer" if tokenizer_model == "gpt2" else "PreTrainedTokenizerFast",
    "clean_up_tokenization_spaces": False,
}
with open(OUT_DIR / "tokenizer_config.json", 'w', encoding='utf-8') as f:
    json.dump(tokenizer_config, f, ensure_ascii=False, indent=2)

# 4. special_tokens_map.json
special_tokens = {
    "bos_token": tokens[bos_id] if bos_id < len(tokens) else "<s>",
    "eos_token": tokens[eos_id] if eos_id < len(tokens) else "</s>",
    "pad_token": tokens[pad_id] if pad_id < len(tokens) else "<pad>",
    "unk_token": tokens[0] if len(tokens) > 0 else "<unk>",
}
with open(OUT_DIR / "special_tokens_map.json", 'w', encoding='utf-8') as f:
    json.dump(special_tokens, f, ensure_ascii=False, indent=2)

# 5. tokenizer.json (HuggingFace fast tokenizer format)
# For GPT-2 BPE, write a minimal tokenizer.json
if tokenizer_model == "gpt2" and merges:
    tokenizer_json = {
        "version": "1.0",
        "truncation": None,
        "padding": None,
        "added_tokens": [
            {"id": bos_id, "content": tokens[bos_id], "single_word": False, "lstrip": False, "rstrip": False, "normalized": False, "special": True},
            {"id": eos_id, "content": tokens[eos_id], "single_word": False, "lstrip": False, "rstrip": False, "normalized": False, "special": True},
            {"id": pad_id, "content": tokens[pad_id], "single_word": False, "lstrip": False, "rstrip": False, "normalized": False, "special": True},
        ],
        "normalizer": {"type": "Sequence", "normalizers": []},
        "pre_tokenizer": {
            "type": "ByteLevel",
            "add_prefix_space": False,
            "trim_offsets": True,
            "use_regex": True,
        },
        "post_processor": {
            "type": "ByteLevel",
            "add_prefix_space": False,
            "trim_offsets": True,
        },
        "decoder": {
            "type": "ByteLevel",
            "add_prefix_space": False,
            "trim_offsets": True,
        },
        "model": {
            "type": "BPE",
            "dropout": None,
            "unk_token": tokens[0] if len(tokens) > 0 else "<unk>",
            "continuing_subword_prefix": "",
            "end_of_word_suffix": "",
            "fuse_unk": False,
            "byte_fallback": False,
            "vocab": vocab,
            "merges": merges,
        },
    }
    with open(OUT_DIR / "tokenizer.json", 'w', encoding='utf-8') as f:
        json.dump(tokenizer_json, f, ensure_ascii=False, indent=2)

print(f"\nTokenizer saved to: {OUT_DIR}")
print(f"Use with: --tokenizer {OUT_DIR}")
print("Done!")
