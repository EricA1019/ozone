#!/usr/bin/env bash
# install-dev-hooks.sh: one-time setup that symlinks contrib/hooks/* into .git/hooks/
# Run once after cloning: ./contrib/install-dev-hooks.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HOOKS_SRC="$REPO_ROOT/contrib/hooks"
GIT_HOOKS_DIR="$REPO_ROOT/.git/hooks"

if [[ ! -d "$REPO_ROOT/.git" ]]; then
    echo "Error: .git directory not found — run this from within the repo." >&2
    exit 1
fi

mkdir -p "$GIT_HOOKS_DIR"

echo "Installing dev hooks from contrib/hooks/ → .git/hooks/"

any_installed=0
for hook_src in "$HOOKS_SRC"/*; do
    [[ -f "$hook_src" ]] || continue
    hook_name="$(basename "$hook_src")"
    hook_dest="$GIT_HOOKS_DIR/$hook_name"

    # Make the template executable
    chmod +x "$hook_src"

    if [[ -L "$hook_dest" ]]; then
        existing_target="$(readlink "$hook_dest")"
        if [[ "$existing_target" == "$hook_src" ]]; then
            printf '✓  %s already linked\n' "$hook_name"
            continue
        fi
        # Symlink points elsewhere — update it
        rm -f "$hook_dest"
        ln -s "$hook_src" "$hook_dest"
        printf '↻  %s updated (was → %s)\n' "$hook_name" "$existing_target"
    elif [[ -f "$hook_dest" ]]; then
        # Real file exists — back it up and replace with symlink
        cp "$hook_dest" "${hook_dest}.bak"
        rm -f "$hook_dest"
        ln -s "$hook_src" "$hook_dest"
        printf '↻  %s replaced existing hook (backup: %s.bak)\n' "$hook_name" "$hook_dest"
    else
        ln -s "$hook_src" "$hook_dest"
        printf '＋  %s installed\n' "$hook_name"
    fi
    any_installed=1
done

echo
if [[ $any_installed -eq 1 ]]; then
    echo "Dev hooks installed. Local binaries will now auto-sync after commits and merges."
else
    echo "All hooks are already current."
fi
