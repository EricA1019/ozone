use rusqlite::{Connection, Transaction};

use crate::migration::{Migration, Migrator};

pub(crate) const SESSION_SCHEMA_VERSION: u32 = 2;

pub(crate) static SESSION_MIGRATOR: Migrator = Migrator {
    migrations: &[
        Migration {
            version: 1,
            description: "phase 1a session schema",
            apply: apply_session_v1,
        },
        Migration {
            version: SESSION_SCHEMA_VERSION,
            description: "phase 1b conversation state durability",
            apply: apply_session_v2,
        },
    ],
};

pub(crate) fn ensure_global_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(GLOBAL_SCHEMA_V1)
}

fn apply_session_v1(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch(SESSION_SCHEMA_V1)
}

fn apply_session_v2(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch(SESSION_SCHEMA_V2)
}

const SESSION_SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER NOT NULL,
    applied_at INTEGER NOT NULL,
    description TEXT
);

CREATE TABLE IF NOT EXISTS session_lock (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    instance_id TEXT NOT NULL,
    acquired_at INTEGER NOT NULL,
    heartbeat_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
    message_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    parent_id TEXT REFERENCES messages(message_id),
    author_kind TEXT NOT NULL,
    author_name TEXT,
    content TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    edited_at INTEGER,
    is_hidden INTEGER NOT NULL DEFAULT 0,
    token_count INTEGER,
    token_count_method TEXT,
    generation_record_json TEXT,
    thinking_block_json TEXT,
    bookmarked INTEGER NOT NULL DEFAULT 0,
    bookmark_note TEXT
);

CREATE TABLE IF NOT EXISTS branches (
    branch_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    name TEXT NOT NULL,
    tip_message_id TEXT NOT NULL REFERENCES messages(message_id),
    created_at INTEGER NOT NULL,
    state TEXT NOT NULL DEFAULT 'inactive',
    description TEXT
);

CREATE TABLE IF NOT EXISTS message_ancestry (
    ancestor_id TEXT NOT NULL REFERENCES messages(message_id),
    descendant_id TEXT NOT NULL REFERENCES messages(message_id),
    depth INTEGER NOT NULL,
    PRIMARY KEY (ancestor_id, descendant_id)
);

CREATE TABLE IF NOT EXISTS swipe_groups (
    swipe_group_id TEXT PRIMARY KEY,
    parent_message_id TEXT NOT NULL REFERENCES messages(message_id),
    parent_context_message_id TEXT REFERENCES messages(message_id),
    active_ordinal INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS swipe_candidates (
    swipe_group_id TEXT NOT NULL REFERENCES swipe_groups(swipe_group_id),
    ordinal INTEGER NOT NULL,
    message_id TEXT NOT NULL REFERENCES messages(message_id),
    state TEXT NOT NULL DEFAULT 'active',
    partial_content TEXT,
    tokens_generated INTEGER,
    PRIMARY KEY (swipe_group_id, ordinal)
);

CREATE TABLE IF NOT EXISTS memory_artifacts (
    artifact_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    content_json TEXT NOT NULL,
    source_start_message_id TEXT,
    source_end_message_id TEXT,
    provenance TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    snapshot_version INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS bookmarks (
    bookmark_id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES messages(message_id),
    note TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS context_plans (
    plan_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    branch_id TEXT NOT NULL,
    is_dry_run INTEGER NOT NULL DEFAULT 0,
    plan_json TEXT NOT NULL,
    total_tokens INTEGER NOT NULL,
    budget INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS events (
    event_id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    content,
    content=messages,
    content_rowid=rowid
);

CREATE VIRTUAL TABLE IF NOT EXISTS artifacts_fts USING fts5(
    content_json,
    content=memory_artifacts,
    content_rowid=rowid
);

CREATE TRIGGER IF NOT EXISTS messages_fts_insert AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, content) VALUES (NEW.rowid, NEW.content);
END;

CREATE TRIGGER IF NOT EXISTS messages_fts_update AFTER UPDATE OF content ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, content) VALUES ('delete', OLD.rowid, OLD.content);
    INSERT INTO messages_fts(rowid, content) VALUES (NEW.rowid, NEW.content);
END;

CREATE TRIGGER IF NOT EXISTS messages_fts_delete AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, content) VALUES ('delete', OLD.rowid, OLD.content);
END;

CREATE TRIGGER IF NOT EXISTS artifacts_fts_insert AFTER INSERT ON memory_artifacts BEGIN
    INSERT INTO artifacts_fts(rowid, content_json) VALUES (NEW.rowid, NEW.content_json);
END;

CREATE TRIGGER IF NOT EXISTS artifacts_fts_update AFTER UPDATE OF content_json ON memory_artifacts BEGIN
    INSERT INTO artifacts_fts(artifacts_fts, rowid, content_json) VALUES ('delete', OLD.rowid, OLD.content_json);
    INSERT INTO artifacts_fts(rowid, content_json) VALUES (NEW.rowid, NEW.content_json);
END;

CREATE TRIGGER IF NOT EXISTS artifacts_fts_delete AFTER DELETE ON memory_artifacts BEGIN
    INSERT INTO artifacts_fts(artifacts_fts, rowid, content_json) VALUES ('delete', OLD.rowid, OLD.content_json);
END;

CREATE INDEX IF NOT EXISTS idx_messages_parent ON messages(parent_id);
CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
CREATE INDEX IF NOT EXISTS idx_messages_created ON messages(created_at);
CREATE INDEX IF NOT EXISTS idx_branches_session ON branches(session_id);
CREATE INDEX IF NOT EXISTS idx_branches_state ON branches(state);
CREATE INDEX IF NOT EXISTS idx_ancestry_descendant ON message_ancestry(descendant_id);
CREATE INDEX IF NOT EXISTS idx_artifacts_session ON memory_artifacts(session_id);
CREATE INDEX IF NOT EXISTS idx_artifacts_kind ON memory_artifacts(kind);
CREATE INDEX IF NOT EXISTS idx_swipe_groups_parent ON swipe_groups(parent_message_id);
CREATE INDEX IF NOT EXISTS idx_context_plans_session ON context_plans(session_id, created_at);
CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
CREATE INDEX IF NOT EXISTS idx_events_created ON events(created_at);
"#;

const SESSION_SCHEMA_V2: &str = r#"
ALTER TABLE branches ADD COLUMN forked_from_message_id TEXT REFERENCES messages(message_id);

UPDATE branches
SET forked_from_message_id = tip_message_id
WHERE forked_from_message_id IS NULL;

CREATE TABLE IF NOT EXISTS message_edits (
    revision_id INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id TEXT NOT NULL REFERENCES messages(message_id) ON DELETE CASCADE,
    previous_content TEXT NOT NULL,
    edited_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_message_edits_message ON message_edits(message_id, revision_id);
CREATE INDEX IF NOT EXISTS idx_branches_forked_from ON branches(forked_from_message_id);

DELETE FROM message_ancestry;

INSERT INTO message_ancestry (ancestor_id, descendant_id, depth)
SELECT message_id, message_id, 0
FROM messages;

WITH RECURSIVE ancestry(descendant_id, ancestor_id, depth) AS (
    SELECT message_id, parent_id, 1
    FROM messages
    WHERE parent_id IS NOT NULL

    UNION ALL

    SELECT ancestry.descendant_id, messages.parent_id, ancestry.depth + 1
    FROM ancestry
    JOIN messages ON messages.message_id = ancestry.ancestor_id
    WHERE messages.parent_id IS NOT NULL
)
INSERT INTO message_ancestry (ancestor_id, descendant_id, depth)
SELECT ancestor_id, descendant_id, depth
FROM ancestry;

UPDATE branches
SET state = 'inactive'
WHERE state = 'active'
  AND rowid NOT IN (
      SELECT rowid
      FROM (
          SELECT rowid,
                 ROW_NUMBER() OVER (
                     PARTITION BY session_id
                     ORDER BY created_at DESC, branch_id ASC
                 ) AS branch_rank
          FROM branches
          WHERE state = 'active'
      ) ranked
      WHERE branch_rank = 1
  );

CREATE UNIQUE INDEX IF NOT EXISTS idx_branches_one_active_per_session
    ON branches(session_id)
    WHERE state = 'active';
"#;

const GLOBAL_SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    session_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    character_name TEXT,
    created_at INTEGER NOT NULL,
    last_opened_at INTEGER NOT NULL,
    message_count INTEGER NOT NULL DEFAULT 0,
    db_size_bytes INTEGER,
    tags TEXT
);

CREATE TABLE IF NOT EXISTS session_search (
    session_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    content TEXT NOT NULL,
    author_kind TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (session_id, message_id)
);

CREATE VIRTUAL TABLE IF NOT EXISTS session_search_fts USING fts5(
    content,
    content=session_search,
    content_rowid=rowid
);

CREATE TABLE IF NOT EXISTS character_cards (
    card_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    system_prompt TEXT NOT NULL DEFAULT '',
    personality TEXT NOT NULL DEFAULT '',
    scenario TEXT NOT NULL DEFAULT '',
    greeting TEXT NOT NULL DEFAULT '',
    example_dialogue TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_character_cards_name ON character_cards(name);

CREATE TRIGGER IF NOT EXISTS session_search_fts_insert AFTER INSERT ON session_search BEGIN
    INSERT INTO session_search_fts(rowid, content) VALUES (NEW.rowid, NEW.content);
END;

CREATE TRIGGER IF NOT EXISTS session_search_fts_update AFTER UPDATE OF content ON session_search BEGIN
    INSERT INTO session_search_fts(session_search_fts, rowid, content) VALUES ('delete', OLD.rowid, OLD.content);
    INSERT INTO session_search_fts(rowid, content) VALUES (NEW.rowid, NEW.content);
END;

CREATE TRIGGER IF NOT EXISTS session_search_fts_delete AFTER DELETE ON session_search BEGIN
    INSERT INTO session_search_fts(session_search_fts, rowid, content) VALUES ('delete', OLD.rowid, OLD.content);
END;
"#;
