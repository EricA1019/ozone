#!/usr/bin/env bash

set -euo pipefail

usage() {
    cat <<'EOF'
Usage: ./contrib/sync-local-install.sh [--no-build]

Build the current release binaries and sync them into:
  - ~/.cargo/bin
  - ~/.local/bin

Each destination is updated only when its SHA-256 checksum differs from the
freshly built artifact.

Options:
  --no-build   Skip cargo build and sync the current target/release artifacts
  -h, --help   Show this help
EOF
}

checksum_file() {
    local path="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$path" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$path" | awk '{print $1}'
    else
        echo "Need sha256sum or shasum to compare install checksums." >&2
        exit 1
    fi
}

sync_binary() {
    local source="$1"
    local binary_name="$2"
    local source_sum="$3"
    local dest_dir dest_path dest_sum

    for dest_dir in "${DEST_DIRS[@]}"; do
        mkdir -p "$dest_dir"
        dest_path="$dest_dir/$binary_name"

        if [[ -e "$dest_path" || -L "$dest_path" ]]; then
            if [[ -f "$dest_path" ]]; then
                dest_sum="$(checksum_file "$dest_path")"
                if [[ "$dest_sum" == "$source_sum" ]]; then
                    printf '✓ %s is already current\n' "$dest_path"
                    continue
                fi
            fi

            rm -f "$dest_path"
            install -m 755 "$source" "$dest_path"
            printf '↻ %s updated (checksum changed)\n' "$dest_path"
        else
            install -m 755 "$source" "$dest_path"
            printf '＋ %s installed\n' "$dest_path"
        fi

        dest_sum="$(checksum_file "$dest_path")"
        if [[ "$dest_sum" != "$source_sum" ]]; then
            echo "Checksum mismatch after installing $dest_path" >&2
            exit 1
        fi
    done
}

NO_BUILD=0
case "${1:-}" in
    "")
        ;;
    --no-build)
        NO_BUILD=1
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

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TARGET_DIR="$REPO_ROOT/target/release"
DEST_DIRS=("$HOME/.cargo/bin" "$HOME/.local/bin")
INSTALL_STATE_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/ozone"
INSTALL_SOURCE_ROOT_FILE="$INSTALL_STATE_DIR/install-source-root.txt"
BINARIES=(
    "ozone:ozone"
    "ozone-plus:ozone-plus"
    "ozone-mcp-app:ozone-mcp"
)

if [[ "$NO_BUILD" -eq 0 ]]; then
    echo "Building release binaries..."
    (
        cd "$REPO_ROOT"
        cargo build --release -p ozone -p ozone-plus -p ozone-mcp-app
    )
fi

echo "Syncing local installs..."
for spec in "${BINARIES[@]}"; do
    IFS=":" read -r package_name binary_name <<<"$spec"
    source_path="$TARGET_DIR/$binary_name"

    if [[ ! -x "$source_path" ]]; then
        echo "Missing built binary for $package_name at $source_path" >&2
        exit 1
    fi

    source_sum="$(checksum_file "$source_path")"
    printf '\n[%s]\n' "$binary_name"
    sync_binary "$source_path" "$binary_name" "$source_sum"
done

mkdir -p "$INSTALL_STATE_DIR"
printf '%s\n' "$REPO_ROOT" > "$INSTALL_SOURCE_ROOT_FILE"

echo
printf 'Recorded install source root at %s\n' "$INSTALL_SOURCE_ROOT_FILE"
echo "Local install sync complete."
