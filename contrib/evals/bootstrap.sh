#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENV_DIR="$SCRIPT_DIR/.venv"
PYTHON_BIN="${PYTHON_BIN:-python3}"
VENV_PYTHON="$VENV_DIR/bin/python"
LM_EVAL_REQUIREMENTS="$SCRIPT_DIR/requirements-lm-eval.txt"
EVALPLUS_REQUIREMENTS="$SCRIPT_DIR/requirements-evalplus.txt"
NLTK_RESOURCE_NAME="punkt_tab"

if [[ ! -f "$LM_EVAL_REQUIREMENTS" ]]; then
  echo "Missing requirements file: $LM_EVAL_REQUIREMENTS" >&2
  exit 1
fi

if [[ ! -f "$EVALPLUS_REQUIREMENTS" ]]; then
  echo "Missing requirements file: $EVALPLUS_REQUIREMENTS" >&2
  exit 1
fi

"$PYTHON_BIN" -m venv "$VENV_DIR"
"$VENV_PYTHON" -m pip install --upgrade pip
"$VENV_PYTHON" -m pip install -r "$LM_EVAL_REQUIREMENTS" -r "$EVALPLUS_REQUIREMENTS"
"$VENV_PYTHON" -m nltk.downloader "$NLTK_RESOURCE_NAME"

echo "Eval runner environment ready at $VENV_DIR"