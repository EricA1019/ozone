//! Result file discovery, classification, and text formatting.
//!
//! Pure functions — no state, no rendering, no I/O beyond reading files.


#[derive(Debug, Clone)]
pub struct ResultFile {
    pub path: std::path::PathBuf,
    pub kind: ResultFileKind,
    pub model: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultFileKind {
    Sweep,
    Eval,
    CreativeWriting,
    Report,
}

impl ResultFileKind {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            ResultFileKind::Sweep => "Sweep",
            ResultFileKind::Eval => "Eval",
            ResultFileKind::CreativeWriting => "Creative",
            ResultFileKind::Report => "Report",
        }
    }
}

/// Read the second line of a CSV (first data row) as a summary.
pub(crate) fn first_csv_summary(path: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let line = text.lines().nth(1)?;
    let parts: Vec<&str> = line.split(',').take(4).collect();
    if parts.len() >= 3 {
        Some(format!(
            "{} ctx={} → {} tok/s",
            parts[0],
            parts.get(1).unwrap_or(&"?"),
            parts.get(2).unwrap_or(&"?")
        ))
    } else {
        Some(line.chars().take(80).collect())
    }
}

/// Recursively scan a directory for CSV and MD result files.
pub(crate) fn scan_result_dir(dir: &std::path::Path, out: &mut Vec<ResultFile>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if path.is_dir() {
                scan_result_dir(&path, out);
            } else if fname.ends_with(".csv")
                && (fname.contains("eval")
                    || fname.contains("sweep")
                    || fname.contains("creative")
                    || fname.starts_with("results_"))
            {
                let kind = if fname.contains("creative")
                    || path.to_string_lossy().contains("creative_writing")
                {
                    ResultFileKind::CreativeWriting
                } else if fname.contains("sweep") {
                    ResultFileKind::Sweep
                } else {
                    ResultFileKind::Eval
                };
                let summary = first_csv_summary(&path).unwrap_or_default();
                let model = path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                out.push(ResultFile {
                    path,
                    kind,
                    model,
                    summary,
                });
            } else if fname.ends_with(".md")
                && (fname.contains("creative") || fname.starts_with("results_"))
            {
                let summary = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|t| {
                        t.lines()
                            .next()
                            .map(|l| l.trim_start_matches("# ").to_string())
                    })
                    .unwrap_or_default();
                let model = path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                out.push(ResultFile {
                    path,
                    kind: ResultFileKind::Report,
                    model,
                    summary,
                });
            }
        }
    }
}

/// Format a result file's contents for display in the viewer.
pub(crate) fn format_result_text(path: &std::path::Path, text: &str, kind: &ResultFileKind) -> String {
    match kind {
        ResultFileKind::Report | ResultFileKind::CreativeWriting
            if path.extension().is_some_and(|e| e == "md") =>
        {
            text.to_string()
        }
        _ => {
            // CSV → aligned table
            let mut out = String::new();
            let lines: Vec<&str> = text.lines().collect();
            if lines.is_empty() {
                return "(empty file)".into();
            }
            // Compute column widths
            let headers: Vec<&str> = lines[0].split(',').collect();
            let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
            for line in &lines[1..] {
                let cols: Vec<&str> = line.split(',').collect();
                for (i, col) in cols.iter().enumerate() {
                    if i < widths.len() {
                        widths[i] = widths[i].max(col.len());
                    }
                }
            }
            // Header
            let header_line: String = headers
                .iter()
                .enumerate()
                .map(|(i, h)| format!("{:width$}", h, width = widths[i] + 2))
                .collect();
            out.push_str(&header_line);
            out.push('\n');
            out.push_str(&"-".repeat(header_line.len().min(120)));
            out.push('\n');
            // Data rows (limit to 50)
            for line in lines[1..].iter().take(50) {
                let cols: Vec<&str> = line.split(',').collect();
                let row: String = cols
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let w = widths.get(i).copied().unwrap_or(10) + 2;
                        let s = if c.len() > 20 {
                            format!("{}…", &c[..19])
                        } else {
                            c.to_string()
                        };
                        format!("{:width$}", s, width = w)
                    })
                    .collect();
                out.push_str(&row);
                out.push('\n');
            }
            if lines.len() > 51 {
                out.push_str(&format!("\n... {} more rows", lines.len() - 51));
            }
            out
        }
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn scan_result_dir_empty() {
        let dir = std::env::temp_dir().join("ozone-test-empty-results");
        let _ = std::fs::create_dir_all(&dir);
        let mut results = vec![];
        scan_result_dir(&dir, &mut results);
        assert!(results.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn format_result_text_csv() {
        let text = "model,ctx,tok/s\nmy-model,4096,42.0\n";
        let result = format_result_text(
            &PathBuf::from("results.csv"),
            text,
            &ResultFileKind::Eval,
        );
        assert!(result.contains("model"));
        assert!(result.contains("my-model"));
    }

    #[test]
    fn format_result_text_json() {
        let text = r#"{"score": 0.95}"#;
        let result = format_result_text(
            &PathBuf::from("results.json"),
            text,
            &ResultFileKind::Eval,
        );
        assert!(!result.is_empty());
    }

    #[test]
    fn format_result_text_empty() {
        let result = format_result_text(
            &PathBuf::from("results.csv"),
            "",
            &ResultFileKind::Eval,
        );
        assert_eq!(result, "(empty file)");
    }

    #[test]
    fn first_csv_summary_nonexistent_returns_none() {
        let result = first_csv_summary(&PathBuf::from("/nonexistent/file.csv"));
        assert!(result.is_none());
    }

    #[test]
    fn result_file_kind_label_is_consistent() {
        assert_eq!(ResultFileKind::Sweep.label(), "Sweep");
        assert_eq!(ResultFileKind::Eval.label(), "Eval");
        assert_eq!(ResultFileKind::CreativeWriting.label(), "Creative");
        assert_eq!(ResultFileKind::Report.label(), "Report");
    }
}
