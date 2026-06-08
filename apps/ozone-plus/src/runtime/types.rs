use ozone_engine::ThinkingDisplayMode;
use ozone_persist::MemoryArtifactId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SessionCommand {
    Show,
    Rename(String),
    Retitle,
    Reroll,
    Character(Option<String>),
    Tags(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MemoryCommand {
    List,
    Note(String),
    Unpin(MemoryArtifactId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SearchCommand {
    Session(String),
    Global(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ShellCommand {
    Session(SessionCommand),
    Memory(MemoryCommand),
    Search(SearchCommand),
    Summarize(SummarizeShellCommand),
    Thinking(ThinkingCommand),
    TierB(TierBCommand),
    Hooks(HooksCommand),
    SafeMode(SafeModeCommand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SummarizeShellCommand {
    Session,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ThinkingCommand {
    Status,
    SetMode(ThinkingDisplayMode),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TierBCommand {
    Status,
    Toggle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HooksCommand {
    Status,
    List,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SafeModeCommand {
    Status,
    On,
    Off,
    Toggle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecentSearchSection {
    pub(crate) summary: String,
    pub(crate) hit_count: usize,
    pub(crate) lines: Vec<String>,
}
