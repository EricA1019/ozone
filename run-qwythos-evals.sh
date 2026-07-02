#!/bin/bash
set -euo pipefail

MODEL="Qwythos-9B-Claude-Mythos-5-1M-MTP-Q4_K_M"
TOKENIZER="results/tokenizers/Qwythos-9B-Claude-Mythos-5-1M-MTP-Q4_K_M"
LOGDIR="/home/eric/projects/ozone/results/eval_logs"
mkdir -p "$LOGDIR"

log() {
    echo "[$(date '+%H:%M:%S')] $*" | tee -a "$LOGDIR/qwythos-batch.log"
}

run_eval() {
    local preset=$1 limit=$2 tokenizer=${3:-}
    local token_flag=""
    [[ -n "$tokenizer" ]] && token_flag="--tokenizer $tokenizer"
    local outfile="$LOGDIR/qwythos-${preset}-limit${limit}.out"

    log "Running $preset (limit=$limit)..."
    cd /home/eric/projects/ozone
    cargo run -- eval "$MODEL" --preset "$preset" --limit "$limit" $token_flag \
        > "$outfile" 2>&1
    local rc=$?
    if [[ $rc -eq 0 ]]; then
        log "$preset: ✅ DONE (limit=$limit)"
        # Extract score
        grep -E "^\|.*[0-9]+\.[0-9]+.*\|$" "$outfile" | tail -3 | tee -a "$LOGDIR/qwythos-batch.log"
    else
        log "$preset: ❌ FAILED (exit $rc)"
        tail -20 "$outfile" | tee -a "$LOGDIR/qwythos-batch.log"
    fi
}

log "=== Qwythos-9B eval batch starting ==="
log "Model: $MODEL"
log "Server must be running on http://127.0.0.1:8989"

# Generate-only tasks (no tokenizer needed)
run_eval math         10     # 8-shot, slow — expect ~20 min
run_eval instruction  10     # multi-shot, slow — expect ~15 min
run_eval truthfulqa   50     # single questions, faster

log "=== Batch complete ==="
