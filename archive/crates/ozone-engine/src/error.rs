use std::{error::Error, fmt};

use ozone_core::engine::{BranchId, GenerationState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    NotFound {
        entity: &'static str,
        id: String,
    },
    AlreadyExists {
        entity: &'static str,
        id: String,
    },
    InvalidCommand {
        reason: String,
    },
    InvalidGenerationTransition {
        branch_id: BranchId,
        from: GenerationState,
        to: GenerationState,
    },
}

impl EngineError {
    pub fn not_found(entity: &'static str, id: impl Into<String>) -> Self {
        Self::NotFound {
            entity,
            id: id.into(),
        }
    }

    pub fn already_exists(entity: &'static str, id: impl Into<String>) -> Self {
        Self::AlreadyExists {
            entity,
            id: id.into(),
        }
    }

    pub fn invalid_command(reason: impl Into<String>) -> Self {
        Self::InvalidCommand {
            reason: reason.into(),
        }
    }
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { entity, id } => write!(f, "{entity} `{id}` was not found"),
            Self::AlreadyExists { entity, id } => write!(f, "{entity} `{id}` already exists"),
            Self::InvalidCommand { reason } => f.write_str(reason),
            Self::InvalidGenerationTransition {
                branch_id,
                from,
                to,
            } => write!(
                f,
                "invalid generation transition for branch `{branch_id}`: {from:?} -> {to:?}"
            ),
        }
    }
}

impl Error for EngineError {}

pub type EngineResult<T> = Result<T, EngineError>;
