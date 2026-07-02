#!/usr/bin/env python3
"""Generate static HTML leaderboard from eval results.

Scans results/<output_dir>/<model>/ directories for CSV files,
parses scores, and writes results/leaderboard.html as a single
self-contained file (inline CSS + JS, zero external dependencies).

Usage:
    python generate_leaderboard.py                 # default
    python generate_leaderboard.py --output /path   # custom output
"""

import argparse
import csv
import json
import os
import re
import sys
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path

# ── Data definitions ──────────────────────────────────────────────────────────

# Model metadata keys we might find in results directories or catalog JSON.
# Scored columns: each key is a preset CLI name, value is display label.
SCORE_COLUMNS = [
    ("baseline", "Baseline", [
        ("hellaswag", "HS"),
        ("arc_challenge", "ARC"),
        ("bbh_formal_fallacies", "FF"),
        ("bbh_causal_judgement", "CJ"),
    ]),
    ("general", "General", [
        ("mmlu", "MLU"),
        ("mmlu_pro", "MLP"),
    ]),
    ("philosophy-ethics", "Philosophy & Ethics", [
        ("mmlu_philosophy", "Phil"),
        ("hendrycks_ethics", "Eth"),
        ("bbh_formal_fallacies", "FF"),
        ("bbh_causal_judgement", "CJ"),
    ]),
    ("reasoning", "Reasoning", [
        ("bbh", "BBH"),
        ("drop", "DROP"),
    ]),
    ("math", "Math", [
        ("gsm8k", "GSM"),
        ("math", "MATH"),
    ]),
    ("coding", "Coding", [
        ("mbpp", "MBPP"),
    ]),
    ("safety", "Safety", [
        ("truthfulqa", "TQA"),
        ("instruction", "IFE"),
    ]),
    ("hard", "Hard", [
        ("gpqa", "GPQA"),
    ]),
]

SUITE_ORDER = ["baseline", "general", "philosophy-ethics", "reasoning",
               "math", "coding", "safety", "hard"]

QUANT_COLORS = {
    "q2_k":  "#ef4444",  # red (lowest quality)
    "q3_k":  "#e67e22",  # orange
    "q4_k":  "#f59e0b",  # amber
    "q5_k":  "#8dd6d1",  # cyan
    "q6_k":  "#22c55e",  # lime/green
    "q8_0":  "#7c3aed",  # violet (premium)
    "f16":   "#7c3aed",  # violet (premium)
    "iq4":   "#f59e0b",  # amber (IQ4_XS etc.)
    "iq3":   "#e67e22",  # orange
    "iq2":   "#ef4444",  # red
}


# ── Result parsing ────────────────────────────────────────────────────────────

def find_score_files(root: str) -> list[Path]:
    """Find all result CSV files under root/results/<output_dir>/<model>/."""
    results_dir = Path(root) / "results"
    if not results_dir.exists():
        return []
    return sorted(results_dir.rglob("results_*.csv"))


def parse_result_csv(path: Path) -> dict:
    """Parse a single result CSV into {metric_name: float_value}."""
    scores = {}
    with open(path, newline="") as f:
        reader = csv.DictReader(f)
        for row in reader:
            for key, val in row.items():
                try:
                    scores[key.strip()] = float(val.strip())
                except (ValueError, AttributeError):
                    pass
    return scores


def infer_model_name(file_path: Path, root: str) -> str:
    """Infer model name from path: results/<output_dir>/<model>/..."""
    # Path structure: results/<dir>/<model>/results_*.csv
    parts = file_path.relative_to(Path(root) / "results").parts
    if len(parts) >= 2:
        return parts[1]  # <model>
    return file_path.parent.name


def infer_quant(model_name: str) -> str | None:
    """Guess quantization from model filename stem."""
    model_lower = model_name.lower()
    for q in ["iq4_xs", "iq3_xxs", "iq2_xs", "q4_k_m", "q5_k_m",
              "q6_k", "q8_0", "f16", "q4_k", "q5_k", "q3_k", "q2_k"]:
        if q in model_lower:
            return q.upper()
    return None


def collect_data(root: str) -> tuple[dict, dict]:
    """Return (models, scores) where:
    - models: {model_name: {quant: str, size: str}}
    - scores: {model_name: {preset: float}}
    """
    csv_files = find_score_files(root)
    models = {}
    scores = defaultdict(dict)

    for fp in csv_files:
        model_name = infer_model_name(fp, root)
        result = parse_result_csv(fp)
        if model_name not in models:
            quant = infer_quant(model_name)
            models[model_name] = {"quant": quant}

        for preset_name, _ in [(p, d) for suite in SCORE_COLUMNS for p, d, _ in [suite]]:
            task_name = f"lm_eval_{preset_name}" if "lm_eval_" not in preset_name else preset_name
            for key, val in result.items():
                # Match metric key patterns: preset.acc, preset.em, etc.
                if key.startswith(preset_name + ".") or key.startswith(f"results.{preset_name}."):
                    scores[model_name][preset_name] = val
                    break

    return models, dict(scores)


# ── HTML generation ───────────────────────────────────────────────────────────

CSS = """/* Ozone Leaderboard — TUI-branded theme */
* { box-sizing: border-box; margin: 0; padding: 0; }
body { font-family: 'Inter', 'Segoe UI', system-ui, -apple-system, sans-serif;
       background: #0a0a12; color: #e0e0e0; padding: 24px; }
/* ── Ozone brand palette ────────────────────────────────────────
   lime/green:  #22c55e   (primary accent)
   violet:      #7c3aed   (secondary accent)
   cyan/teal:   #8dd6d1   (muted text, secondary)
   amber:       #f59e0b   (warnings, medium scores)
   red:         #ef4444   (errors, low scores)
   dark bg:     #0a0a12   (near black)
   panel bg:    #12121e   (card/tile background)                */
body.light { background: #f0f0f5; color: #333; }
.hero { text-align: center; padding: 40px 16px 28px;
        background: linear-gradient(135deg, #0f0f1e 0%, #12122a 50%, #0a0a12 100%);
        border-radius: 12px; margin-bottom: 20px; position: relative;
        border: 1px solid #1a1a30; overflow: hidden; }
.hero::before { content: ''; position: absolute; top: -50%; left: -50%;
                width: 200%; height: 200%;
                background: radial-gradient(circle at 30% 50%, rgba(124,58,237,0.04) 0%, transparent 50%),
                            radial-gradient(circle at 70% 50%, rgba(34,197,94,0.03) 0%, transparent 50%); }
body.light .hero { background: linear-gradient(135deg, #e8e8f5 0%, #f0f0f8 50%, #f8f8fc 100%);
                   border-color: #ddd; }
.hero h1 { font-size: 28px; font-weight: 700; letter-spacing: 1px;
           color: #fff; position: relative; }
body.light .hero h1 { color: #222; }
.hero .ascii-logo { font-family: 'JetBrains Mono', 'Cascadia Code', 'Consolas', monospace;
                    font-size: 11px; line-height: 1.2;
                    color: #22c55e; opacity: 0.7; white-space: pre; margin-bottom: 10px;
                    position: relative; }
body.light .hero .ascii-logo { color: #22c55e; opacity: 0.5; }
.hero .stats { color: #8dd6d1; font-size: 14px; margin-top: 8px; position: relative; }
.hero .timestamp { color: #555; font-size: 12px; margin-top: 4px; position: relative; }
.controls { display: flex; gap: 12px; align-items: center; flex-wrap: wrap;
            margin-bottom: 16px; }
.controls input, .controls select, .controls button {
    padding: 6px 12px; border-radius: 6px; border: 1px solid #1e1e3a;
    background: #12121e; color: #e0e0e0; font-size: 13px; outline: none; }
.controls input:focus, .controls select:focus, .controls button:focus {
    border-color: #22c55e; box-shadow: 0 0 0 1px rgba(34,197,94,0.2); }
body.light .controls input, body.light .controls select,
body.light .controls button { background: #fff; color: #333; border-color: #ccc; }
.controls input { flex: 1; min-width: 180px; }
.controls label { font-size: 13px; color: #888; display: flex; align-items: center; gap: 4px; }
.theme-btn { cursor: pointer; padding: 6px 14px !important; }
.theme-btn:hover { border-color: #7c3aed !important; }
.legend { display: flex; gap: 16px; margin-bottom: 12px; font-size: 12px; color: #8dd6d1; }
.legend-item { display: flex; align-items: center; gap: 4px; }
.legend-swatch { width: 20px; height: 12px; border-radius: 2px; display: inline-block; }
.table-wrap { overflow-x: auto; border-radius: 8px; border: 1px solid #1e1e30; }
body.light .table-wrap { border-color: #ddd; }
table { width: 100%; border-collapse: collapse; font-size: 13px; }
thead { position: sticky; top: 0; z-index: 10; }
thead th { background: #12121e; padding: 8px 10px; text-align: center;
           font-weight: 600; text-transform: uppercase; letter-spacing: 0.5px;
           border-bottom: 2px solid #1e1e30; white-space: nowrap; color: #c0c0e0; }
body.light thead th { background: #e8e8f5; border-bottom-color: #ccc; color: #444; }
thead th.suite-header { background: #0e0e18; font-size: 11px; color: #7c3aed; letter-spacing: 1px; }
body.light thead th.suite-header { background: #ddddee; color: #7c3aed; }
tbody tr { border-bottom: 1px solid #141422; transition: background 0.15s; }
body.light tbody tr { border-bottom-color: #e8e8ee; }
tbody tr:hover { background: #14142a; }
body.light tbody tr:hover { background: #e8e8f5; }
tbody td { padding: 8px 10px; text-align: center; white-space: nowrap; }
tbody td.left { text-align: left; }
.model-name { font-weight: 600; color: #e0e0f0; }
body.light .model-name { color: #333; }
.quant-badge { display: inline-block; padding: 2px 8px; border-radius: 10px;
               font-size: 10px; font-weight: 600; color: #fff;
               border: 1px solid rgba(255,255,255,0.08); }
.score-cell { position: relative; }
.score-bar { position: absolute; left: 0; top: 0; height: 100%;
             opacity: 0.08; border-radius: 0; }
.score-val { position: relative; z-index: 1; font-family: 'JetBrains Mono',
             'Cascadia Code', 'Consolas', monospace;
             font-variant-numeric: tabular-nums; font-weight: 500; }
.score-best { font-weight: 700; }
.score-best .score-val { color: #22c55e !important; text-shadow: 0 0 8px rgba(34,197,94,0.3); }
.score-best::after { content: ' \\u2605'; color: #22c55e; font-size: 10px; vertical-align: super; }
.score-green { color: #22c55e; }
.score-yellow { color: #f59e0b; }
.score-orange { color: #e67e22; }
.score-red { color: #ef4444; }
.score-none { color: #444; font-style: italic; }
body.light .score-none { color: #bbb; }
.cell-highlight { background: rgba(34,197,94,0.04); }
td.meta { font-size: 12px; color: #8dd6d1; }
.export-bar { margin-top: 16px; display: flex; gap: 8px; justify-content: center; }
.export-btn { padding: 6px 16px; border-radius: 6px; border: 1px solid #1e1e3a;
              background: #12121e; color: #e0e0e0; cursor: pointer; font-size: 13px; }
body.light .export-btn { background: #fff; border-color: #ccc; color: #333; }
.export-btn:hover { background: #1a1a30; border-color: #22c55e; }
body.light .export-btn:hover { background: #e8e8f5; border-color: #22c55e; }
.footer { text-align: center; margin-top: 20px; font-size: 12px; color: #555; }
@media (max-width: 768px) {
    thead th, tbody td { padding: 4px 6px; font-size: 11px; }
    .hero h1 { font-size: 20px; }
}
"""


def score_color(val: float) -> str:
    if val >= 80:
        return "score-green"
    if val >= 50:
        return "score-yellow"
    if val >= 20:
        return "score-orange"
    return "score-red"


def quant_bg(quant: str | None) -> str:
    if quant is None:
        return "#555"
    for key, color in QUANT_COLORS.items():
        if key in quant.lower().replace("_", ""):
            return color
    return "#555"


def build_ascii_logo() -> str:
    return r"""
   ___  _____  _____  _   _  _____
  / _ \|  _  ||  _  || \ | ||  ___|
 / /_\ \| | | || | | ||  \| ||___ \
 |  _  || | | || | | || . ` |    \ \
 | | | |\ \_/ /\ \_/ /| |\  |/\__/ /
 \_| |_/ \___/  \___/ \_| \_/\____/
    """


def generate_html(models: dict, scores: dict, output_path: str, root: str):
    """Write leaderboard.html to output_path."""

    # Collect all preset names in order
    all_presets = []
    for suite_name, suite_label, cols in SCORE_COLUMNS:
        for preset_name, short_label in cols:
            all_presets.append(preset_name)

    # Build table rows
    model_names = sorted(models.keys(), key=str.lower)
    all_scores_by_preset = {p: [] for p in all_presets}

    rows_html = ""
    for idx, mname in enumerate(model_names):
        quant = models[mname].get("quant")
        q_bg = quant_bg(quant)
        row_class = "row-alt" if idx % 2 == 1 else ""

        cells = f'<td class="left model-name">{mname}</td>'
        cells += f'<td class="meta"><span class="quant-badge" style="background:{q_bg}">{quant or "--"}</span></td>'

        for preset_name in all_presets:
            val = scores.get(mname, {}).get(preset_name)
            if val is not None:
                pct = round(val * 100, 1)
                cls = score_color(pct)
                bar_pct = min(pct, 100)
                # Track for best-score detection
                all_scores_by_preset[preset_name].append((pct, mname))
                cells += (f'<td class="score-cell {cls}">'
                          f'<div class="score-bar" style="width:{bar_pct}%"></div>'
                          f'<span class="score-val">{pct:.0f}</span></td>')
            else:
                cells += '<td class="score-none">--</td>'

        rows_html += f'<tr class="{row_class}">{cells}</tr>'

    # Determine best scores
    best_scores = {}
    for p, lst in all_scores_by_preset.items():
        if lst:
            best_pct = max(lst, key=lambda x: x[0])
            best_scores[p] = best_pct[1]  # model name

    # Re-render with best-score highlighting
    rows_html = ""
    for idx, mname in enumerate(model_names):
        quant = models[mname].get("quant")
        q_bg = quant_bg(quant)
        row_style = f'background: #18182e;' if idx % 2 == 1 else ''
        row_style_light = f'background: #fafafa;' if idx % 2 == 1 else ''

        cells = f'<td class="left model-name">{mname}</td>'
        cells += f'<td class="meta"><span class="quant-badge" style="background:{q_bg}">{quant or "--"}</span></td>'

        for preset_name in all_presets:
            val = scores.get(mname, {}).get(preset_name)
            if val is not None:
                pct = round(val * 100, 1)
                cls = score_color(pct)
                bar_pct = min(pct, 100)
                is_best = (best_scores.get(preset_name) == mname)
                best_cls = "score-best cell-highlight" if is_best else ""
                star = " ★" if is_best else ""
                cells += (f'<td class="score-cell {cls} {best_cls}">'
                          f'<div class="score-bar" style="width:{bar_pct}%"></div>'
                          f'<span class="score-val">{pct:.0f}{star}</span></td>')
            else:
                cells += '<td class="score-none">--</td>'

        rows_html += f'<tr style="{row_style}" data-light="{row_style_light}">{cells}</tr>'

    # Suite header row
    suite_cells = '<th colspan="2" style="text-align:left">Model</th>'
    for suite_name, suite_label, cols in SCORE_COLUMNS:
        span = len(cols)
        suite_cells += f'<th class="suite-header" colspan="{span}">{suite_label}</th>'
    suite_header = f'<tr>{suite_cells}</tr>'

    # Score column headers
    col_cells = '<th style="text-align:left;min-width:140px">Model</th><th style="min-width:60px">Quant</th>'
    for suite_name, suite_label, cols in SCORE_COLUMNS:
        for preset_name, short_label in cols:
            col_cells += f'<th title="{preset_name}" onclick="sortTable({list(all_presets).index(preset_name) + 2})">{short_label}</th>'
    col_header = f'<tr>{col_cells}</tr>'

    timestamp = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
    num_models = len(model_names)
    total_suites = len(SUITE_ORDER)
    total_tasks = len(all_presets)

    html = f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Ozone Model Leaderboard</title>
<style>{CSS}</style>
</head>
<body>
<div class="hero">
<div class="ascii-logo">{build_ascii_logo()}</div>
<h1>Model Leaderboard</h1>
<div class="stats">{num_models} models &middot; {total_suites} suites &middot; {total_tasks} tasks</div>
<div class="timestamp">Generated {timestamp}</div>
</div>

<div class="controls">
<input type="text" id="filter" placeholder="Filter models..." oninput="filterTable()">
<label><input type="checkbox" id="completeOnly" onchange="filterTable()"> Show only complete</label>
<select id="sortSelect" onchange="sortTable(-1)">
<option value="name">Sort: Name</option>
<option value="score">Sort: Avg Score</option>
<option value="speed">Sort: Speed</option>
</select>
<button class="theme-btn" onclick="toggleTheme()">{chr(0x2600)} Dark</button>
</div>

<div class="legend">
<span class="legend-item"><span class="legend-swatch" style="background:#2ecc71"></span> &ge;80%</span>
<span class="legend-item"><span class="legend-swatch" style="background:#f1c40f"></span> 50-79%</span>
<span class="legend-item"><span class="legend-swatch" style="background:#e67e22"></span> 20-49%</span>
<span class="legend-item"><span class="legend-swatch" style="background:#e74c3c"></span> &lt;20%</span>
<span class="legend-item"><span class="legend-swatch" style="background:#555"></span> not run</span>
</div>

<div class="table-wrap">
<table id="leaderboard">
<thead>{suite_header}{col_header}</thead>
<tbody id="tbody">{rows_html}</tbody>
</table>
</div>

<div class="export-bar">
<button class="export-btn" onclick="copyCSV()">{chr(0x1F4CB)} Copy CSV</button>
<button class="export-btn" onclick="downloadCSV()">{chr(0x1F4E5)} Download CSV</button>
<button class="export-btn" onclick="window.print()">{chr(0x1F5A8)} Print</button>
</div>

<div class="footer">Ozone eval results &mdash; scores are percentages 0-100</div>

<script>
let theme = 'dark';

function toggleTheme() {{
    document.body.classList.toggle('light');
    theme = document.body.classList.contains('light') ? 'light' : 'dark';
    document.querySelector('.theme-btn').textContent = theme === 'dark'
        ? '{chr(0x2600)} Dark'
        : '{chr(0x1F319)} Light';
}}

function filterTable() {{
    const input = document.getElementById('filter').value.toLowerCase();
    const completeOnly = document.getElementById('completeOnly').checked;
    const rows = document.querySelectorAll('#tbody tr');
    let visible = 0;
    rows.forEach(row => {{
        const name = row.cells[0].textContent.toLowerCase();
        const scores = Array.from(row.cells).slice(2);
        const allScored = scores.every(c => !c.classList.contains('score-none'));
        const match = name.includes(input);
        const completeOk = !completeOnly || allScored;
        row.style.display = (match && completeOk) ? '' : 'none';
        if (match && completeOk) visible++;
    }});
}}

function sortTable(colIdx) {{
    const tbody = document.getElementById('tbody');
    const rows = Array.from(tbody.querySelectorAll('tr'));
    const isNumeric = colIdx >= 0;

    rows.sort((a, b) => {{
        if (isNumeric) {{
            const va = parseFloat(a.cells[colIdx]?.textContent.trim()) || -1;
            const vb = parseFloat(b.cells[colIdx]?.textContent.trim()) || -1;
            return vb - va;  // descending
        }}
        const sel = document.getElementById('sortSelect').value;
        if (sel === 'name') {{
            return a.cells[0].textContent.localeCompare(b.cells[0].textContent);
        }}
        if (sel === 'score') {{
            let sa = 0, ca = 0, sb = 0, cb = 0;
            Array.from(a.cells).slice(2).forEach(c => {{
                const v = parseFloat(c.textContent);
                if (!isNaN(v)) {{ sa += v; ca++; }}
            }});
            Array.from(b.cells).slice(2).forEach(c => {{
                const v = parseFloat(c.textContent);
                if (!isNaN(v)) {{ sb += v; cb++; }}
            }});
            const avgA = ca > 0 ? sa/ca : 0;
            const avgB = cb > 0 ? sb/cb : 0;
            return avgB - avgA;
        }}
        return 0;
    }});

    rows.forEach(r => tbody.appendChild(r));
}}

function getCSV() {{
    const rows = document.querySelectorAll('#tbody tr');
    const headers = ['Model', 'Quant'];
    document.querySelectorAll('#leaderboard thead tr:last-child th').forEach(th => {{
        if (th.title) headers.push(th.title);
    }});
    let csv = headers.join(',') + '\\n';
    rows.forEach(row => {{
        if (row.style.display === 'none') return;
        const cols = Array.from(row.cells);
        const vals = [cols[0].textContent.trim(), cols[1].textContent.replace(/[^A-Za-z0-9_]/g,'')];
        cols.slice(2).forEach(c => {{
            const v = c.textContent.trim();
            vals.push(v === '--' ? '' : v);
        }});
        csv += vals.join(',') + '\\n';
    }});
    return csv;
}}

function copyCSV() {{
    navigator.clipboard.writeText(getCSV()).then(() => {{
        alert('CSV copied to clipboard');
    }});
}}

function downloadCSV() {{
    const blob = new Blob([getCSV()], {{type: 'text/csv'}});
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url; a.download = 'ozone_leaderboard.csv';
    a.click(); URL.revokeObjectURL(url);
}}
</script>
</body>
</html>"""

    Path(output_path).parent.mkdir(parents=True, exist_ok=True)
    with open(output_path, "w") as f:
        f.write(html)
    print(f"Leaderboard written to {output_path}")


# ── Main ─────────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(
        description="Generate HTML leaderboard from ozone eval results")
    parser.add_argument("--output", default=None,
                        help="Output path (default: results/leaderboard.html)")
    parser.add_argument("--root", default=".",
                        help="Project root (default: current directory)")
    args = parser.parse_args()

    root = Path(args.root).resolve()
    output = Path(args.output or root / "results" / "leaderboard.html")

    models, scores = collect_data(str(root))

    if not models:
        print("No eval results found. Run some evals first:", file=sys.stderr)
        print("  python ozone_eval.py <model.gguf> --presets [preset...]", file=sys.stderr)
        sys.exit(1)

    generate_html(models, scores, str(output), str(root))


if __name__ == "__main__":
    main()
