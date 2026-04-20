#!/usr/bin/env bash

set -euo pipefail

usage() {
    cat <<'EOF'
Usage: ./contrib/prune-build-artifacts.sh [--dry-run] [--full]

Drop rebuildable Cargo outputs that tend to balloon over time while preserving
the most useful rollback-friendly artifacts by default.

Default mode removes:
  - target/debug/{deps,incremental,build,.fingerprint,examples}
  - target/release/{deps,build,.fingerprint,incremental,examples}
  - target/doc
  - target/release-lite
  - target/test-artifacts
  - target/cxxbridge

This keeps the current top-level binaries in target/debug and target/release.

Options:
  --dry-run   Print what would be removed without deleting anything
  --full      Also remove target/debug and target/release completely
  -h, --help  Show this help
EOF
}

DRY_RUN=0
FULL=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            DRY_RUN=1
            ;;
        --full)
            FULL=1
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown argument: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
    shift
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

measure_bytes() {
    local path="$1"
    du -sb "$path" 2>/dev/null | awk '{print $1}'
}

human_size() {
    local bytes="$1"
    numfmt --to=iec-i --suffix=B "$bytes"
}

remove_path() {
    local path="$1"
    local abs="$REPO_ROOT/$path"
    [[ -e "$abs" ]] || return 0

    local bytes
    bytes="$(measure_bytes "$abs")"
    [[ -n "$bytes" && "$bytes" -gt 0 ]] || bytes=0

    printf '%-28s %10s\n' "$path" "$(human_size "$bytes")"
    RECLAIMED_BYTES=$((RECLAIMED_BYTES + bytes))

    if [[ "$DRY_RUN" -eq 0 ]]; then
        rm -rf "$abs"
    fi
}

RECLAIMED_BYTES=0

REMOVE_PATHS=(
    "target/debug/deps"
    "target/debug/incremental"
    "target/debug/build"
    "target/debug/.fingerprint"
    "target/debug/examples"
    "target/release/deps"
    "target/release/build"
    "target/release/.fingerprint"
    "target/release/incremental"
    "target/release/examples"
    "target/doc"
    "target/release-lite"
    "target/test-artifacts"
    "target/cxxbridge"
)

if [[ "$FULL" -eq 1 ]]; then
    REMOVE_PATHS+=(
        "target/debug"
        "target/release"
    )
fi

BEFORE_TARGET_BYTES=0
if [[ -d "$REPO_ROOT/target" ]]; then
    BEFORE_TARGET_BYTES="$(measure_bytes "$REPO_ROOT/target")"
fi

if [[ "${#REMOVE_PATHS[@]}" -eq 0 ]]; then
    echo "Nothing configured to remove."
    exit 0
fi

if [[ "$DRY_RUN" -eq 1 ]]; then
    echo "Dry run: would remove these artifact paths:"
else
    echo "Removing artifact paths:"
fi

for path in "${REMOVE_PATHS[@]}"; do
    remove_path "$path"
done

AFTER_TARGET_BYTES=0
if [[ -d "$REPO_ROOT/target" ]]; then
    AFTER_TARGET_BYTES="$(measure_bytes "$REPO_ROOT/target")"
fi

echo
printf 'Target before: %s\n' "$(human_size "${BEFORE_TARGET_BYTES:-0}")"
if [[ "$DRY_RUN" -eq 1 ]]; then
    printf 'Would reclaim: %s\n' "$(human_size "$RECLAIMED_BYTES")"
else
    printf 'Reclaimed:     %s\n' "$(human_size "$RECLAIMED_BYTES")"
    printf 'Target after:  %s\n' "$(human_size "${AFTER_TARGET_BYTES:-0}")"
fi

if [[ "$FULL" -eq 0 ]]; then
    echo
    echo "Kept current top-level binaries in target/debug and target/release."
fi
