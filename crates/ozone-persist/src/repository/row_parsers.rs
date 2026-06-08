//! SQLite row parsing helpers for typed entity extraction.
//!
//! This module contains pure functions for parsing SQLite Row objects into
//! strongly-typed Rust structures. All parsers handle column validation and
//! conversion errors with explicit error messages.

use ozone_core::engine::{BranchId, BranchState, ConversationBranch, ConversationMessage, MessageId};
use ozone_core::session::SessionId;
use rusqlite::Row;

use crate::PersistError;

// BranchRecord is defined in mod.rs and imported here for use by parsers
use super::BranchRecord;

/// Parse a ConversationMessage from a SQLite row.
///
/// Expected row layout (in order):
/// 0. message_id (text)
/// 1. session_id (text, SessionId parseable)
/// 2. parent_id (optional text, MessageId parseable if present)
/// 3. author_kind (text)
/// 4. author_name (optional text)
/// 5. content (text)
/// 6. created_at (integer)
/// 7. edited_at (integer)
/// 8. is_hidden (integer, nonzero = true)
pub(super) fn read_conversation_message(
    row: &Row<'_>,
) -> rusqlite::Result<ConversationMessage> {
    let message_id = parse_sqlite_text::<MessageId>(row.get(0)?, 0)?;
    let session_id = SessionId::parse(row.get::<_, String>(1)?)
        .map_err(|error| sqlite_text_parse_error(1, error))?;
    let parent_id = row
        .get::<_, Option<String>>(2)?
        .map(|value| parse_sqlite_text::<MessageId>(value, 2))
        .transpose()?;

    Ok(ConversationMessage {
        message_id,
        session_id,
        parent_id,
        author_kind: row.get(3)?,
        author_name: row.get(4)?,
        content: row.get(5)?,
        created_at: row.get(6)?,
        edited_at: row.get(7)?,
        is_hidden: row.get::<_, i64>(8)? != 0,
    })
}

/// Parse a BranchRecord from a SQLite row.
///
/// Expected row layout (in order):
/// 0. branch_id (text)
/// 1. session_id (text, SessionId parseable)
/// 2. name (text)
/// 3. tip_message_id (text, MessageId parseable)
/// 4. created_at (integer)
/// 5. state (text, BranchState parseable)
/// 6. description (optional text)
/// 7. forked_from (text, MessageId parseable; must be present)
pub(super) fn read_branch_record(row: &Row<'_>) -> rusqlite::Result<BranchRecord> {
    let branch_id = parse_sqlite_text::<BranchId>(row.get(0)?, 0)?;
    let session_id = SessionId::parse(row.get::<_, String>(1)?)
        .map_err(|error| sqlite_text_parse_error(1, error))?;
    let tip_message_id = parse_sqlite_text::<MessageId>(row.get(3)?, 3)?;
    let state = row
        .get::<_, String>(5)?
        .parse::<BranchState>()
        .map_err(|error| sqlite_text_parse_error(5, error))?;
    let forked_from = row
        .get::<_, Option<String>>(7)?
        .ok_or_else(|| {
            sqlite_text_parse_error(
                7,
                PersistError::InvalidData("branch is missing forked_from_message_id".to_owned()),
            )
        })
        .and_then(|value| parse_sqlite_text::<MessageId>(value, 7))?;

    Ok(BranchRecord {
        branch: ConversationBranch {
            branch_id,
            session_id,
            name: row.get(2)?,
            tip_message_id,
            created_at: row.get(4)?,
            state,
            description: row.get(6)?,
        },
        forked_from,
    })
}

/// Parse a generic `FromStr` type from a SQLite text value.
///
/// This is the generic form used throughout row parsing; it converts parse
/// errors to `sqlite_text_parse_error` for consistent column-indexed error reporting.
pub fn parse_sqlite_text<T>(value: String, column_index: usize) -> rusqlite::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    value
        .parse::<T>()
        .map_err(|error| sqlite_text_parse_error(column_index, error))
}

/// Parse a u16 from a SQLite i64 column, with explicit range check.
///
/// Used when a column must hold a small integer value (0..=65535).
pub fn parse_i64_as_u16(
    value: i64,
    column_index: usize,
    field: &'static str,
) -> rusqlite::Result<u16> {
    u16::try_from(value).map_err(|_| {
        sqlite_integer_parse_error(
            column_index,
            PersistError::InvalidData(format!("{field} {value} is out of range for u16")),
        )
    })
}

/// Parse a u64 from a SQLite i64 column, with explicit range check.
///
/// Used when a column must hold a large unsigned value but SQLite stores it as i64.
pub fn parse_i64_as_u64(
    value: i64,
    column_index: usize,
    field: &'static str,
) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(|_| {
        sqlite_integer_parse_error(
            column_index,
            PersistError::InvalidData(format!("{field} {value} is out of range for u64")),
        )
    })
}

/// Format a text-column parse error with explicit column index for debugging.
pub fn sqlite_text_parse_error(
    column_index: usize,
    error: impl std::error::Error + Send + Sync + 'static,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column_index,
        rusqlite::types::Type::Text,
        Box::new(error),
    )
}

/// Format an integer-column parse error with explicit column index for debugging.
pub fn sqlite_integer_parse_error(
    column_index: usize,
    error: impl std::error::Error + Send + Sync + 'static,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column_index,
        rusqlite::types::Type::Integer,
        Box::new(error),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sqlite_text_converts_generic_type() {
        // Test that FromStr types are correctly parsed from strings
        let result = parse_sqlite_text::<u32>("42".to_string(), 0);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42u32);
    }

    #[test]
    fn parse_sqlite_text_reports_parse_error_with_column() {
        // Test that parse errors are reported with column index
        let result: rusqlite::Result<u32> = parse_sqlite_text("not-a-number".to_string(), 5);
        assert!(result.is_err());
    }

    #[test]
    fn parse_i64_as_u16_converts_valid_range() {
        // Test that valid u16 values are converted
        let result = parse_i64_as_u16(1000i64, 0, "test_field");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1000u16);
    }

    #[test]
    fn parse_i64_as_u16_respects_range() {
        // Test that out-of-range values are rejected
        let result = parse_i64_as_u16(100_000i64, 0, "test_field");
        assert!(result.is_err());
    }

    #[test]
    fn parse_i64_as_u64_converts_valid_range() {
        // Test that valid u64 values are converted
        let result = parse_i64_as_u64(9_223_372_036_854_775_000i64, 0, "test_field");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 9_223_372_036_854_775_000u64);
    }

    #[test]
    fn parse_i64_as_u64_respects_range() {
        // Test that negative values are rejected (i64 overflow)
        let result = parse_i64_as_u64(-1i64, 0, "test_field");
        assert!(result.is_err());
    }
}
