#!/usr/bin/env bash
# refresh-graphify.sh: update an existing Graphify graph for this repo after code changes.

set -euo pipefail

quiet=0
if [[ "${1:-}" == "--quiet" ]]; then
    quiet=1
    shift
fi

if [[ $# -ne 0 ]]; then
    printf 'Usage: %s [--quiet]\n' "$(basename "$0")" >&2
    exit 1
fi

say() {
    if [[ $quiet -eq 0 ]]; then
        printf '%s\n' "$1"
    fi
}

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
GRAPH_FILE="$REPO_ROOT/graphify-out/graph.json"

if ! command -v graphify >/dev/null 2>&1; then
    say "[graphify] CLI not found; skipping graph refresh"
    exit 0
fi

if [[ ! -f "$GRAPH_FILE" ]]; then
    say "[graphify] no graphify-out/graph.json yet; run /graphify . in Copilot Chat first"
    exit 0
fi

say "[graphify] refreshing code graph with graphify update ."
cd "$REPO_ROOT"
graphify update .
say "[graphify] refresh complete"