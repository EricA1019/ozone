use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::testing::types::BaselineCompareDiff;
use crate::testing::types::{
    ComparableScreenCell, PtyVteCaptureCell, PtyVteCaptureResult, PtyVteCaptureRow,
    ScreenCheckOutcome, ScreenColorMatch, ScreenRegion,
};

// =============================================================================
// Screen Capture Loading
// =============================================================================

pub fn load_screen_capture_sidecar(artifact_path: &str) -> Result<(PathBuf, PtyVteCaptureResult)> {
    let requested_path = PathBuf::from(artifact_path);
    if requested_path.is_dir() {
        bail!(
            "screen capture path `{}` is a directory; provide a JSON sidecar or matching PNG path",
            requested_path.display()
        );
    }

    let sidecar_path = if requested_path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("json"))
    {
        requested_path.clone()
    } else {
        requested_path.with_extension("json")
    };

    if !sidecar_path.exists() {
        if requested_path == sidecar_path {
            bail!(
                "screen capture sidecar `{}` does not exist",
                sidecar_path.display()
            );
        }
        bail!(
            "screen capture sidecar `{}` does not exist for artifact `{}`",
            sidecar_path.display(),
            requested_path.display()
        );
    }

    let sidecar_text = std::fs::read_to_string(&sidecar_path).with_context(|| {
        format!(
            "failed to read screen capture sidecar {}",
            sidecar_path.display()
        )
    })?;
    let mut capture: PtyVteCaptureResult =
        serde_json::from_str(&sidecar_text).with_context(|| {
            format!(
                "screen capture sidecar {} is not valid JSON",
                sidecar_path.display()
            )
        })?;
    if capture.rows.is_empty() && !capture.grid.is_empty() {
        capture.rows = capture.grid.clone();
    }
    if capture.rows.is_empty() {
        bail!(
            "screen capture sidecar `{}` is missing screen-grid rows",
            sidecar_path.display()
        );
    }
    if capture.rows.iter().any(|row| row.cells.is_empty()) {
        bail!(
            "screen capture sidecar `{}` is missing screen-grid cells",
            sidecar_path.display()
        );
    }

    Ok((sidecar_path, capture))
}

// =============================================================================
// Screen Check Evaluation
// =============================================================================

const DEFAULT_BORDER_MAX_BLANK_RUN: usize = 1;
const DEFAULT_LAYOUT_MIN_GAP: usize = 2;

pub fn evaluate_screen_check(
    index: usize,
    check: &Value,
    capture: &PtyVteCaptureResult,
) -> Result<ScreenCheckOutcome> {
    let object = check
        .as_object()
        .ok_or_else(|| anyhow!("check #{} must be an object", index + 1))?;
    let check_type = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("check #{} is missing string field `type`", index + 1))?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);

    match check_type {
        "text_present" => evaluate_text_present_check(index, name, object, capture),
        "text_absent" => evaluate_text_absent_check(index, name, object, capture),
        "color_at" => evaluate_color_at_check(index, name, object, capture),
        "border_intact" => evaluate_border_intact_check(index, name, object, capture),
        "layout_columns" => evaluate_layout_columns_check(index, name, object, capture),
        "no_overlap" => evaluate_no_overlap_check(index, name, object, capture),
        "baseline_compare" => evaluate_baseline_compare_check(index, name, object, capture),
        other => bail!(
            "check #{} has unsupported type `{other}`; supported types: text_present, text_absent, color_at, border_intact, layout_columns, no_overlap, baseline_compare",
            index + 1
        ),
    }
}

fn evaluate_text_present_check(
    index: usize,
    name: Option<String>,
    check: &Map<String, Value>,
    capture: &PtyVteCaptureResult,
) -> Result<ScreenCheckOutcome> {
    let text = required_check_string(check, "text", index)?;
    if text.is_empty() {
        bail!("check #{} field `text` must not be empty", index + 1);
    }
    let case_sensitive = check
        .get("caseSensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let min_occurrences = optional_check_usize(check, "minOccurrences", index)?.unwrap_or(1);
    let region = region_from_check(check, capture, index, false, "text_present")?;
    let matches = text_matches(capture, region, &text, case_sensitive)?;
    let occurrences = matches.iter().map(|(_, count)| *count).sum::<usize>();
    let passed = occurrences >= min_occurrences;
    let summary = if passed {
        format!(
            "Found `{text}` {occurrences} time(s) in region {}",
            format_region(region)
        )
    } else {
        format!(
            "Expected `{text}` at least {min_occurrences} time(s) in region {}, found {occurrences}",
            format_region(region)
        )
    };

    Ok(ScreenCheckOutcome {
        index,
        check_type: "text_present".to_owned(),
        name,
        passed,
        summary,
        detail: json!({
            "text": text,
            "caseSensitive": case_sensitive,
            "region": region,
            "minOccurrences": min_occurrences,
            "occurrences": occurrences,
            "matches": matches.iter().map(|(row, count)| json!({ "row": row, "count": count })).collect::<Vec<_>>()
        }),
    })
}

fn evaluate_text_absent_check(
    index: usize,
    name: Option<String>,
    check: &Map<String, Value>,
    capture: &PtyVteCaptureResult,
) -> Result<ScreenCheckOutcome> {
    let text = required_check_string(check, "text", index)?;
    if text.is_empty() {
        bail!("check #{} field `text` must not be empty", index + 1);
    }
    let case_sensitive = check
        .get("caseSensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let region = region_from_check(check, capture, index, false, "text_absent")?;
    let matches = text_matches(capture, region, &text, case_sensitive)?;
    let occurrences = matches.iter().map(|(_, count)| *count).sum::<usize>();
    let passed = occurrences == 0;
    let summary = if passed {
        format!(
            "Confirmed `{text}` is absent from region {}",
            format_region(region)
        )
    } else {
        format!(
            "Expected `{text}` to be absent from region {}, found {occurrences} time(s)",
            format_region(region)
        )
    };

    Ok(ScreenCheckOutcome {
        index,
        check_type: "text_absent".to_owned(),
        name,
        passed,
        summary,
        detail: json!({
            "text": text,
            "caseSensitive": case_sensitive,
            "region": region,
            "occurrences": occurrences,
            "matches": matches.iter().map(|(row, count)| json!({ "row": row, "count": count })).collect::<Vec<_>>()
        }),
    })
}

fn evaluate_color_at_check(
    index: usize,
    name: Option<String>,
    check: &Map<String, Value>,
    capture: &PtyVteCaptureResult,
) -> Result<ScreenCheckOutcome> {
    let row = required_check_u16(check, "row", index)?;
    let column = required_check_u16(check, "column", index)?;
    let fg = check.get("fg");
    let bg = check.get("bg");
    if fg.is_none() && bg.is_none() {
        bail!(
            "check #{} `color_at` requires `fg`, `bg`, or both",
            index + 1
        );
    }

    let cell = capture_cell(capture, row, column)?;
    let actual_fg = ScreenColorMatch {
        raw: &cell.fg,
        resolved: &cell.resolved_fg,
    };
    let actual_bg = ScreenColorMatch {
        raw: &cell.bg,
        resolved: &cell.resolved_bg,
    };
    let fg_passed = fg
        .map(|expected| color_matches(expected, actual_fg, "fg", index))
        .transpose()?
        .unwrap_or(true);
    let bg_passed = bg
        .map(|expected| color_matches(expected, actual_bg, "bg", index))
        .transpose()?
        .unwrap_or(true);
    let passed = fg_passed && bg_passed;
    let summary = if passed {
        format!("Color check passed at row {row}, column {column}")
    } else {
        format!("Color check failed at row {row}, column {column}")
    };

    Ok(ScreenCheckOutcome {
        index,
        check_type: "color_at".to_owned(),
        name,
        passed,
        summary,
        detail: json!({
            "row": row,
            "column": column,
            "cellText": cell.text,
            "expected": {
                "fg": fg.cloned(),
                "bg": bg.cloned()
            },
            "actual": {
                "fg": { "raw": cell.fg, "resolved": cell.resolved_fg },
                "bg": { "raw": cell.bg, "resolved": cell.resolved_bg }
            }
        }),
    })
}

fn evaluate_border_intact_check(
    index: usize,
    name: Option<String>,
    check: &Map<String, Value>,
    capture: &PtyVteCaptureResult,
) -> Result<ScreenCheckOutcome> {
    let region = region_from_check(check, capture, index, true, "border_intact")?;
    let width = region_width(region);
    let height = region_height(region);
    if width < 2 || height < 2 {
        bail!(
            "check #{} `border_intact` region {} must be at least 2x2",
            index + 1,
            format_region(region)
        );
    }

    let mut issues = Vec::new();
    let corners = [
        ("topLeft", region.top, region.left),
        ("topRight", region.top, region.right),
        ("bottomLeft", region.bottom, region.left),
        ("bottomRight", region.bottom, region.right),
    ];
    for (label, row, column) in corners {
        if cell_text(capture_cell(capture, row, column)?)
            .trim()
            .is_empty()
        {
            issues.push(
                json!({ "edge": label, "row": row, "column": column, "reason": "corner is blank" }),
            );
        }
    }
    for row in (region.top + 1)..region.bottom {
        if cell_text(capture_cell(capture, row, region.left)?)
            .trim()
            .is_empty()
        {
            issues.push(json!({ "edge": "left", "row": row, "column": region.left, "reason": "left border is blank" }));
        }
        if cell_text(capture_cell(capture, row, region.right)?)
            .trim()
            .is_empty()
        {
            issues.push(json!({ "edge": "right", "row": row, "column": region.right, "reason": "right border is blank" }));
        }
    }

    let top_stats = horizontal_edge_stats(capture, region.top, region.left, region.right)?;
    let bottom_stats = horizontal_edge_stats(capture, region.bottom, region.left, region.right)?;
    if top_stats.max_blank_run > DEFAULT_BORDER_MAX_BLANK_RUN {
        issues.push(json!({
            "edge": "top",
            "reason": format!("top border has blank run {} (> {})", top_stats.max_blank_run, DEFAULT_BORDER_MAX_BLANK_RUN)
        }));
    }
    if bottom_stats.max_blank_run > DEFAULT_BORDER_MAX_BLANK_RUN {
        issues.push(json!({
            "edge": "bottom",
            "reason": format!("bottom border has blank run {} (> {})", bottom_stats.max_blank_run, DEFAULT_BORDER_MAX_BLANK_RUN)
        }));
    }

    let passed = issues.is_empty();
    let summary = if passed {
        format!("Border is intact for region {}", format_region(region))
    } else {
        format!(
            "Border is not intact for region {} ({} issue(s))",
            format_region(region),
            issues.len()
        )
    };

    Ok(ScreenCheckOutcome {
        index,
        check_type: "border_intact".to_owned(),
        name,
        passed,
        summary,
        detail: json!({
            "region": region,
            "topEdge": top_stats,
            "bottomEdge": bottom_stats,
            "issues": issues
        }),
    })
}

fn evaluate_layout_columns_check(
    index: usize,
    name: Option<String>,
    check: &Map<String, Value>,
    capture: &PtyVteCaptureResult,
) -> Result<ScreenCheckOutcome> {
    let count = required_check_usize(check, "count", index)?;
    let min_gap = optional_check_usize(check, "minGap", index)?.unwrap_or(DEFAULT_LAYOUT_MIN_GAP);
    let region = region_from_check(check, capture, index, false, "layout_columns")?;
    let columns = detect_layout_columns(capture, region, min_gap)?;
    let passed = columns.len() == count;
    let summary = if passed {
        format!(
            "Detected {count} layout column(s) in region {}",
            format_region(region)
        )
    } else {
        format!(
            "Expected {count} layout column(s) in region {}, found {}",
            format_region(region),
            columns.len()
        )
    };

    Ok(ScreenCheckOutcome {
        index,
        check_type: "layout_columns".to_owned(),
        name,
        passed,
        summary,
        detail: json!({
            "region": region,
            "count": count,
            "minGap": min_gap,
            "detectedColumns": columns
        }),
    })
}

fn evaluate_no_overlap_check(
    index: usize,
    name: Option<String>,
    check: &Map<String, Value>,
    capture: &PtyVteCaptureResult,
) -> Result<ScreenCheckOutcome> {
    let regions_value = check
        .get("regions")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("check #{} `no_overlap` requires `regions`", index + 1))?;
    if regions_value.len() < 2 {
        bail!(
            "check #{} `no_overlap` requires at least two regions",
            index + 1
        );
    }

    let regions = regions_value
        .iter()
        .enumerate()
        .map(|(region_index, value)| named_region_from_value(value, capture, index, region_index))
        .collect::<Result<Vec<_>>>()?;
    let mut overlaps = Vec::new();
    for (left_index, (left_name, left_region)) in regions.iter().enumerate() {
        for (right_name, right_region) in regions.iter().skip(left_index + 1) {
            if let Some(overlap) = overlapping_region(*left_region, *right_region) {
                overlaps.push(json!({
                    "left": left_name,
                    "right": right_name,
                    "overlap": overlap
                }));
            }
        }
    }

    let passed = overlaps.is_empty();
    let summary = if passed {
        format!("Confirmed {} regions do not overlap", regions.len())
    } else {
        format!("Detected {} overlapping region pair(s)", overlaps.len())
    };

    Ok(ScreenCheckOutcome {
        index,
        check_type: "no_overlap".to_owned(),
        name,
        passed,
        summary,
        detail: json!({
            "regions": regions.iter().map(|(region_name, region)| json!({
                "name": region_name,
                "region": region
            })).collect::<Vec<_>>(),
            "overlaps": overlaps
        }),
    })
}

fn evaluate_baseline_compare_check(
    index: usize,
    name: Option<String>,
    check: &Map<String, Value>,
    capture: &PtyVteCaptureResult,
) -> Result<ScreenCheckOutcome> {
    let baseline_path = check
        .get("baselinePath")
        .or_else(|| check.get("baselineSidecarPath"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            anyhow!(
                "check #{} `baseline_compare` requires `baselinePath` or `baselineSidecarPath`",
                index + 1
            )
        })?;
    let (baseline_sidecar_path, baseline_capture) = load_screen_capture_sidecar(&baseline_path)?;
    let current_cells = comparable_capture_cells(capture);
    let baseline_cells = comparable_capture_cells(&baseline_capture);
    let compared_positions = current_cells
        .keys()
        .chain(baseline_cells.keys())
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut changed_cells = Vec::new();
    let mut sample_diffs = Vec::new();
    let mut row_diff_counts = BTreeMap::new();

    for (row, column) in &compared_positions {
        let actual = current_cells.get(&(*row, *column));
        let baseline = baseline_cells.get(&(*row, *column));
        if actual == baseline {
            continue;
        }

        changed_cells.push(json!({ "row": row, "column": column }));
        *row_diff_counts.entry(*row).or_insert(0_usize) += 1;
        if sample_diffs.len() < 20 {
            sample_diffs.push(BaselineCompareDiff {
                row: *row,
                column: *column,
                kind: baseline_compare_diff_kind(actual, baseline),
                baseline: baseline.cloned(),
                actual: actual.cloned(),
            });
        }
    }

    let diff_count = changed_cells.len();
    let total_cells_compared = compared_positions.len();
    let matched_cells = total_cells_compared.saturating_sub(diff_count);
    let match_percent = if total_cells_compared == 0 {
        100.0
    } else {
        ((matched_cells as f64 / total_cells_compared as f64) * 10_000.0).round() / 100.0
    };
    let dimensions_match = capture.screen_rows == baseline_capture.screen_rows
        && capture.screen_columns == baseline_capture.screen_columns;
    let difference_summary = baseline_difference_summary(
        diff_count,
        &row_diff_counts,
        dimensions_match,
        capture,
        &baseline_capture,
    );
    let passed = diff_count == 0 && dimensions_match;
    let summary = if passed {
        format!(
            "Baseline compare matched {matched_cells}/{total_cells_compared} cells ({match_percent:.2}%)"
        )
    } else if dimensions_match {
        format!(
            "Baseline compare found {diff_count} diff(s) across {total_cells_compared} cells ({match_percent:.2}% match)"
        )
    } else {
        format!(
            "Baseline compare failed: {diff_count} diff(s) and screen dimensions changed (current {}x{}, baseline {}x{})",
            capture.screen_rows,
            capture.screen_columns,
            baseline_capture.screen_rows,
            baseline_capture.screen_columns
        )
    };

    Ok(ScreenCheckOutcome {
        index,
        check_type: "baseline_compare".to_owned(),
        name,
        passed,
        summary,
        detail: json!({
            "baselinePath": baseline_path,
            "baselineSidecarPath": baseline_sidecar_path.display().to_string(),
            "currentScreen": {
                "rows": capture.screen_rows,
                "columns": capture.screen_columns,
                "cellCount": current_cells.len()
            },
            "baselineScreen": {
                "rows": baseline_capture.screen_rows,
                "columns": baseline_capture.screen_columns,
                "cellCount": baseline_cells.len()
            },
            "dimensionsMatch": dimensions_match,
            "changedCells": changed_cells,
            "diffCount": diff_count,
            "matchedCells": matched_cells,
            "totalCellsCompared": total_cells_compared,
            "matchRatio": {
                "matched": matched_cells,
                "total": total_cells_compared
            },
            "matchPercent": match_percent,
            "differenceSummary": difference_summary,
            "rowDiffs": row_diff_counts.iter().map(|(row, count)| json!({
                "row": row,
                "count": count
            })).collect::<Vec<_>>(),
            "sampleDiffs": sample_diffs
        }),
    })
}

// =============================================================================
// Check Helpers
// =============================================================================

fn required_check_string(check: &Map<String, Value>, key: &str, index: usize) -> Result<String> {
    check
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("check #{} is missing string field `{key}`", index + 1))
}

fn required_check_u16(check: &Map<String, Value>, key: &str, index: usize) -> Result<u16> {
    optional_check_u16(check, key, index)?
        .ok_or_else(|| anyhow!("check #{} is missing integer field `{key}`", index + 1))
}

fn required_check_usize(check: &Map<String, Value>, key: &str, index: usize) -> Result<usize> {
    optional_check_usize(check, key, index)?
        .ok_or_else(|| anyhow!("check #{} is missing integer field `{key}`", index + 1))
}

fn optional_check_u16(check: &Map<String, Value>, key: &str, index: usize) -> Result<Option<u16>> {
    match check.get(key) {
        None => Ok(None),
        Some(value) => {
            let raw = value
                .as_u64()
                .ok_or_else(|| anyhow!("check #{} field `{key}` must be an integer", index + 1))?;
            Ok(Some(checked_u16(raw, key)?))
        }
    }
}

fn optional_check_usize(
    check: &Map<String, Value>,
    key: &str,
    index: usize,
) -> Result<Option<usize>> {
    match check.get(key) {
        None => Ok(None),
        Some(value) => {
            let raw = value
                .as_u64()
                .ok_or_else(|| anyhow!("check #{} field `{key}` must be an integer", index + 1))?;
            Ok(Some(checked_usize(raw, key)?))
        }
    }
}

fn region_from_check(
    check: &Map<String, Value>,
    capture: &PtyVteCaptureResult,
    index: usize,
    required: bool,
    check_type: &str,
) -> Result<ScreenRegion> {
    match check.get("region") {
        Some(value) => region_from_value(value, capture, index, "region"),
        None if required => bail!(
            "check #{} `{check_type}` requires a `region` object",
            index + 1
        ),
        None => {
            let row = optional_check_u16(check, "row", index)?;
            Ok(ScreenRegion {
                top: row.unwrap_or(0),
                bottom: row.unwrap_or_else(|| capture.screen_rows.saturating_sub(1)),
                left: 0,
                right: capture.screen_columns.saturating_sub(1),
            })
        }
    }
}

fn region_from_value(
    value: &Value,
    capture: &PtyVteCaptureResult,
    index: usize,
    label: &str,
) -> Result<ScreenRegion> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("check #{} field `{label}` must be an object", index + 1))?;
    let max_row = capture.screen_rows.saturating_sub(1);
    let max_column = capture.screen_columns.saturating_sub(1);
    let top = region_bound(object, "top", 0, max_row, index, label)?;
    let left = region_bound(object, "left", 0, max_column, index, label)?;
    let bottom = region_bound(object, "bottom", max_row, max_row, index, label)?;
    let right = region_bound(object, "right", max_column, max_column, index, label)?;
    if top > bottom {
        bail!(
            "check #{} field `{label}` has top {} greater than bottom {}",
            index + 1,
            top,
            bottom
        );
    }
    if left > right {
        bail!(
            "check #{} field `{label}` has left {} greater than right {}",
            index + 1,
            left,
            right
        );
    }
    Ok(ScreenRegion {
        top,
        left,
        bottom,
        right,
    })
}

fn region_bound(
    object: &Map<String, Value>,
    key: &str,
    default: u16,
    max: u16,
    index: usize,
    label: &str,
) -> Result<u16> {
    let Some(value) = object.get(key) else {
        return Ok(default);
    };
    let raw = value.as_u64().ok_or_else(|| {
        anyhow!(
            "check #{} field `{label}.{key}` must be an integer",
            index + 1
        )
    })?;
    let parsed = checked_u16(raw, key)?;
    if parsed > max {
        bail!(
            "check #{} field `{label}.{key}`={} is outside the screen bounds (max {})",
            index + 1,
            parsed,
            max
        );
    }
    Ok(parsed)
}

fn named_region_from_value(
    value: &Value,
    capture: &PtyVteCaptureResult,
    check_index: usize,
    region_index: usize,
) -> Result<(String, ScreenRegion)> {
    let object = value.as_object().ok_or_else(|| {
        anyhow!(
            "check #{} `regions[{}]` must be an object",
            check_index + 1,
            region_index
        )
    })?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("region_{}", region_index + 1));
    Ok((
        name,
        region_from_value(
            value,
            capture,
            check_index,
            &format!("regions[{region_index}]"),
        )?,
    ))
}

fn format_region(region: ScreenRegion) -> String {
    format!(
        "[top={}, left={}, bottom={}, right={}]",
        region.top, region.left, region.bottom, region.right
    )
}

fn region_width(region: ScreenRegion) -> usize {
    usize::from(region.right - region.left + 1)
}

fn region_height(region: ScreenRegion) -> usize {
    usize::from(region.bottom - region.top + 1)
}

fn text_matches(
    capture: &PtyVteCaptureResult,
    region: ScreenRegion,
    needle: &str,
    case_sensitive: bool,
) -> Result<Vec<(u16, usize)>> {
    let mut matches = Vec::new();
    for row in region.top..=region.bottom {
        let line = row_text_in_region(capture, row, region.left, region.right)?;
        let count = substring_count(&line, needle, case_sensitive);
        if count > 0 {
            matches.push((row, count));
        }
    }
    Ok(matches)
}

fn row_text_in_region(
    capture: &PtyVteCaptureResult,
    row: u16,
    left: u16,
    right: u16,
) -> Result<String> {
    let mut rendered = String::new();
    for column in left..=right {
        rendered.push_str(cell_text(capture_cell(capture, row, column)?));
    }
    Ok(rendered)
}

fn substring_count(haystack: &str, needle: &str, case_sensitive: bool) -> usize {
    if case_sensitive {
        haystack.match_indices(needle).count()
    } else {
        haystack
            .to_lowercase()
            .match_indices(&needle.to_lowercase())
            .count()
    }
}

fn capture_cell(
    capture: &PtyVteCaptureResult,
    row: u16,
    column: u16,
) -> Result<&PtyVteCaptureCell> {
    let row_data = capture.rows.get(usize::from(row)).ok_or_else(|| {
        anyhow!(
            "screen capture is missing row {} (screen rows {})",
            row,
            capture.screen_rows
        )
    })?;
    row_data
        .cells
        .iter()
        .find(|cell| cell.column == column)
        .or_else(|| row_data.cells.get(usize::from(column)))
        .ok_or_else(|| anyhow!("screen capture row {} is missing column {}", row, column))
}

fn cell_text(cell: &PtyVteCaptureCell) -> &str {
    if cell.text.is_empty() {
        " "
    } else {
        &cell.text
    }
}

fn comparable_capture_cells(
    capture: &PtyVteCaptureResult,
) -> BTreeMap<(u16, u16), ComparableScreenCell> {
    let mut cells = BTreeMap::new();
    for (fallback_row, row) in capture.rows.iter().enumerate() {
        let row_number = capture_row_number(row, fallback_row);
        for cell in &row.cells {
            cells.insert((row_number, cell.column), comparable_screen_cell(cell));
        }
    }
    cells
}

fn capture_row_number(row: &PtyVteCaptureRow, fallback_row: usize) -> u16 {
    row.row.unwrap_or_else(|| {
        if row.index > 0 || fallback_row == 0 {
            row.index
        } else {
            u16::try_from(fallback_row).unwrap_or(u16::MAX)
        }
    })
}

fn comparable_screen_cell(cell: &PtyVteCaptureCell) -> ComparableScreenCell {
    ComparableScreenCell {
        text: cell_text(cell).to_owned(),
        fg: comparable_color_label(&cell.fg, &cell.resolved_fg),
        bg: comparable_color_label(&cell.bg, &cell.resolved_bg),
        bold: cell.bold,
        italics: cell.italics,
        underscore: cell.underscore,
        strikethrough: cell.strikethrough,
        blink: cell.blink,
        reverse: cell.reverse,
    }
}

fn comparable_color_label(raw: &str, resolved: &[u8]) -> String {
    if resolved.len() == 3 {
        return format!("rgb({},{},{})", resolved[0], resolved[1], resolved[2]);
    }
    normalize_color_name(raw)
}

fn baseline_compare_diff_kind(
    actual: Option<&ComparableScreenCell>,
    baseline: Option<&ComparableScreenCell>,
) -> &'static str {
    match (actual, baseline) {
        (Some(_), Some(_)) => "changed",
        (Some(_), None) => "added",
        (None, Some(_)) => "missing",
        (None, None) => "unchanged",
    }
}

fn baseline_difference_summary(
    diff_count: usize,
    row_diff_counts: &BTreeMap<u16, usize>,
    dimensions_match: bool,
    current: &PtyVteCaptureResult,
    baseline: &PtyVteCaptureResult,
) -> String {
    if diff_count == 0 && dimensions_match {
        return "No grid differences detected".to_owned();
    }

    let mut parts = Vec::new();
    if !dimensions_match {
        parts.push(format!(
            "dimensions {}x{} -> {}x{}",
            baseline.screen_rows,
            baseline.screen_columns,
            current.screen_rows,
            current.screen_columns
        ));
    }
    if diff_count > 0 {
        let row_summary = row_diff_counts
            .iter()
            .take(5)
            .map(|(row, count)| format!("row {row} ({count})"))
            .collect::<Vec<_>>()
            .join(", ");
        let overflow = row_diff_counts.len().saturating_sub(5);
        if overflow > 0 {
            parts.push(format!(
                "{diff_count} cell diff(s) across {} row(s): {row_summary}, +{overflow} more row(s)",
                row_diff_counts.len()
            ));
        } else {
            parts.push(format!(
                "{diff_count} cell diff(s) across {} row(s): {row_summary}",
                row_diff_counts.len()
            ));
        }
    }

    parts.join("; ")
}

#[derive(Debug, Serialize)]
struct HorizontalEdgeStats {
    filled: usize,
    total: usize,
    max_blank_run: usize,
}

fn horizontal_edge_stats(
    capture: &PtyVteCaptureResult,
    row: u16,
    left: u16,
    right: u16,
) -> Result<HorizontalEdgeStats> {
    let mut filled = 0;
    let mut blank_run = 0;
    let mut max_blank_run = 0;
    for column in left..=right {
        let is_blank = cell_text(capture_cell(capture, row, column)?)
            .trim()
            .is_empty();
        if is_blank {
            blank_run += 1;
            max_blank_run = max_blank_run.max(blank_run);
        } else {
            filled += 1;
            blank_run = 0;
        }
    }
    Ok(HorizontalEdgeStats {
        filled,
        total: usize::from(right - left + 1),
        max_blank_run,
    })
}

fn detect_layout_columns(
    capture: &PtyVteCaptureResult,
    region: ScreenRegion,
    min_gap: usize,
) -> Result<Vec<ScreenRegion>> {
    let mut occupied = vec![false; region_width(region)];
    for row in region.top..=region.bottom {
        for column in region.left..=region.right {
            let cell = capture_cell(capture, row, column)?;
            if !cell_text(cell).trim().is_empty() {
                occupied[usize::from(column - region.left)] = true;
            }
        }
    }

    let raw_runs = occupied_runs(&occupied, region.left);
    if raw_runs.len() <= 1 || min_gap <= 1 {
        return Ok(raw_runs
            .into_iter()
            .map(|(left, right)| ScreenRegion {
                top: region.top,
                bottom: region.bottom,
                left,
                right,
            })
            .collect());
    }

    let mut merged = Vec::new();
    for (left, right) in raw_runs {
        match merged.last_mut() {
            Some((_, previous_right)) if usize::from(left - *previous_right - 1) < min_gap => {
                *previous_right = right;
            }
            _ => merged.push((left, right)),
        }
    }

    Ok(merged
        .into_iter()
        .map(|(left, right)| ScreenRegion {
            top: region.top,
            bottom: region.bottom,
            left,
            right,
        })
        .collect())
}

fn occupied_runs(occupied: &[bool], base_column: u16) -> Vec<(u16, u16)> {
    let mut runs = Vec::new();
    let mut current_start = None;
    for (offset, is_occupied) in occupied.iter().copied().enumerate() {
        match (current_start, is_occupied) {
            (None, true) => current_start = Some(offset),
            (Some(start), false) => {
                runs.push((
                    base_column + u16::try_from(start).unwrap_or(0),
                    base_column + u16::try_from(offset.saturating_sub(1)).unwrap_or(0),
                ));
                current_start = None;
            }
            _ => {}
        }
    }
    if let Some(start) = current_start {
        runs.push((
            base_column + u16::try_from(start).unwrap_or(0),
            base_column + u16::try_from(occupied.len().saturating_sub(1)).unwrap_or(0),
        ));
    }
    runs
}

fn overlapping_region(left: ScreenRegion, right: ScreenRegion) -> Option<ScreenRegion> {
    let top = left.top.max(right.top);
    let left_column = left.left.max(right.left);
    let bottom = left.bottom.min(right.bottom);
    let right_column = left.right.min(right.right);
    (top <= bottom && left_column <= right_column).then_some(ScreenRegion {
        top,
        left: left_column,
        bottom,
        right: right_column,
    })
}

fn color_matches(
    expected: &Value,
    actual: ScreenColorMatch<'_>,
    field: &str,
    index: usize,
) -> Result<bool> {
    match expected {
        Value::String(value) => {
            let normalized = normalize_color_name(value);
            if normalized == normalize_color_name(actual.raw) {
                return Ok(true);
            }
            if let Some(rgb) = parse_hex_color(value).or_else(|| ansi_color_rgb(&normalized)) {
                return Ok(actual.resolved == rgb);
            }
            Ok(false)
        }
        Value::Array(values) => {
            Ok(actual.resolved == parse_rgb_triplet(values, field, index)?.as_slice())
        }
        _ => bail!(
            "check #{} field `{field}` must be a color string or RGB array",
            index + 1
        ),
    }
}

fn parse_rgb_triplet(values: &[Value], field: &str, index: usize) -> Result<[u8; 3]> {
    if values.len() != 3 {
        bail!(
            "check #{} field `{field}` must contain exactly 3 RGB values",
            index + 1
        );
    }
    let mut rgb = [0_u8; 3];
    for (slot, value) in rgb.iter_mut().zip(values.iter()) {
        let component = value.as_u64().ok_or_else(|| {
            anyhow!(
                "check #{} field `{field}` must contain only integers",
                index + 1
            )
        })?;
        *slot = u8::try_from(component).map_err(|_| {
            anyhow!(
                "check #{} field `{field}` RGB values must be between 0 and 255",
                index + 1
            )
        })?;
    }
    Ok(rgb)
}

fn parse_hex_color(value: &str) -> Option<[u8; 3]> {
    let normalized = value.trim().trim_start_matches('#');
    if normalized.len() != 6 || !normalized.chars().all(|char| char.is_ascii_hexdigit()) {
        return None;
    }
    let red = u8::from_str_radix(&normalized[0..2], 16).ok()?;
    let green = u8::from_str_radix(&normalized[2..4], 16).ok()?;
    let blue = u8::from_str_radix(&normalized[4..6], 16).ok()?;
    Some([red, green, blue])
}

fn normalize_color_name(value: &str) -> String {
    value
        .chars()
        .filter(|char| !matches!(char, '_' | '-' | ' '))
        .flat_map(char::to_lowercase)
        .collect()
}

fn ansi_color_rgb(value: &str) -> Option<[u8; 3]> {
    Some(match value {
        "black" => [12, 12, 12],
        "red" => [205, 49, 49],
        "green" => [13, 188, 121],
        "brown" | "yellow" => [229, 229, 16],
        "blue" => [36, 114, 200],
        "magenta" => [188, 63, 188],
        "cyan" => [17, 168, 205],
        "white" => [229, 229, 229],
        "brightblack" => [102, 102, 102],
        "brightred" => [241, 76, 76],
        "brightgreen" => [35, 209, 139],
        "brightyellow" => [245, 245, 67],
        "brightblue" => [59, 142, 234],
        "brightmagenta" => [214, 112, 214],
        "brightcyan" => [41, 184, 219],
        "brightwhite" => [255, 255, 255],
        _ => return None,
    })
}

// =============================================================================
// Check Utility (shared with lib.rs)
// =============================================================================

pub fn checked_u16(raw: u64, key: &str) -> Result<u16> {
    u16::try_from(raw).map_err(|_| anyhow!("value for `{key}` must be between 0 and 65535"))
}

pub fn checked_usize(raw: u64, key: &str) -> Result<usize> {
    usize::try_from(raw)
        .map_err(|_| anyhow!("value for `{key}` must be between 0 and the maximum usize"))
}
