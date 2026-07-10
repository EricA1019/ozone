#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextTokenBudget {
    pub used_tokens: u32,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextPreview {
    pub source: String,
    pub summary: String,
    pub lines: Vec<String>,
    pub selected_items: Option<usize>,
    pub omitted_items: Option<usize>,
    pub token_budget: Option<ContextTokenBudget>,
    pub inline_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextDryRunPreview {
    pub summary: String,
    pub built_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecallBrowser {
    pub title: String,
    pub summary: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TuiSessionMemoryMetadata {
    pub pinned_memories: Vec<TuiMemoryView>,
    pub note_memories: Vec<TuiMemoryView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuiMemoryView {
    pub artifact_id: String,
    pub text: String,
    pub provenance: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionMetadata {
    pub character_name: Option<String>,
    pub tags: Vec<String>,
    pub pinned_count: Option<usize>,
    pub greeting: Option<String>,
    pub memory_metadata: Option<TuiSessionMemoryMetadata>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionStats {
    pub message_count: usize,
    pub branch_count: usize,
    pub bookmark_count: usize,
}
