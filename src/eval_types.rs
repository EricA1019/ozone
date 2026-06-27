//! Normalized failure types and status codes for the eval pipeline.
//!
//! These types give every eval result a shared vocabulary so that
//! aggregation, CSV export, and TUI views can speak the same language.

use serde::{Deserialize, Serialize};

/// Normalized failure types for eval tasks.
///
/// Every task result should record one of these, never a free-text string.
/// This enables summary queries like "how often does this model fail with
/// compile_error vs wrong_answer?"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureType {
    None,
    WrongAnswer,
    FormatInvalid,
    JsonInvalid,
    SchemaInvalid,
    SyntaxError,
    CompileError,
    TestFailure,
    Timeout,
    RuntimeError,
    ForbiddenImport,
    HallucinatedDependency,
    WrongLanguage,
    EmptyOutput,
    TruncatedOutput,
    Underanswered,
    OverlongOutput,
    RepetitionCollapse,
    AaaaCollapse,
    StopIgnored,
    PatchInvalid,
    WrongFileModified,
    ContextTooSmall,
    SandboxError,
    AdapterError,
    BackendError,
}

impl FailureType {
    /// Stable snake_case string representation for CSV/DB storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::WrongAnswer => "wrong_answer",
            Self::FormatInvalid => "format_invalid",
            Self::JsonInvalid => "json_invalid",
            Self::SchemaInvalid => "schema_invalid",
            Self::SyntaxError => "syntax_error",
            Self::CompileError => "compile_error",
            Self::TestFailure => "test_failure",
            Self::Timeout => "timeout",
            Self::RuntimeError => "runtime_error",
            Self::ForbiddenImport => "forbidden_import",
            Self::HallucinatedDependency => "hallucinated_dependency",
            Self::WrongLanguage => "wrong_language",
            Self::EmptyOutput => "empty_output",
            Self::TruncatedOutput => "truncated_output",
            Self::Underanswered => "underanswered",
            Self::OverlongOutput => "overlong_output",
            Self::RepetitionCollapse => "repetition_collapse",
            Self::AaaaCollapse => "aaaa_collapse",
            Self::StopIgnored => "stop_ignored",
            Self::PatchInvalid => "patch_invalid",
            Self::WrongFileModified => "wrong_file_modified",
            Self::ContextTooSmall => "context_too_small",
            Self::SandboxError => "sandbox_error",
            Self::AdapterError => "adapter_error",
            Self::BackendError => "backend_error",
        }
    }
}

/// Result status for eval tasks, gates, and suites.
///
/// Explicit statuses ensure no blank cells in CSV exports and enable
/// filtering by outcome category in reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvalStatus {
    Passed,
    Failed,
    SkippedGate,
    SkippedBudget,
    SkippedUser,
    Crashed,
    Timeout,
    Invalid,
    Unstable,
    AdapterError,
    SandboxError,
    ContextTooSmall,
    CacheHit,
}

impl EvalStatus {
    /// Stable snake_case string representation for CSV/DB storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::SkippedGate => "skipped_gate",
            Self::SkippedBudget => "skipped_budget",
            Self::SkippedUser => "skipped_user",
            Self::Crashed => "crashed",
            Self::Timeout => "timeout",
            Self::Invalid => "invalid",
            Self::Unstable => "unstable",
            Self::AdapterError => "adapter_error",
            Self::SandboxError => "sandbox_error",
            Self::ContextTooSmall => "context_too_small",
            Self::CacheHit => "cache_hit",
        }
    }
}

/// Size class for eval tasks, used to determine timeouts and budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SizeClass {
    /// Expected output <= 64 tokens
    Tiny,
    /// Expected output <= 256 tokens
    Small,
    /// Expected output <= 1024 tokens
    Medium,
    /// Expected output <= 2048 tokens
    Large,
    /// Repo/task dependent
    Heavy,
}

impl SizeClass {
    /// Maximum output tokens for the Tiny size class.
    pub const TINY_MAX_TOKENS: u32 = 64;
    /// Maximum output tokens for the Small size class.
    pub const SMALL_MAX_TOKENS: u32 = 256;
    /// Maximum output tokens for the Medium size class.
    pub const MEDIUM_MAX_TOKENS: u32 = 1024;
    /// Maximum output tokens for the Large size class.
    pub const LARGE_MAX_TOKENS: u32 = 2048;
    /// Maximum output tokens for the Heavy size class.
    pub const HEAVY_MAX_TOKENS: u32 = 4096;

    /// Default maximum output tokens for this size class.
    pub fn max_output_tokens(&self) -> u32 {
        match self {
            Self::Tiny => Self::TINY_MAX_TOKENS,
            Self::Small => Self::SMALL_MAX_TOKENS,
            Self::Medium => Self::MEDIUM_MAX_TOKENS,
            Self::Large => Self::LARGE_MAX_TOKENS,
            Self::Heavy => Self::HEAVY_MAX_TOKENS,
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_class_token_limits_are_monotonic() {
        // Each size class must have a strictly larger token limit than
        // the previous one. If this fails, someone reordered or changed
        // the limits without thinking about monotonicity.
        assert!(SizeClass::Tiny.max_output_tokens() < SizeClass::Small.max_output_tokens());
        assert!(SizeClass::Small.max_output_tokens() < SizeClass::Medium.max_output_tokens());
        assert!(SizeClass::Medium.max_output_tokens() < SizeClass::Large.max_output_tokens());
        assert!(SizeClass::Large.max_output_tokens() < SizeClass::Heavy.max_output_tokens());
    }

    #[test]
    fn failure_type_as_str_all_variants_non_empty_snake_case() {
        // Every FailureType variant must produce a non-empty, snake_case
        // string for CSV export stability.
        use FailureType::*;
        let all = [
            None,
            WrongAnswer,
            FormatInvalid,
            JsonInvalid,
            SchemaInvalid,
            SyntaxError,
            CompileError,
            TestFailure,
            Timeout,
            RuntimeError,
            ForbiddenImport,
            HallucinatedDependency,
            WrongLanguage,
            EmptyOutput,
            TruncatedOutput,
            Underanswered,
            OverlongOutput,
            RepetitionCollapse,
            AaaaCollapse,
            StopIgnored,
            PatchInvalid,
            WrongFileModified,
            ContextTooSmall,
            SandboxError,
            AdapterError,
            BackendError,
        ];
        for v in &all {
            let s = v.as_str();
            assert!(!s.is_empty(), "empty as_str for {v:?}");
            assert!(
                s.chars().all(|c| c.is_lowercase() || c == '_'),
                "non-snake-case as_str '{s}' for {v:?}"
            );
        }
    }

    #[test]
    fn eval_status_as_str_all_variants_non_empty_snake_case() {
        // Every EvalStatus variant must produce a non-empty, snake_case
        // string for CSV export stability.
        use EvalStatus::*;
        let all = [
            Passed,
            Failed,
            SkippedGate,
            SkippedBudget,
            SkippedUser,
            Crashed,
            Timeout,
            Invalid,
            Unstable,
            AdapterError,
            SandboxError,
            ContextTooSmall,
            CacheHit,
        ];
        for v in &all {
            let s = v.as_str();
            assert!(!s.is_empty(), "empty as_str for {v:?}");
            assert!(
                s.chars().all(|c| c.is_lowercase() || c == '_'),
                "non-snake-case as_str '{s}' for {v:?}"
            );
        }
    }
}
