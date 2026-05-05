/// MCP tool: screen check.
use crate::OzoneMcpServer;
use crate::ToolReply;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use serde_json::Value;
use serde_json::json;
use crate::optional_string;
use crate::load_screen_capture_sidecar;
use crate::evaluate_screen_check;

pub fn screen_check_tool(_server: &OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
    let artifact_path = optional_string(args, "artifactPath")
        .or_else(|| optional_string(args, "path"))
        .or_else(|| optional_string(args, "sidecarPath"))
        .ok_or_else(|| {
            anyhow!("screen_check_tool requires `artifactPath` (or `path` / `sidecarPath`)")
        })?;
    let checks = args
        .get("checks")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("field `checks` must be an array of check objects"))?;
    if checks.is_empty() {
        bail!("field `checks` must contain at least one check");
    }

    let (sidecar_path, capture) = load_screen_capture_sidecar(&artifact_path)?;
    let results = checks
        .iter()
        .enumerate()
        .map(|(index, check)| evaluate_screen_check(index, check, &capture))
        .collect::<Result<Vec<_>>>()?;
    let passed = results.iter().filter(|result| result.passed).count();
    let failed = results.len().saturating_sub(passed);
    let success = failed == 0;
    let summary = if success {
        format!("Screen check passed ({passed}/{passed} checks)")
    } else {
        format!(
            "Screen check failed ({passed}/{} checks passed)",
            results.len()
        )
    };

    let data = json!({
        "artifactPath": artifact_path,
        "sidecarPath": sidecar_path.display().to_string(),
        "pngPath": capture.png_path,
        "screen": {
            "rows": capture.screen_rows,
            "columns": capture.screen_columns,
            "cursor": capture.cursor,
            "font": capture.font
        },
        "summary": {
            "total": results.len(),
            "passed": passed,
            "failed": failed,
            "success": success
        },
        "checks": results
    });

    Ok(if success {
        ToolReply::success(summary, data)
    } else {
        ToolReply::error(summary, data)
    })
}
