//! Shared CLI output helpers for consistent terminal formatting across
//! all Ozone binaries (`ozone`, `ozone-plus`).
//!
//! Uses Unicode box-drawing characters and the ⬡ brand glyph.
//! No ANSI color codes — keeps output clean when piped to files or CI logs.

use std::fmt;

/// The Ozone brand glyph used as a prefix.
pub const BRAND: &str = "⬡";

/// Print a branded section header.
///
/// ```text
/// ⬡ Section Title
/// ─────────────────────────────────────────────────
/// ```
pub fn header(title: &str) {
    println!();
    println!("  {BRAND} {title}");
    println!("  {}", "─".repeat(49));
}

/// Print a key-value pair aligned for section bodies.
///
/// ```text
///   Key:       value
/// ```
pub fn field(key: &str, value: &impl fmt::Display) {
    println!("  {key:<12}{value}");
}

/// Print an informational line (indented, no prefix).
pub fn info(msg: &str) {
    println!("  {msg}");
}

/// Print a success line.
pub fn success(msg: &str) {
    println!("  ✓ {msg}");
}

/// Print a warning to stderr.
pub fn warn(msg: &str) {
    eprintln!("  ⚠ {msg}");
}

/// Print an error to stderr.
pub fn error(msg: &str) {
    eprintln!("  ✗ {msg}");
}

/// Print a blank line (section spacer).
pub fn spacer() {
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brand_glyph_is_hexagon() {
        assert_eq!(BRAND, "⬡");
    }

    #[test]
    fn field_formatting() {
        // Just ensure it doesn't panic.
        field("Model:", &"test-model.gguf");
        field("Layers:", &32);
    }
}
