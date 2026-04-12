#!/usr/bin/env bash
# Launch KoboldCpp with a model from ~/models/
# Usage: ./launch-koboldcpp.sh <model.gguf> [--contextsize N] [--gpulayers N]
#
# If no overrides given, uses sensible defaults per model size category.
# Flash attention is ON by default in KoboldCpp 1.111+.

set -euo pipefail

KCPP="$HOME/koboldcpp/koboldcpp"
MODEL_DIR="$HOME/models"
PRESET_FILE="$MODEL_DIR/koboldcpp-presets.conf"

if [[ $# -lt 1 ]]; then
    echo "Usage: $0 <model.gguf> [extra koboldcpp flags...]"
    echo ""
    echo "Available models:"
    ls -1 "$MODEL_DIR"/*.gguf 2>/dev/null | xargs -I{} basename {} | sort
    exit 1
fi

MODEL="$1"
shift

# Resolve full path if just a filename was given
if [[ ! -f "$MODEL" ]]; then
    MODEL="$MODEL_DIR/$MODEL"
fi

if [[ ! -f "$MODEL" ]]; then
    echo "Error: Model file not found: $MODEL"
    exit 1
fi

# Get file size in GB to auto-categorize
SIZE_BYTES=$(stat --format=%s "$(readlink -f "$MODEL")" 2>/dev/null || stat -f%z "$(readlink -f "$MODEL")" 2>/dev/null)
SIZE_GB=$(awk "BEGIN {printf \"%.1f\", $SIZE_BYTES / 1073741824}")
MODEL_NAME=$(basename "$MODEL")

echo "=== KoboldCpp Launcher ==="
echo "Model: $MODEL_NAME ($SIZE_GB GB)"
echo ""

# Default flags (can be overridden by passing extra args)
CUDA_FLAG="--usecuda"
QUANTKV="--quantkv 1"
CTX=""
LAYERS=""
PRESET_PROFILE=""

# Check if user passed --contextsize or --gpulayers as overrides
HAS_CTX=false
HAS_LAYERS=false
HAS_QUANTKV=false
for arg in "$@"; do
    case "$arg" in
        --contextsize|--ctx-size|-c) HAS_CTX=true ;;
        --gpulayers|--gpu-layers|--n-gpu-layers|-ngl) HAS_LAYERS=true ;;
        --quantkv) HAS_QUANTKV=true ;;
    esac
done

if [[ -f "$PRESET_FILE" ]]; then
    while IFS='|' read -r preset_model preset_layers preset_ctx preset_quantkv preset_note; do
        [[ -z "$preset_model" || "${preset_model:0:1}" == "#" ]] && continue
        if [[ "$preset_model" == "$MODEL_NAME" ]]; then
            if ! $HAS_LAYERS; then
                LAYERS="--gpulayers $preset_layers"
            fi
            if ! $HAS_CTX; then
                CTX="--contextsize $preset_ctx"
            fi
            if ! $HAS_QUANTKV; then
                QUANTKV="--quantkv $preset_quantkv"
            fi
            PRESET_PROFILE=${preset_note:-Tuned preset}
            break
        fi
    done < "$PRESET_FILE"
fi

# Auto-detect settings based on model size
# RTX 3060 12GB VRAM budget
if [[ -n "$PRESET_PROFILE" ]]; then
    echo "Profile: $PRESET_PROFILE"
elif ! $HAS_CTX || ! $HAS_LAYERS; then
    if echo "$MODEL_NAME" | grep -qi "MOE\|MoE\|moe"; then
        # MOE models: only a fraction of params active per token
        $HAS_CTX  || CTX="--contextsize 12288"
        $HAS_LAYERS || LAYERS="--gpulayers -1"
        echo "Profile: MOE (full GPU offload, 12K context)"
    elif awk "BEGIN {exit ($SIZE_GB <= 8.0) ? 0 : 1}"; then
        # Small models (<=8GB): fully in VRAM, max context
        $HAS_CTX  || CTX="--contextsize 16384"
        $HAS_LAYERS || LAYERS="--gpulayers -1"
        echo "Profile: Small (full GPU offload, 16K context)"
    elif awk "BEGIN {exit ($SIZE_GB <= 12.5) ? 0 : 1}"; then
        # Medium models (8-12.5GB): fits in VRAM, good context
        $HAS_CTX  || CTX="--contextsize 8192"
        $HAS_LAYERS || LAYERS="--gpulayers -1"
        echo "Profile: Medium (full GPU offload, 8K context)"
    elif awk "BEGIN {exit ($SIZE_GB <= 14.0) ? 0 : 1}"; then
        # Large models (12.5-14GB): partial offload needed
        $HAS_CTX  || CTX="--contextsize 8192"
        $HAS_LAYERS || LAYERS="--gpulayers 32"
        echo "Profile: Large (partial GPU offload ~32 layers, 8K context)"
    else
        # Very large models (>14GB): conservative offload
        $HAS_CTX  || CTX="--contextsize 4096"
        $HAS_LAYERS || LAYERS="--gpulayers 28"
        echo "Profile: X-Large (partial GPU offload ~28 layers, 4K context)"
    fi
else
    CTX=""
    LAYERS=""
    echo "Profile: Custom (user overrides)"
fi

echo ""
echo "Running: $KCPP --model $MODEL $CUDA_FLAG $LAYERS $CTX $QUANTKV $*"
echo ""

exec "$KCPP" \
    --model "$MODEL" \
    $CUDA_FLAG \
    $LAYERS \
    $CTX \
    $QUANTKV \
    "$@"
