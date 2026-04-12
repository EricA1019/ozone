#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

if ! command -v mex >/dev/null 2>&1; then
  echo "mex is not installed or not on PATH" >&2
  exit 127
fi

cd "${REPO_ROOT}"
exec mex sync "$@"
