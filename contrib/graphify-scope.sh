#!/usr/bin/env bash

set -euo pipefail

usage() {
    cat <<'EOF'
Usage: graphify-scope.sh [--quiet] <scope>
       graphify-scope.sh --list

Build an isolated Graphify graph under tmp/graphify-scopes/<scope>/.

Scopes:
  ozone-src       Base ozone src/ tree
  ozone-tui       Full ozone-tui src/ tree
  ozone-tui-core  Production-only ozone-tui core: lib.rs + layout.rs + render/coordinator.rs,
                  with embedded #[cfg(test)] mod tests blocks stripped before extraction
  ozone-memory    Full ozone-memory src/ tree
EOF
}

quiet=0
scope=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --quiet)
            quiet=1
            shift
            ;;
        --list)
            printf '%s\n' "ozone-src" "ozone-tui" "ozone-tui-core" "ozone-memory"
            exit 0
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            if [[ -n "$scope" ]]; then
                usage >&2
                exit 1
            fi
            scope="$1"
            shift
            ;;
    esac
done

if [[ -z "$scope" ]]; then
    usage >&2
    exit 1
fi

case "$scope" in
    ozone-src|ozone-tui|ozone-tui-core|ozone-memory)
        ;;
    *)
        printf 'Unknown scope: %s\n\n' "$scope" >&2
        usage >&2
        exit 1
        ;;
esac

say() {
    if [[ $quiet -eq 0 ]]; then
        printf '%s\n' "$1"
    fi
}

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

resolve_python() {
    local python_path=""
    local graphify_bin

    if [[ -f "$REPO_ROOT/graphify-out/.graphify_python" ]]; then
        python_path="$(cat "$REPO_ROOT/graphify-out/.graphify_python")"
    else
        graphify_bin="$(command -v graphify || true)"
        if [[ -n "$graphify_bin" ]]; then
            python_path="$(head -1 "$graphify_bin" | tr -d '#!')"
            case "$python_path" in
                *[!a-zA-Z0-9/_.-]*)
                    python_path="python3"
                    ;;
            esac
        else
            python_path="python3"
        fi
    fi

    if ! "$python_path" -c "import graphify" >/dev/null 2>&1; then
        printf '[graphify] graphifyy is not installed for %s; run uv tool install --upgrade --force graphifyy\n' "$python_path" >&2
        exit 1
    fi

    printf '%s' "$python_path"
}

PYTHON="$(resolve_python)"
WORK_ROOT="$REPO_ROOT/tmp/graphify-scopes/$scope"

rm -rf "$WORK_ROOT"
mkdir -p "$WORK_ROOT/graphify-out"
printf '%s' "$PYTHON" > "$WORK_ROOT/graphify-out/.graphify_python"

say "[graphify] building scope '$scope' in $WORK_ROOT"

REPO_ROOT="$REPO_ROOT" WORK_ROOT="$WORK_ROOT" SCOPE="$scope" GRAPHIFY_SCOPE_QUIET="$quiet" "$PYTHON" - <<'PYCODE'
import json
import os
import re
from pathlib import Path

from graphify.analyze import god_nodes, suggest_questions, surprising_connections
from graphify.build import build_from_json
from graphify.cluster import cluster, score_all
from graphify.detect import detect
from graphify.export import to_html, to_json
from graphify.extract import collect_files, extract
from graphify.report import generate

ROOT = Path(os.environ["REPO_ROOT"])
WORK = Path(os.environ["WORK_ROOT"])
SCOPE = os.environ["SCOPE"]
QUIET = os.environ.get("GRAPHIFY_SCOPE_QUIET", "0") == "1"


def say(message: str) -> None:
    if not QUIET:
        print(message)


def strip_cfg_test_modules(text: str) -> str:
    lines = text.splitlines(keepends=True)
    result: list[str] = []
    index = 0

    while index < len(lines):
        if lines[index].lstrip().startswith("#[cfg(test)]"):
            next_index = index + 1
            while next_index < len(lines) and not lines[next_index].strip():
                next_index += 1
            if next_index < len(lines) and re.match(r"\s*mod\s+tests\s*\{", lines[next_index]):
                depth = 0
                cursor = next_index
                while cursor < len(lines):
                    depth += lines[cursor].count("{")
                    depth -= lines[cursor].count("}")
                    cursor += 1
                    if depth == 0:
                        break
                index = cursor
                while index < len(lines) and not lines[index].strip():
                    index += 1
                continue

        result.append(lines[index])
        index += 1

    return "".join(result)


def sanitize_tui_core_file(original_path: Path, input_root: Path) -> str:
    text = original_path.read_text()
    text = strip_cfg_test_modules(text)

    if original_path.relative_to(input_root).as_posix() == "lib.rs":
        sanitized_lines: list[str] = []
        for line in text.splitlines(keepends=True):
            stripped = line.strip()
            if stripped == "pub mod mock;":
                continue
            if stripped == "pub use mock::{MockRuntime, SessionRuntime};":
                sanitized_lines.append("pub use mock::SessionRuntime;\n")
                continue
            sanitized_lines.append(line)
        text = "".join(sanitized_lines)

    return text


def rewrite_projection_source_files(payload: dict, file_map: dict[str, str]) -> None:
    for key in ("nodes", "edges", "hyperedges"):
        for item in payload.get(key, []):
            source_file = item.get("source_file")
            if source_file in file_map:
                item["source_file"] = file_map[source_file]

            source_files = item.get("source_files")
            if isinstance(source_files, list):
                item["source_files"] = [file_map.get(path, path) for path in source_files]


scope_inputs = {
    "ozone-src": ROOT / "src",
    "ozone-tui": ROOT / "crates/ozone-tui/src",
    "ozone-memory": ROOT / "crates/ozone-memory/src",
}

projection_map: dict[str, str] = {}

if SCOPE == "ozone-tui-core":
    input_root = ROOT / "crates/ozone-tui/src"
    selected_files = [
        input_root / "lib.rs",
        input_root / "layout.rs",
        input_root / "render/coordinator.rs",
    ]
    projection_root = WORK / "projection"

    for original_path in selected_files:
        relative_path = original_path.relative_to(input_root)
        projected_path = projection_root / relative_path
        projected_path.parent.mkdir(parents=True, exist_ok=True)
        projected_path.write_text(sanitize_tui_core_file(original_path, input_root))
        projection_map[str(projected_path)] = str(original_path)

    code_files = [Path(path) for path in projection_map]
    detection = {
        "total_files": len(code_files),
        "total_words": sum(len(Path(path).read_text().split()) for path in projection_map),
        "files": {
            "code": [projection_map[str(path)] for path in code_files],
        },
        "projection": {
            "type": "sanitized_files",
            "source_root": str(input_root),
        },
    }
else:
    input_path = scope_inputs[SCOPE]
    detection = detect(input_path)
    code_files = []
    for file_name in detection.get("files", {}).get("code", []):
        path = Path(file_name)
        code_files.extend(collect_files(path) if path.is_dir() else [path])

(WORK / "graphify-out" / ".graphify_detect.json").write_text(json.dumps(detection, indent=2))

if not code_files:
    raise SystemExit("[graphify] no code files found for scoped graph build")

extraction = extract(code_files)
if projection_map:
    rewrite_projection_source_files(extraction, projection_map)

(WORK / "graphify-out" / ".graphify_extract.json").write_text(json.dumps(extraction, indent=2))

graph = build_from_json(extraction)
communities = cluster(graph)
cohesion = score_all(graph, communities)
gods = god_nodes(graph)
surprises = surprising_connections(graph, communities)
labels = {community_id: f"Community {community_id}" for community_id in communities}
questions = suggest_questions(graph, communities, labels)

report = generate(
    graph,
    communities,
    cohesion,
    labels,
    gods,
    surprises,
    detection,
    {"input": extraction.get("input_tokens", 0), "output": extraction.get("output_tokens", 0)},
    str(scope_inputs.get(SCOPE, ROOT / "crates/ozone-tui/src")),
    suggested_questions=questions,
)

(WORK / "graphify-out" / "GRAPH_REPORT.md").write_text(report)
to_json(graph, communities, str(WORK / "graphify-out" / "graph.json"))

analysis = {
    "communities": {str(key): value for key, value in communities.items()},
    "cohesion": {str(key): value for key, value in cohesion.items()},
    "gods": gods,
    "surprises": surprises,
    "questions": questions,
}
(WORK / "graphify-out" / ".graphify_analysis.json").write_text(json.dumps(analysis, indent=2))

if graph.number_of_nodes() <= 5000:
    to_html(graph, communities, str(WORK / "graphify-out" / "graph.html"))

say(f"[graphify] nodes={graph.number_of_nodes()} edges={graph.number_of_edges()} communities={len(communities)}")
say(f"[graphify] top god nodes: {', '.join(item['label'] for item in gods[:5])}")
say(f"[graphify] report: {WORK / 'graphify-out' / 'GRAPH_REPORT.md'}")
PYCODE