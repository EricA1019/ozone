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
    /// Default maximum output tokens for this size class.
    pub fn max_output_tokens(&self) -> u32 {
        match self {
            Self::Tiny => 64,
            Self::Small => 256,
            Self::Medium => 1024,
            Self::Large => 2048,
            Self::Heavy => 4096,
        }
    }
}
