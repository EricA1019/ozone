use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicI64, AtomicU64, Ordering},
        Arc,
    },
};

use ozone_core::{
    engine::{
        ActivateSwipeCommand, CommitMessageCommand, CreateBranchCommand,
        RecordSwipeCandidateCommand, SwipeCandidate, SwipeCandidateState, SwipeGroup, SwipeGroupId,
    },
    session::{CreateSessionRequest, UpdateSessionRequest},
};
use ozone_memory::{
    source_text_hash, AuthorId, CreateNoteMemoryRequest, EmbeddingProviderKind, EmbeddingRecord,
    EmbeddingRecordMetadata, PinMessageMemoryRequest,
};
use rusqlite::Connection;

use super::*;
use crate::{
    import_export::{SESSION_EXPORT_FORMAT, TRANSCRIPT_EXPORT_FORMAT},
    migration::backup_path,
    schema::SESSION_SCHEMA_VERSION,
};

static SANDBOX_COUNTER: AtomicU64 = AtomicU64::new(1);

#[test]
fn schema_creation_builds_session_and_global_databases() {
    let sandbox = TestSandbox::new("schema-creation");
    let (repo, _) = test_repo(&sandbox, 1_725_647_200_123);
    let mut request = CreateSessionRequest::new("Schema Session");
    request.character_name = Some("Alice".to_owned());
    request.tags = vec!["alpha".to_owned(), "beta".to_owned()];

    let summary = repo.create_session(request).unwrap();
    let session_db_path = repo.paths().session_db_path(&summary.session_id);
    let global_db_path = repo.paths().global_db_path();

    assert!(session_db_path.exists());
    assert!(repo
        .paths()
        .session_config_path(&summary.session_id)
        .exists());
    assert!(repo
        .paths()
        .session_draft_path(&summary.session_id)
        .exists());
    assert!(global_db_path.exists());
    assert_eq!(summary.created_at, 1_725_647_200_123);
    assert_eq!(summary.last_opened_at, 1_725_647_200_123);

    let session_conn = Connection::open(&session_db_path).unwrap();
    for name in [
        "schema_version",
        "session_lock",
        "messages",
        "message_edits",
        "branches",
        "message_ancestry",
        "swipe_groups",
        "swipe_candidates",
        "memory_artifacts",
        "bookmarks",
        "context_plans",
        "events",
        "messages_fts",
        "artifacts_fts",
    ] {
        assert_sqlite_object(&session_conn, "table", name);
    }
    for name in [
        "messages_fts_insert",
        "messages_fts_update",
        "messages_fts_delete",
        "artifacts_fts_insert",
        "artifacts_fts_update",
        "artifacts_fts_delete",
    ] {
        assert_sqlite_object(&session_conn, "trigger", name);
    }
    let version: i64 = session_conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(version, i64::from(SESSION_SCHEMA_VERSION));

    let global_conn = Connection::open(&global_db_path).unwrap();
    for name in ["sessions", "session_search", "session_search_fts"] {
        assert_sqlite_object(&global_conn, "table", name);
    }
    for name in [
        "session_search_fts_insert",
        "session_search_fts_update",
        "session_search_fts_delete",
    ] {
        assert_sqlite_object(&global_conn, "trigger", name);
    }
}

#[test]
fn create_list_and_get_session_flow_uses_global_index() {
    let sandbox = TestSandbox::new("create-list-get");
    let (repo, clock) = test_repo(&sandbox, 1_725_647_200_000);

    let first = repo
        .create_session(CreateSessionRequest::new("First Session"))
        .unwrap();
    clock.store(1_725_647_260_000, Ordering::SeqCst);
    let mut second_request = CreateSessionRequest::new("Second Session");
    second_request.character_name = Some("Beatrice".to_owned());
    second_request.tags = vec!["story".to_owned()];
    let second = repo.create_session(second_request).unwrap();

    let sessions = repo.list_sessions().unwrap();
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0].session_id, second.session_id);
    assert_eq!(sessions[1].session_id, first.session_id);
    assert_eq!(sessions[0].character_name.as_deref(), Some("Beatrice"));
    assert_eq!(sessions[0].tags, vec!["story".to_owned()]);
    assert_eq!(sessions[0].created_at, 1_725_647_260_000);

    let fetched = repo.get_session(&second.session_id).unwrap().unwrap();
    assert_eq!(fetched, second);
    assert!(repo
        .get_session(&SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap())
        .unwrap()
        .is_none());
}

#[test]
fn update_session_metadata_rewrites_global_summary_fields() {
    let sandbox = TestSandbox::new("update-session-metadata");
    let (repo, clock) = test_repo(&sandbox, 1_725_647_200_000);
    let session = repo
        .create_session(CreateSessionRequest::new("Original Session"))
        .unwrap();

    clock.store(1_725_647_260_000, Ordering::SeqCst);
    let updated = repo
        .update_session_metadata(
            &session.session_id,
            UpdateSessionRequest {
                name: Some("Renamed Session".to_owned()),
                character_name: Some(Some("Beatrice".to_owned())),
                tags: Some(vec!["story".to_owned(), "phase1f".to_owned()]),
            },
        )
        .unwrap();

    assert_eq!(updated.name, "Renamed Session");
    assert_eq!(updated.character_name.as_deref(), Some("Beatrice"));
    assert_eq!(updated.tags, vec!["story".to_owned(), "phase1f".to_owned()]);
    assert_eq!(updated.last_opened_at, 1_725_647_260_000);

    let fetched = repo.get_session(&session.session_id).unwrap().unwrap();
    assert_eq!(fetched, updated);
}

#[test]
fn message_fts_triggers_sync_on_insert() {
    let sandbox = TestSandbox::new("message-fts");
    let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
    let session = repo
        .create_session(CreateSessionRequest::new("FTS Session"))
        .unwrap();

    let record = repo
        .insert_message(
            &session.session_id,
            CreateMessageRequest::user("The nebula fox remembers the hidden orchard"),
        )
        .unwrap();

    let hits = repo.search_messages(&session.session_id, "nebula").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].message_id, record.message_id);
    assert!(hits[0].content.contains("hidden orchard"));

    let global_conn = Connection::open(repo.paths().global_db_path()).unwrap();
    let count: i64 = global_conn
        .query_row(
            "SELECT COUNT(*)
                 FROM session_search_fts
                 JOIN session_search ON session_search.rowid = session_search_fts.rowid
                 WHERE session_search.session_id = ?1 AND session_search_fts MATCH 'nebula'",
            [session.session_id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn pinning_and_unpinning_message_memories_round_trip() {
    let sandbox = TestSandbox::new("pinned-memory-lifecycle");
    let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
    let session = repo
        .create_session(CreateSessionRequest::new("Pinned Memory Session"))
        .unwrap();
    let message = repo
        .insert_message(
            &session.session_id,
            CreateMessageRequest::user("Remember the observatory override phrase."),
        )
        .unwrap();
    let message_id = MessageId::parse(message.message_id).unwrap();

    let pinned = repo
        .pin_message_memory(
            &session.session_id,
            &message_id,
            PinMessageMemoryRequest {
                pinned_by: ozone_memory::AuthorId::User,
                expires_after_turns: Some(3),
                provenance: Provenance::UserAuthored,
            },
        )
        .unwrap();

    assert_eq!(pinned.source_message_id, Some(message_id));
    assert_eq!(
        pinned.content.text,
        "Remember the observatory override phrase."
    );
    assert_eq!(pinned.content.expires_after_turns, Some(3));
    assert_eq!(pinned.snapshot_version, 1);

    let listed = repo.list_pinned_memories(&session.session_id).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].record.artifact_id, pinned.artifact_id);
    assert_eq!(listed[0].remaining_turns, Some(3));
    assert!(listed[0].is_active);

    let conn = Connection::open(repo.paths().session_db_path(&session.session_id)).unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*)
                 FROM memory_artifacts
                 WHERE session_id = ?1 AND kind = 'pinned_memory'",
            [session.session_id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);

    assert!(repo
        .remove_pinned_memory(&session.session_id, &pinned.artifact_id)
        .unwrap());
    assert!(repo
        .list_pinned_memories(&session.session_id)
        .unwrap()
        .is_empty());
    assert!(!repo
        .remove_pinned_memory(&session.session_id, &pinned.artifact_id)
        .unwrap());
}

#[test]
fn freeform_note_memories_persist_without_source_messages() {
    let sandbox = TestSandbox::new("pinned-note-memory");
    let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
    let session = repo
        .create_session(CreateSessionRequest::new("Pinned Notes"))
        .unwrap();

    let mut request = CreateNoteMemoryRequest::new(
        "Pack the brass lantern before leaving camp.",
        ozone_memory::AuthorId::User,
        Provenance::UserAuthored,
    );
    request.content.expires_after_turns = Some(4);

    let note = repo
        .create_note_memory(&session.session_id, request)
        .unwrap();
    assert_eq!(note.source_message_id, None);
    assert_eq!(note.snapshot_version, 0);

    let listed = repo.list_note_memories(&session.session_id).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(
        listed[0].record.content.text,
        "Pack the brass lantern before leaving camp."
    );
    assert_eq!(listed[0].remaining_turns, Some(4));

    let conn = Connection::open(repo.paths().session_db_path(&session.session_id)).unwrap();
    let stored: (String, String, Option<String>, Option<String>, String, i64) = conn
            .query_row(
                "SELECT kind, content_json, source_start_message_id, source_end_message_id, provenance, snapshot_version
                 FROM memory_artifacts
                 WHERE artifact_id = ?1",
                [note.artifact_id.as_str()],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .unwrap();
    assert_eq!(stored.0, "note_memory");
    assert!(stored.1.contains("Pack the brass lantern"));
    assert_eq!(stored.2, None);
    assert_eq!(stored.3, None);
    assert_eq!(stored.4, "user_authored");
    assert_eq!(stored.5, 0);
}

#[test]
fn pinned_memory_expiry_tracks_message_count() {
    let sandbox = TestSandbox::new("pinned-memory-expiry");
    let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
    let session = repo
        .create_session(CreateSessionRequest::new("Expiry Session"))
        .unwrap();
    let seed = repo
        .insert_message(
            &session.session_id,
            CreateMessageRequest::user("The comet marker is blue."),
        )
        .unwrap();
    let seed_id = MessageId::parse(seed.message_id).unwrap();

    let pinned = repo
        .pin_message_memory(
            &session.session_id,
            &seed_id,
            PinMessageMemoryRequest {
                pinned_by: ozone_memory::AuthorId::User,
                expires_after_turns: Some(2),
                provenance: Provenance::UserAuthored,
            },
        )
        .unwrap();
    assert_eq!(pinned.snapshot_version, 1);

    let initial = repo.list_pinned_memories(&session.session_id).unwrap();
    assert_eq!(initial[0].turns_elapsed, 0);
    assert_eq!(initial[0].remaining_turns, Some(2));
    assert!(initial[0].is_active);

    repo.insert_message(
        &session.session_id,
        CreateMessageRequest::user("A fresh turn advances the countdown."),
    )
    .unwrap();
    let after_one_turn = repo.list_pinned_memories(&session.session_id).unwrap();
    assert_eq!(after_one_turn[0].turns_elapsed, 1);
    assert_eq!(after_one_turn[0].remaining_turns, Some(1));
    assert!(after_one_turn[0].is_active);

    repo.insert_message(
        &session.session_id,
        CreateMessageRequest::user("The countdown should now expire."),
    )
    .unwrap();
    let expired = repo.list_pinned_memories(&session.session_id).unwrap();
    assert_eq!(expired[0].turns_elapsed, 2);
    assert_eq!(expired[0].remaining_turns, Some(0));
    assert!(!expired[0].is_active);
    assert!(expired[0].is_expired());
}

#[test]
fn embedding_artifacts_round_trip_and_replace_per_session() {
    let sandbox = TestSandbox::new("embedding-artifacts");
    let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
    let first = repo
        .create_session(CreateSessionRequest::new("Embedding Session One"))
        .unwrap();
    let second = repo
        .create_session(CreateSessionRequest::new("Embedding Session Two"))
        .unwrap();
    let first_message = repo
        .insert_message(
            &first.session_id,
            CreateMessageRequest::user("The observatory key is under the lamp."),
        )
        .unwrap();
    let second_message = repo
        .insert_message(
            &second.session_id,
            CreateMessageRequest::user("The gate opens at dusk."),
        )
        .unwrap();

    let first_record = embedding_record(
        "923e4567-e89b-12d3-a456-426614174000",
        &first.session_id,
        Some(&MessageId::parse(first_message.message_id.clone()).unwrap()),
        vec![0.1, 0.2, 0.3],
        "The observatory key is under the lamp.",
        1_725_647_200_000,
        1,
    );
    let second_record = embedding_record(
        "a23e4567-e89b-12d3-a456-426614174000",
        &second.session_id,
        Some(&MessageId::parse(second_message.message_id.clone()).unwrap()),
        vec![0.4, 0.5, 0.6],
        "The gate opens at dusk.",
        1_725_647_200_100,
        1,
    );

    assert_eq!(
        repo.upsert_embedding_artifacts(&[first_record.clone(), second_record.clone()])
            .unwrap(),
        2
    );
    assert_eq!(
        repo.list_embedding_artifacts(Some(&first.session_id))
            .unwrap(),
        vec![first_record.clone()]
    );
    let global = repo.list_embedding_artifacts(None).unwrap();
    assert_eq!(global.len(), 2);
    assert!(global.contains(&first_record));
    assert!(global.contains(&second_record));

    let updated_first = embedding_record(
        "923e4567-e89b-12d3-a456-426614174000",
        &first.session_id,
        Some(&MessageId::parse(first_message.message_id.clone()).unwrap()),
        vec![0.9, 0.0, 0.1],
        "The observatory key moved behind the painting.",
        1_725_647_200_200,
        2,
    );
    repo.upsert_embedding_artifacts(std::slice::from_ref(&updated_first))
        .unwrap();
    assert_eq!(
        repo.list_embedding_artifacts(Some(&first.session_id))
            .unwrap(),
        vec![updated_first.clone()]
    );

    let replacement = embedding_record(
        "b23e4567-e89b-12d3-a456-426614174000",
        &first.session_id,
        None,
        vec![0.0, 1.0, 0.0],
        "Pack the brass lantern before leaving camp.",
        1_725_647_200_300,
        0,
    );
    assert_eq!(
        repo.replace_embedding_artifacts(
            Some(&first.session_id),
            std::slice::from_ref(&replacement),
        )
        .unwrap(),
        1
    );
    assert_eq!(
        repo.list_embedding_artifacts(Some(&first.session_id))
            .unwrap(),
        vec![replacement.clone()]
    );
    let global = repo.list_embedding_artifacts(None).unwrap();
    assert_eq!(global.len(), 2);
    assert!(global.contains(&replacement));
    assert!(global.contains(&second_record));
}

#[test]
fn cross_session_search_returns_session_metadata_and_local_search_still_scopes() {
    let sandbox = TestSandbox::new("cross-session-search");
    let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);

    let mut first_request = CreateSessionRequest::new("Observatory Log");
    first_request.character_name = Some("Aster".to_owned());
    first_request.tags = vec!["stellar".to_owned()];
    let first = repo.create_session(first_request).unwrap();

    let mut second_request = CreateSessionRequest::new("Village Log");
    second_request.character_name = Some("Mira".to_owned());
    second_request.tags = vec!["grounded".to_owned(), "phase2a".to_owned()];
    let second = repo.create_session(second_request).unwrap();

    let first_nebula = repo
        .insert_message(
            &first.session_id,
            CreateMessageRequest::user("The nebula gate opens only at dusk."),
        )
        .unwrap();
    repo.insert_message(
        &first.session_id,
        CreateMessageRequest::user("The orchard trail stays quiet tonight."),
    )
    .unwrap();
    let second_nebula = repo
        .insert_message(
            &second.session_id,
            CreateMessageRequest::new("assistant", "Nebula charts point east of the river."),
        )
        .unwrap();

    let local = repo.search_messages(&first.session_id, "nebula").unwrap();
    assert_eq!(local.len(), 1);
    assert_eq!(local[0].message_id, first_nebula.message_id);

    let hits = repo.search_across_sessions("nebula").unwrap();
    assert_eq!(hits.len(), 2);
    assert!(hits
        .iter()
        .all(|hit| hit.content.to_lowercase().contains("nebula")));

    let first_hit = hits
        .iter()
        .find(|hit| hit.message_id.as_str() == first_nebula.message_id)
        .unwrap();
    assert_eq!(first_hit.session.session_id, first.session_id);
    assert_eq!(first_hit.session.session_name, "Observatory Log");
    assert_eq!(first_hit.session.character_name.as_deref(), Some("Aster"));
    assert_eq!(first_hit.session.tags, vec!["stellar".to_owned()]);

    let second_hit = hits
        .iter()
        .find(|hit| hit.message_id.as_str() == second_nebula.message_id)
        .unwrap();
    assert_eq!(second_hit.session.session_id, second.session_id);
    assert_eq!(second_hit.session.session_name, "Village Log");
    assert_eq!(second_hit.session.character_name.as_deref(), Some("Mira"));
    assert_eq!(
        second_hit.session.tags,
        vec!["grounded".to_owned(), "phase2a".to_owned()]
    );
}

#[test]
fn pinned_memory_search_surfaces_note_memories_locally_and_globally() {
    let sandbox = TestSandbox::new("memory-fts-search");
    let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);

    let first = repo
        .create_session(CreateSessionRequest::new("Observatory Notes"))
        .unwrap();
    let second = repo
        .create_session(CreateSessionRequest::new("Village Notes"))
        .unwrap();

    repo.create_note_memory(
        &first.session_id,
        CreateNoteMemoryRequest::new(
            "Remember the observatory dome rendezvous point.",
            AuthorId::User,
            Provenance::UserAuthored,
        ),
    )
    .unwrap();
    repo.create_note_memory(
        &second.session_id,
        CreateNoteMemoryRequest::new(
            "Remember the orchard ladder behind the mill.",
            AuthorId::User,
            Provenance::UserAuthored,
        ),
    )
    .unwrap();

    let local_hits = repo
        .search_pinned_memories(&first.session_id, "observatory dome")
        .unwrap();
    assert_eq!(local_hits.len(), 1);
    assert_eq!(
        local_hits[0].memory.record.content.text,
        "Remember the observatory dome rendezvous point."
    );
    assert!(local_hits[0].memory.record.source_message_id.is_none());

    let global_hits = repo
        .search_pinned_memories_across_sessions("observatory dome")
        .unwrap();
    assert_eq!(global_hits.len(), 1);
    assert_eq!(global_hits[0].session.session_id, first.session_id);
    assert_eq!(
        global_hits[0].memory.record.content.text,
        "Remember the observatory dome rendezvous point."
    );
}

#[test]
fn plain_text_search_treats_hyphenated_terms_as_literals() {
    let sandbox = TestSandbox::new("hyphenated-search");
    let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);

    let first = repo
        .create_session(CreateSessionRequest::new("Hyphen Search A"))
        .unwrap();
    let second = repo
        .create_session(CreateSessionRequest::new("Hyphen Search B"))
        .unwrap();

    let keyword = "observatory-phase2a-validate";
    let first_message = repo
        .insert_message(
            &first.session_id,
            CreateMessageRequest::user(format!("The keyword is {keyword} in session A.")),
        )
        .unwrap();
    let second_message = repo
        .insert_message(
            &second.session_id,
            CreateMessageRequest::user(format!("Session B also stores {keyword}.")),
        )
        .unwrap();

    let local = repo.search_messages(&first.session_id, keyword).unwrap();
    assert_eq!(local.len(), 1);
    assert_eq!(local[0].message_id, first_message.message_id);

    let global = repo.search_across_sessions(keyword).unwrap();
    assert_eq!(global.len(), 2);
    assert!(global
        .iter()
        .any(|hit| hit.message_id.as_str() == first_message.message_id));
    assert!(global
        .iter()
        .any(|hit| hit.message_id.as_str() == second_message.message_id));
}

#[test]
fn set_message_bookmark_round_trips_and_clears() {
    let sandbox = TestSandbox::new("message-bookmarks");
    let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
    let session = repo
        .create_session(CreateSessionRequest::new("Bookmark Session"))
        .unwrap();
    let record = repo
        .insert_message(
            &session.session_id,
            CreateMessageRequest::user("remember this line"),
        )
        .unwrap();
    let message_id = MessageId::parse(&record.message_id).unwrap();

    let bookmark = repo
        .set_message_bookmark(
            &session.session_id,
            &message_id,
            true,
            Some("favorite".to_owned()),
        )
        .unwrap()
        .expect("bookmark should be created");
    assert_eq!(bookmark.note.as_deref(), Some("favorite"));

    let bookmarks = repo.list_bookmarks(&session.session_id).unwrap();
    assert_eq!(bookmarks.len(), 1);
    assert_eq!(bookmarks[0].message_id, message_id);
    assert_eq!(bookmarks[0].note.as_deref(), Some("favorite"));

    let conn = Connection::open(repo.paths().session_db_path(&session.session_id)).unwrap();
    let flagged: i64 = conn
        .query_row(
            "SELECT bookmarked FROM messages WHERE message_id = ?1",
            [record.message_id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(flagged, 1);

    let removed = repo
        .set_message_bookmark(&session.session_id, &message_id, false, None)
        .unwrap();
    assert_eq!(removed, None);
    assert!(repo.list_bookmarks(&session.session_id).unwrap().is_empty());
}

#[test]
fn advisory_session_lock_acquire_release_and_stale_takeover_work() {
    let sandbox = TestSandbox::new("session-locks");
    let (repo, clock) = test_repo(&sandbox, 1_725_647_200_000);
    let session = repo
        .create_session(CreateSessionRequest::new("Lock Session"))
        .unwrap();

    let first_lock = repo
        .acquire_session_lock(&session.session_id, "instance-a")
        .unwrap();
    assert_eq!(first_lock.instance_id, "instance-a");
    assert!(!repo
        .heartbeat_session_lock(&session.session_id, "instance-b")
        .unwrap());

    let error = repo
        .acquire_session_lock(&session.session_id, "instance-b")
        .unwrap_err();
    match error {
        PersistError::SessionLocked {
            instance_id,
            acquired_at,
        } => {
            assert_eq!(instance_id, "instance-a");
            assert_eq!(acquired_at, first_lock.acquired_at);
        }
        other => panic!("expected SessionLocked error, got {other:?}"),
    }

    clock.store(
        first_lock.heartbeat_at + STALE_LOCK_TIMEOUT_MS + 1,
        Ordering::SeqCst,
    );

    let second_lock = repo
        .acquire_session_lock(&session.session_id, "instance-b")
        .unwrap();
    assert_eq!(second_lock.instance_id, "instance-b");
    assert!(!repo
        .release_session_lock(&session.session_id, "instance-a")
        .unwrap());
    assert!(repo
        .release_session_lock(&session.session_id, "instance-b")
        .unwrap());

    let session_conn = Connection::open(repo.paths().session_db_path(&session.session_id)).unwrap();
    let lock_rows: i64 = session_conn
        .query_row("SELECT COUNT(*) FROM session_lock", [], |row| row.get(0))
        .unwrap();
    assert_eq!(lock_rows, 0);
}

#[test]
fn migrating_existing_session_db_creates_backup_before_schema_upgrade() {
    let sandbox = TestSandbox::new("session-backup");
    let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
    let session_id = SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap();
    let session_dir = repo.paths().session_dir(&session_id);
    fs::create_dir_all(&session_dir).unwrap();
    let session_db_path = repo.paths().session_db_path(&session_id);
    let legacy_conn = Connection::open(&session_db_path).unwrap();
    legacy_conn
        .execute("CREATE TABLE legacy_notes (value TEXT NOT NULL)", [])
        .unwrap();
    drop(legacy_conn);

    repo.ensure_session_database(&session_id).unwrap();

    assert!(backup_path(&session_db_path, 0).exists());

    let session_conn = Connection::open(&session_db_path).unwrap();
    let version: i64 = session_conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(version, i64::from(SESSION_SCHEMA_VERSION));
}

#[test]
fn branch_transcripts_follow_closure_paths() {
    let sandbox = TestSandbox::new("branch-transcripts");
    let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
    let session = repo
        .create_session(CreateSessionRequest::new("Branch Session"))
        .unwrap();

    let root = repo
        .insert_message(
            &session.session_id,
            CreateMessageRequest::user("Root prompt"),
        )
        .unwrap();
    let root_id = MessageId::parse(root.message_id).unwrap();
    let main_branch_id = branch_id("323e4567-e89b-12d3-a456-426614174000");
    let alt_branch_id = branch_id("423e4567-e89b-12d3-a456-426614174000");

    let mut main_branch = ConversationBranch::new(
        main_branch_id.clone(),
        session.session_id.clone(),
        "main",
        root_id.clone(),
        1_725_647_200_100,
    );
    main_branch.state = BranchState::Active;
    let created_main = repo
        .create_branch(CreateBranchCommand {
            branch: main_branch.clone(),
            forked_from: root_id.clone(),
        })
        .unwrap();
    assert_eq!(created_main.forked_from, root_id);
    assert_eq!(created_main.branch.state, BranchState::Active);

    let assistant_id = message_id("523e4567-e89b-12d3-a456-426614174000");
    let mut assistant = ConversationMessage::new(
        session.session_id.clone(),
        assistant_id.clone(),
        "assistant",
        "Assistant reply",
        1_725_647_200_200,
    );
    assistant.parent_id = Some(root_id.clone());
    repo.commit_message(CommitMessageCommand {
        branch_id: main_branch_id.clone(),
        message: assistant.clone(),
    })
    .unwrap();

    let user_follow_up_id = message_id("623e4567-e89b-12d3-a456-426614174000");
    let mut user_follow_up = ConversationMessage::new(
        session.session_id.clone(),
        user_follow_up_id.clone(),
        "user",
        "User follow-up",
        1_725_647_200_300,
    );
    user_follow_up.parent_id = Some(assistant_id.clone());
    repo.commit_message(CommitMessageCommand {
        branch_id: main_branch_id.clone(),
        message: user_follow_up.clone(),
    })
    .unwrap();

    let alt_branch = ConversationBranch::new(
        alt_branch_id.clone(),
        session.session_id.clone(),
        "alternate",
        assistant_id.clone(),
        1_725_647_200_350,
    );
    let created_alt = repo
        .create_branch(CreateBranchCommand {
            branch: alt_branch,
            forked_from: assistant_id.clone(),
        })
        .unwrap();
    assert_eq!(created_alt.forked_from, assistant_id);
    assert_eq!(created_alt.branch.state, BranchState::Inactive);

    let alt_assistant_id = message_id("723e4567-e89b-12d3-a456-426614174000");
    let mut alt_assistant = ConversationMessage::new(
        session.session_id.clone(),
        alt_assistant_id.clone(),
        "assistant",
        "Alternate assistant reply",
        1_725_647_200_400,
    );
    alt_assistant.parent_id = Some(assistant_id.clone());
    repo.commit_message(CommitMessageCommand {
        branch_id: alt_branch_id.clone(),
        message: alt_assistant.clone(),
    })
    .unwrap();

    let main_ids = transcript_ids(
        &repo
            .list_branch_messages(&session.session_id, &main_branch_id)
            .unwrap(),
    );
    assert_eq!(
        main_ids,
        vec![
            root_id.clone(),
            assistant_id.clone(),
            user_follow_up_id.clone()
        ]
    );

    let alt_ids = transcript_ids(
        &repo
            .list_branch_messages(&session.session_id, &alt_branch_id)
            .unwrap(),
    );
    assert_eq!(
        alt_ids,
        vec![
            root_id.clone(),
            assistant_id.clone(),
            alt_assistant_id.clone()
        ]
    );

    let active_ids = transcript_ids(
        &repo
            .get_active_branch_transcript(&session.session_id)
            .unwrap(),
    );
    assert_eq!(active_ids, main_ids);

    let session_conn = Connection::open(repo.paths().session_db_path(&session.session_id)).unwrap();
    assert_ancestry_depth(&session_conn, &root_id, &root_id, 0);
    assert_ancestry_depth(&session_conn, &assistant_id, &assistant_id, 0);
    assert_ancestry_depth(&session_conn, &root_id, &assistant_id, 1);
    assert_ancestry_depth(&session_conn, &root_id, &user_follow_up_id, 2);
    assert_ancestry_depth(&session_conn, &assistant_id, &user_follow_up_id, 1);
    assert_ancestry_depth(&session_conn, &assistant_id, &alt_assistant_id, 1);
}

#[test]
fn editing_messages_records_history_and_refreshes_search() {
    let sandbox = TestSandbox::new("message-edits");
    let (repo, clock) = test_repo(&sandbox, 1_725_647_200_000);
    let session = repo
        .create_session(CreateSessionRequest::new("Edit Session"))
        .unwrap();

    let record = repo
        .insert_message(
            &session.session_id,
            CreateMessageRequest::user("The comet is red"),
        )
        .unwrap();
    let message_id = MessageId::parse(record.message_id).unwrap();
    clock.store(1_725_647_260_000, Ordering::SeqCst);

    let edited = repo
        .edit_message(
            &session.session_id,
            &message_id,
            EditMessageRequest::new("The comet is blue"),
        )
        .unwrap();
    assert_eq!(edited.edited_at, Some(1_725_647_260_000));
    assert_eq!(edited.content, "The comet is blue");

    let history = repo
        .list_message_edits(&session.session_id, &message_id)
        .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].message_id, message_id);
    assert_eq!(history[0].previous_content, "The comet is red");
    assert_eq!(history[0].edited_at, 1_725_647_260_000);

    let new_hits = repo.search_messages(&session.session_id, "blue").unwrap();
    assert_eq!(new_hits.len(), 1);
    assert_eq!(new_hits[0].message_id, message_id.to_string());
    assert!(repo
        .search_messages(&session.session_id, "red")
        .unwrap()
        .is_empty());

    let global_conn = Connection::open(repo.paths().global_db_path()).unwrap();
    let stored_content: String = global_conn
        .query_row(
            "SELECT content FROM session_search WHERE session_id = ?1 AND message_id = ?2",
            params![session.session_id.as_str(), message_id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored_content, "The comet is blue");
}

#[test]
fn activating_a_branch_keeps_one_active_branch_per_session() {
    let sandbox = TestSandbox::new("branch-activation");
    let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
    let session = repo
        .create_session(CreateSessionRequest::new("Activation Session"))
        .unwrap();
    let root = repo
        .insert_message(&session.session_id, CreateMessageRequest::user("Root"))
        .unwrap();
    let root_id = MessageId::parse(root.message_id).unwrap();

    let first_branch_id = branch_id("823e4567-e89b-12d3-a456-426614174000");
    let second_branch_id = branch_id("923e4567-e89b-12d3-a456-426614174000");

    let mut first_branch = ConversationBranch::new(
        first_branch_id.clone(),
        session.session_id.clone(),
        "first",
        root_id.clone(),
        1_725_647_200_100,
    );
    first_branch.state = BranchState::Active;
    repo.create_branch(CreateBranchCommand {
        branch: first_branch,
        forked_from: root_id.clone(),
    })
    .unwrap();

    let second_branch = ConversationBranch::new(
        second_branch_id.clone(),
        session.session_id.clone(),
        "second",
        root_id.clone(),
        1_725_647_200_200,
    );
    repo.create_branch(CreateBranchCommand {
        branch: second_branch,
        forked_from: root_id.clone(),
    })
    .unwrap();

    assert_eq!(
        repo.get_active_branch(&session.session_id)
            .unwrap()
            .unwrap()
            .branch
            .branch_id,
        first_branch_id
    );

    let activated = repo
        .activate_branch(&session.session_id, &second_branch_id)
        .unwrap();
    assert_eq!(activated.branch.branch_id, second_branch_id);
    assert_eq!(activated.branch.state, BranchState::Active);

    let branches = repo.list_branches(&session.session_id).unwrap();
    let active_ids: Vec<_> = branches
        .iter()
        .filter(|branch| branch.branch.state == BranchState::Active)
        .map(|branch| branch.branch.branch_id.clone())
        .collect();
    assert_eq!(active_ids, vec![second_branch_id.clone()]);

    let first = branches
        .iter()
        .find(|branch| branch.branch.branch_id == first_branch_id)
        .unwrap();
    assert_eq!(first.branch.state, BranchState::Inactive);
}

#[test]
fn swipe_candidates_preserve_order_and_activation_state() {
    let sandbox = TestSandbox::new("swipe-state");
    let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
    let session = repo
        .create_session(CreateSessionRequest::new("Swipe Session"))
        .unwrap();
    let root = repo
        .insert_message(
            &session.session_id,
            CreateMessageRequest::user("Tell me a story"),
        )
        .unwrap();
    let root_id = MessageId::parse(root.message_id).unwrap();
    let branch_id = branch_id("a23e4567-e89b-12d3-a456-426614174000");

    let mut branch = ConversationBranch::new(
        branch_id.clone(),
        session.session_id.clone(),
        "main",
        root_id.clone(),
        1_725_647_200_050,
    );
    branch.state = BranchState::Active;
    repo.create_branch(CreateBranchCommand {
        branch,
        forked_from: root_id.clone(),
    })
    .unwrap();

    let visible_candidate_id = message_id("b23e4567-e89b-12d3-a456-426614174000");
    let mut visible_candidate = ConversationMessage::new(
        session.session_id.clone(),
        visible_candidate_id.clone(),
        "assistant",
        "Version one",
        1_725_647_200_100,
    );
    visible_candidate.parent_id = Some(root_id.clone());
    repo.commit_message(CommitMessageCommand {
        branch_id: branch_id.clone(),
        message: visible_candidate.clone(),
    })
    .unwrap();

    let swipe_group_id = swipe_group_id("c23e4567-e89b-12d3-a456-426614174000");
    let mut group = SwipeGroup::new(swipe_group_id.clone(), visible_candidate_id.clone());
    group.parent_context_message_id = Some(root_id.clone());
    group.active_ordinal = 0;
    repo.record_swipe_candidate(
        &session.session_id,
        RecordSwipeCandidateCommand {
            group: group.clone(),
            candidate: SwipeCandidate::new(swipe_group_id.clone(), 0, visible_candidate_id),
        },
    )
    .unwrap();

    let alternate_record = repo
        .insert_message(
            &session.session_id,
            CreateMessageRequest {
                parent_id: Some(root_id.to_string()),
                author_kind: "assistant".to_owned(),
                author_name: None,
                content: "Version two".to_owned(),
            },
        )
        .unwrap();
    let alternate_id = MessageId::parse(alternate_record.message_id).unwrap();
    repo.record_swipe_candidate(
        &session.session_id,
        RecordSwipeCandidateCommand {
            group: group.clone(),
            candidate: SwipeCandidate::new(swipe_group_id.clone(), 1, alternate_id.clone()),
        },
    )
    .unwrap();

    let partial_record = repo
        .insert_message(
            &session.session_id,
            CreateMessageRequest {
                parent_id: Some(root_id.to_string()),
                author_kind: "assistant".to_owned(),
                author_name: None,
                content: "Version three (partial)".to_owned(),
            },
        )
        .unwrap();
    let partial_id = MessageId::parse(partial_record.message_id).unwrap();
    let mut partial_candidate = SwipeCandidate::new(swipe_group_id.clone(), 2, partial_id);
    partial_candidate.state = SwipeCandidateState::FailedMidStream;
    partial_candidate.partial_content = Some("Version three".to_owned());
    partial_candidate.tokens_generated = Some(17);
    repo.record_swipe_candidate(
        &session.session_id,
        RecordSwipeCandidateCommand {
            group: group.clone(),
            candidate: partial_candidate.clone(),
        },
    )
    .unwrap();

    let groups = repo.list_swipe_groups(&session.session_id).unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].swipe_group_id, swipe_group_id);
    assert_eq!(groups[0].parent_context_message_id, Some(root_id.clone()));
    assert_eq!(groups[0].active_ordinal, 0);

    let candidates = repo
        .list_swipe_candidates(&session.session_id, &swipe_group_id)
        .unwrap();
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.ordinal)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(candidates[1].message_id, alternate_id);
    assert_eq!(candidates[2].state, SwipeCandidateState::FailedMidStream);
    assert_eq!(
        candidates[2].partial_content.as_deref(),
        Some("Version three")
    );
    assert_eq!(candidates[2].tokens_generated, Some(17));

    let activated = repo
        .activate_swipe_candidate(
            &session.session_id,
            ActivateSwipeCommand {
                swipe_group_id: swipe_group_id.clone(),
                ordinal: 1,
            },
        )
        .unwrap();
    assert_eq!(activated.active_ordinal, 1);
    assert_eq!(
        repo.get_swipe_group(&session.session_id, &swipe_group_id)
            .unwrap()
            .unwrap()
            .active_ordinal,
        1
    );
    assert_eq!(
        repo.list_swipe_candidates(&session.session_id, &swipe_group_id)
            .unwrap()
            .iter()
            .map(|candidate| candidate.ordinal)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
}

#[test]
fn importing_character_card_stores_artifact_and_seeds_greeting() {
    let sandbox = TestSandbox::new("character-card-import");
    let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);

    let imported = repo
        .import_character_card(crate::ImportCharacterCardRequest {
            card: crate::CharacterCard::from_json_str(
                r#"{
                        "name": "Aster",
                        "description": "A patient observatory guide.",
                        "first_mes": "Welcome back to the observatory.",
                        "tags": ["stellar"]
                    }"#,
            )
            .unwrap(),
            session_name: Some("Aster Intake".to_owned()),
            tags: vec!["phase1f".to_owned()],
            provenance: "tests/cards/aster.json".to_owned(),
        })
        .unwrap();

    assert_eq!(imported.session.name, "Aster Intake");
    assert_eq!(imported.session.character_name.as_deref(), Some("Aster"));
    assert_eq!(
        imported.session.tags,
        vec!["stellar".to_owned(), "phase1f".to_owned()]
    );
    assert!(imported.seeded_branch_id.is_some());
    assert!(imported.seeded_message_id.is_some());

    let stored = repo
        .get_character_card(&imported.session.session_id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.provenance, "tests/cards/aster.json");
    assert_eq!(stored.card.name, "Aster");

    let transcript = repo
        .get_active_branch_transcript(&imported.session.session_id)
        .unwrap();
    assert_eq!(transcript.len(), 1);
    assert_eq!(transcript[0].author_kind, "assistant");
    assert_eq!(transcript[0].author_name.as_deref(), Some("Aster"));
    assert_eq!(transcript[0].content, "Welcome back to the observatory.");
}

#[test]
fn session_export_includes_character_cards_bookmarks_and_swipes() {
    let sandbox = TestSandbox::new("session-export");
    let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);

    let imported = repo
        .import_character_card(crate::ImportCharacterCardRequest {
            card: crate::CharacterCard::from_json_str(
                r#"{
                        "name": "Aster",
                        "description": "A patient observatory guide.",
                        "first_mes": "Welcome back to the observatory."
                    }"#,
            )
            .unwrap(),
            session_name: None,
            tags: vec!["phase1f".to_owned()],
            provenance: "tests/cards/aster.json".to_owned(),
        })
        .unwrap();
    let session_id = imported.session.session_id.clone();
    let greeting_id = imported.seeded_message_id.clone().unwrap();

    repo.set_message_bookmark(&session_id, &greeting_id, true, Some("opening".to_owned()))
        .unwrap();

    let swipe_group_id = swipe_group_id("d23e4567-e89b-12d3-a456-426614174000");
    repo.record_swipe_candidate(
        &session_id,
        RecordSwipeCandidateCommand {
            group: SwipeGroup::new(swipe_group_id.clone(), greeting_id.clone()),
            candidate: SwipeCandidate::new(swipe_group_id.clone(), 0, greeting_id.clone()),
        },
    )
    .unwrap();
    let alternate = repo
        .insert_message(
            &session_id,
            CreateMessageRequest {
                parent_id: None,
                author_kind: "assistant".to_owned(),
                author_name: Some("Aster".to_owned()),
                content: "The stars have shifted since your last visit.".to_owned(),
            },
        )
        .unwrap();
    repo.record_swipe_candidate(
        &session_id,
        RecordSwipeCandidateCommand {
            group: SwipeGroup::new(swipe_group_id.clone(), greeting_id.clone()),
            candidate: SwipeCandidate::new(
                swipe_group_id.clone(),
                1,
                MessageId::parse(alternate.message_id).unwrap(),
            ),
        },
    )
    .unwrap();

    let export = repo.export_session(&session_id).unwrap();
    let json = export.to_pretty_json().unwrap();

    assert_eq!(export.format, SESSION_EXPORT_FORMAT);
    assert_eq!(export.session.session_id, session_id.to_string());
    assert_eq!(export.character_card.as_ref().unwrap().card.name, "Aster");
    assert_eq!(export.branches.len(), 1);
    assert_eq!(
        export.branches[0].transcript_message_ids,
        vec![greeting_id.to_string()]
    );
    assert_eq!(export.messages.len(), 2);
    assert_eq!(export.bookmarks.len(), 1);
    assert_eq!(export.swipe_groups.len(), 1);
    assert_eq!(export.swipe_groups[0].candidates.len(), 2);
    assert!(json.contains("\"format\": \"ozone-plus.session-export.v1\""));
}

#[test]
fn transcript_export_preserves_branch_message_order() {
    let sandbox = TestSandbox::new("transcript-export");
    let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
    let session = repo
        .create_session(CreateSessionRequest::new("Transcript Export Session"))
        .unwrap();
    let root = repo
        .insert_message(
            &session.session_id,
            CreateMessageRequest::user("Root prompt"),
        )
        .unwrap();
    let root_id = MessageId::parse(root.message_id).unwrap();
    let branch_id = branch_id("e23e4567-e89b-12d3-a456-426614174000");
    let mut branch = ConversationBranch::new(
        branch_id.clone(),
        session.session_id.clone(),
        "main",
        root_id.clone(),
        1_725_647_200_050,
    );
    branch.state = BranchState::Active;
    repo.create_branch(CreateBranchCommand {
        branch,
        forked_from: root_id.clone(),
    })
    .unwrap();

    let assistant_id = message_id("f23e4567-e89b-12d3-a456-426614174000");
    let mut assistant = ConversationMessage::new(
        session.session_id.clone(),
        assistant_id.clone(),
        "assistant",
        "Assistant reply",
        1_725_647_200_100,
    );
    assistant.parent_id = Some(root_id.clone());
    repo.commit_message(CommitMessageCommand {
        branch_id: branch_id.clone(),
        message: assistant,
    })
    .unwrap();

    let user_follow_up_id = message_id("123e4567-e89b-42d3-a456-426614174000");
    let mut user_follow_up = ConversationMessage::new(
        session.session_id.clone(),
        user_follow_up_id.clone(),
        "user",
        "User follow-up",
        1_725_647_200_150,
    );
    user_follow_up.parent_id = Some(assistant_id.clone());
    repo.commit_message(CommitMessageCommand {
        branch_id: branch_id.clone(),
        message: user_follow_up,
    })
    .unwrap();

    let export = repo
        .export_transcript(&session.session_id, Some(&branch_id))
        .unwrap();
    let ids = export
        .messages
        .iter()
        .map(|message| message.message_id.clone())
        .collect::<Vec<_>>();

    assert_eq!(export.format, TRANSCRIPT_EXPORT_FORMAT);
    assert_eq!(
        export.branch.as_ref().unwrap().branch_id,
        branch_id.to_string()
    );
    assert_eq!(
        ids,
        vec![
            root_id.to_string(),
            assistant_id.to_string(),
            user_follow_up_id.to_string()
        ]
    );
}

fn assert_sqlite_object(conn: &Connection, kind: &str, name: &str) {
    let exists: i64 = conn
        .query_row(
            "SELECT EXISTS(
                    SELECT 1
                    FROM sqlite_master
                    WHERE type = ?1 AND name = ?2
                )",
            params![kind, name],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(exists, 1, "missing {kind} {name}");
}

fn assert_ancestry_depth(
    conn: &Connection,
    ancestor_id: &MessageId,
    descendant_id: &MessageId,
    expected_depth: i64,
) {
    let depth: i64 = conn
        .query_row(
            "SELECT depth
                 FROM message_ancestry
                 WHERE ancestor_id = ?1 AND descendant_id = ?2",
            params![ancestor_id.as_str(), descendant_id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(depth, expected_depth);
}

fn transcript_ids(messages: &[ConversationMessage]) -> Vec<MessageId> {
    messages
        .iter()
        .map(|message| message.message_id.clone())
        .collect()
}

fn message_id(value: &str) -> MessageId {
    MessageId::parse(value).unwrap()
}

fn branch_id(value: &str) -> BranchId {
    BranchId::parse(value).unwrap()
}

fn swipe_group_id(value: &str) -> SwipeGroupId {
    SwipeGroupId::parse(value).unwrap()
}

fn embedding_record(
    artifact_id: &str,
    session_id: &SessionId,
    source_message_id: Option<&MessageId>,
    vector: Vec<f32>,
    text: &str,
    created_at: i64,
    snapshot_version: u64,
) -> EmbeddingRecord {
    EmbeddingRecord {
        artifact_id: MemoryArtifactId::parse(artifact_id).unwrap(),
        session_id: session_id.clone(),
        content: ozone_memory::EmbeddingContent::new(vector, source_text_hash(text)),
        source_message_id: source_message_id.cloned(),
        provenance: Provenance::UserAuthored,
        created_at,
        snapshot_version,
        metadata: EmbeddingRecordMetadata {
            provider: EmbeddingProviderKind::Mock,
            model: "mock/stable".to_owned(),
            dimensions: 3,
        },
    }
}

pub(super) fn test_repo(
    sandbox: &TestSandbox,
    initial_time: i64,
) -> (SqliteRepository, Arc<AtomicI64>) {
    let clock = Arc::new(AtomicI64::new(initial_time));
    let clock_for_repo = Arc::clone(&clock);
    let repo = SqliteRepository::with_clock(
        PersistencePaths::from_data_dir(sandbox.data_dir()),
        Arc::new(move || clock_for_repo.load(Ordering::SeqCst)),
    );
    (repo, clock)
}

pub(super) struct TestSandbox {
    root: PathBuf,
}

impl TestSandbox {
    pub(super) fn new(prefix: &str) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("ozone-persist-tests")
            .join(format!(
                "{prefix}-{}-{}",
                std::process::id(),
                SANDBOX_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));

        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }

        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn data_dir(&self) -> PathBuf {
        self.root.join("data")
    }
}

impl Drop for TestSandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn store_and_list_chunk_summaries() {
    let sandbox = TestSandbox::new("chunk-summaries");
    let (repo, clock) = test_repo(&sandbox, 1_725_647_200_000);
    let session = repo
        .create_session(CreateSessionRequest::new("Summary Session"))
        .unwrap();

    let msg1 = repo
        .insert_message(
            &session.session_id,
            CreateMessageRequest::user("First message"),
        )
        .unwrap();
    let msg2 = repo
        .insert_message(
            &session.session_id,
            CreateMessageRequest::user("Second message"),
        )
        .unwrap();
    let msg3 = repo
        .insert_message(
            &session.session_id,
            CreateMessageRequest::user("Third message"),
        )
        .unwrap();

    let start1 = message_id(&msg1.message_id);
    let end1 = message_id(&msg2.message_id);
    let start2 = message_id(&msg2.message_id);
    let end2 = message_id(&msg3.message_id);

    let first = repo
        .store_chunk_summary(
            &session.session_id,
            "Alice greeted Bob.",
            2,
            &start1,
            &end1,
            1,
        )
        .unwrap();
    assert_eq!(first.kind, "chunk_summary");
    assert_eq!(first.text, "Alice greeted Bob.");
    assert_eq!(first.source_count, Some(2));
    assert_eq!(first.message_count, None);

    clock.store(1_725_647_201_000, Ordering::SeqCst);
    let second = repo
        .store_chunk_summary(
            &session.session_id,
            "They discussed the plan.",
            3,
            &start2,
            &end2,
            3,
        )
        .unwrap();

    let summaries = repo.list_chunk_summaries(&session.session_id).unwrap();
    assert_eq!(summaries.len(), 2);
    // Ordered by created_at DESC — second is first
    assert_eq!(summaries[0].artifact_id, second.artifact_id);
    assert_eq!(summaries[0].text, "They discussed the plan.");
    assert_eq!(summaries[0].source_count, Some(3));
    assert_eq!(
        summaries[0].start_message_id.as_deref(),
        Some(start2.as_str())
    );
    assert_eq!(summaries[0].end_message_id.as_deref(), Some(end2.as_str()));

    assert_eq!(summaries[1].artifact_id, first.artifact_id);
    assert_eq!(summaries[1].text, "Alice greeted Bob.");
    assert_eq!(summaries[1].source_count, Some(2));
}

#[test]
fn store_and_get_session_synopsis() {
    let sandbox = TestSandbox::new("session-synopsis");
    let (repo, clock) = test_repo(&sandbox, 1_725_647_200_000);
    let session = repo
        .create_session(CreateSessionRequest::new("Synopsis Session"))
        .unwrap();

    assert!(repo
        .get_latest_session_synopsis(&session.session_id)
        .unwrap()
        .is_none());

    let first = repo
        .store_session_synopsis(
            &session.session_id,
            "A roleplay about forest exploration.",
            10,
            5,
        )
        .unwrap();
    assert_eq!(first.kind, "session_synopsis");
    assert_eq!(first.text, "A roleplay about forest exploration.");
    assert_eq!(first.message_count, Some(10));
    assert_eq!(first.source_count, None);

    clock.store(1_725_647_201_000, Ordering::SeqCst);
    let second = repo
        .store_session_synopsis(
            &session.session_id,
            "Updated synopsis with more detail.",
            20,
            10,
        )
        .unwrap();

    let latest = repo
        .get_latest_session_synopsis(&session.session_id)
        .unwrap()
        .expect("should find a synopsis");
    assert_eq!(latest.artifact_id, second.artifact_id);
    assert_eq!(latest.text, "Updated synopsis with more detail.");
    assert_eq!(latest.message_count, Some(20));
    assert_eq!(latest.snapshot_version, 10);
}

#[test]
fn delete_summary_artifact_removes_only_summaries() {
    let sandbox = TestSandbox::new("delete-summary");
    let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
    let session = repo
        .create_session(CreateSessionRequest::new("Delete Summary"))
        .unwrap();

    let msg = repo
        .insert_message(
            &session.session_id,
            CreateMessageRequest::user("Some message"),
        )
        .unwrap();
    let msg_id = message_id(&msg.message_id);

    let chunk = repo
        .store_chunk_summary(&session.session_id, "A chunk.", 1, &msg_id, &msg_id, 1)
        .unwrap();
    let synopsis = repo
        .store_session_synopsis(&session.session_id, "A synopsis.", 5, 2)
        .unwrap();

    assert!(repo
        .delete_summary_artifact(&session.session_id, &chunk.artifact_id)
        .unwrap());
    // Deleting again returns false
    assert!(!repo
        .delete_summary_artifact(&session.session_id, &chunk.artifact_id)
        .unwrap());

    // Chunk is gone
    let remaining = repo.list_chunk_summaries(&session.session_id).unwrap();
    assert!(remaining.is_empty());

    // Synopsis still there
    let latest = repo
        .get_latest_session_synopsis(&session.session_id)
        .unwrap();
    assert!(latest.is_some());

    // Delete synopsis
    assert!(repo
        .delete_summary_artifact(&session.session_id, &synopsis.artifact_id)
        .unwrap());
    assert!(repo
        .get_latest_session_synopsis(&session.session_id)
        .unwrap()
        .is_none());
}

#[test]
fn derived_artifact_inventory_reports_source_existence_and_previews() {
    let sandbox = TestSandbox::new("derived-artifact-inventory");
    let (repo, clock) = test_repo(&sandbox, 1_725_647_200_000);
    let session = repo
        .create_session(CreateSessionRequest::new("Derived Inventory"))
        .unwrap();

    let first = repo
        .insert_message(
            &session.session_id,
            CreateMessageRequest::user("The lantern stays by the observatory door."),
        )
        .unwrap();
    let second = repo
        .insert_message(
            &session.session_id,
            CreateMessageRequest::user("Bring the spare key before dusk."),
        )
        .unwrap();
    let first_message_id = message_id(&first.message_id);
    let second_message_id = message_id(&second.message_id);
    let missing_message_id = message_id("c23e4567-e89b-12d3-a456-426614174000");

    let embedding = embedding_record(
        "c33e4567-e89b-12d3-a456-426614174000",
        &session.session_id,
        Some(&first_message_id),
        vec![0.1, 0.2, 0.3],
        "The lantern stays by the observatory door.",
        1_725_647_200_000,
        1,
    );
    repo.upsert_embedding_artifacts(std::slice::from_ref(&embedding))
        .unwrap();

    clock.store(1_725_647_200_010, Ordering::SeqCst);
    let chunk = repo
        .store_chunk_summary(
            &session.session_id,
            "Lantern location confirmed.",
            2,
            &first_message_id,
            &second_message_id,
            2,
        )
        .unwrap();

    clock.store(1_725_647_200_020, Ordering::SeqCst);
    let synopsis = repo
        .store_session_synopsis(
            &session.session_id,
            "A short exchange about lantern placement.",
            2,
            2,
        )
        .unwrap();

    let conn = repo.open_session_connection(&session.session_id).unwrap();
    conn.execute(
        "UPDATE memory_artifacts
             SET source_end_message_id = ?2
             WHERE artifact_id = ?1",
        params![chunk.artifact_id.as_str(), missing_message_id.as_str()],
    )
    .unwrap();

    let artifacts = repo
        .list_derived_artifacts(Some(&session.session_id))
        .unwrap();
    assert_eq!(artifacts.len(), 3);

    assert_eq!(artifacts[0].artifact_id, embedding.artifact_id);
    assert_eq!(artifacts[0].kind, DerivedArtifactKind::Embedding);
    assert_eq!(
        artifacts[0].source_start_message_id.as_ref(),
        Some(&first_message_id)
    );
    assert_eq!(
        artifacts[0].source_end_message_id.as_ref(),
        Some(&first_message_id)
    );
    assert!(artifacts[0].source_exists);
    assert_eq!(artifacts[0].text_preview, None);

    assert_eq!(artifacts[1].artifact_id, chunk.artifact_id);
    assert_eq!(artifacts[1].kind, DerivedArtifactKind::ChunkSummary);
    assert_eq!(
        artifacts[1].text_preview.as_deref(),
        Some("Lantern location confirmed.")
    );
    assert_eq!(
        artifacts[1].source_start_message_id.as_ref(),
        Some(&first_message_id)
    );
    assert_eq!(
        artifacts[1].source_end_message_id.as_ref(),
        Some(&missing_message_id)
    );
    assert!(!artifacts[1].source_exists);

    assert_eq!(artifacts[2].artifact_id, synopsis.artifact_id);
    assert_eq!(artifacts[2].kind, DerivedArtifactKind::SessionSynopsis);
    assert_eq!(
        artifacts[2].text_preview.as_deref(),
        Some("A short exchange about lantern placement.")
    );
    assert!(artifacts[2].source_exists);
    assert!(artifacts[2].source_start_message_id.is_none());
    assert!(artifacts[2].source_end_message_id.is_none());
}

#[test]
fn minimal_tier_plan_marks_embeddings_chunks_and_superseded_synopses() {
    let sandbox = TestSandbox::new("gc-minimal-tier");
    let (repo, clock) = test_repo(&sandbox, 1_725_647_200_000);
    let session = repo
        .create_session(CreateSessionRequest::new("Minimal Tier Session"))
        .unwrap();

    let messages = (0..5)
        .map(|index| {
            repo.insert_message(
                &session.session_id,
                CreateMessageRequest::user(format!("Message #{index}")),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let first_message_id = message_id(&messages[0].message_id);
    let second_message_id = message_id(&messages[1].message_id);

    let embedding = embedding_record(
        "d33e4567-e89b-12d3-a456-426614174000",
        &session.session_id,
        Some(&first_message_id),
        vec![0.2, 0.3, 0.4],
        "Message #0",
        1_725_647_200_000,
        1,
    );
    repo.upsert_embedding_artifacts(std::slice::from_ref(&embedding))
        .unwrap();

    clock.store(1_725_647_200_010, Ordering::SeqCst);
    let chunk = repo
        .store_chunk_summary(
            &session.session_id,
            "Messages summarized.",
            2,
            &first_message_id,
            &second_message_id,
            1,
        )
        .unwrap();

    clock.store(1_725_647_200_020, Ordering::SeqCst);
    let old_synopsis = repo
        .store_session_synopsis(&session.session_id, "Older synopsis.", 2, 1)
        .unwrap();

    clock.store(1_725_647_200_030, Ordering::SeqCst);
    let new_synopsis = repo
        .store_session_synopsis(&session.session_id, "Newest synopsis.", 5, 4)
        .unwrap();

    let plan = repo
        .plan_garbage_collection(
            Some(&session.session_id),
            &StorageTierPolicy::new(1, 2),
            500,
            168,
            &GarbageCollectionPolicy::new(10, false),
        )
        .unwrap();
    let candidates = candidate_reasons_by_artifact(&plan);

    assert_eq!(plan.inspected_count, 4);
    assert_eq!(plan.candidate_count, 3);
    assert_eq!(
        plan.reason_counts
            .get(&GarbageCollectionReason::MinimalTier),
        Some(&2)
    );
    assert_eq!(
        plan.reason_counts
            .get(&GarbageCollectionReason::SupersededSynopsis),
        Some(&1)
    );
    assert_eq!(
        candidates.get(&(session.session_id.clone(), embedding.artifact_id.clone())),
        Some(&vec![GarbageCollectionReason::MinimalTier])
    );
    assert_eq!(
        candidates.get(&(session.session_id.clone(), chunk.artifact_id.clone())),
        Some(&vec![GarbageCollectionReason::MinimalTier])
    );
    assert_eq!(
        candidates.get(&(session.session_id.clone(), old_synopsis.artifact_id.clone())),
        Some(&vec![GarbageCollectionReason::SupersededSynopsis])
    );
    assert!(!candidates.contains_key(&(session.session_id, new_synopsis.artifact_id)));
}

#[test]
fn orphaned_source_cleanup_marks_candidates_when_enabled() {
    let sandbox = TestSandbox::new("gc-orphaned-source");
    let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
    let session = repo
        .create_session(CreateSessionRequest::new("Orphan Cleanup"))
        .unwrap();
    let message = repo
        .insert_message(
            &session.session_id,
            CreateMessageRequest::user("Archive the observatory route."),
        )
        .unwrap();
    let source_message_id = message_id(&message.message_id);
    let missing_message_id = message_id("e33e4567-e89b-12d3-a456-426614174000");

    let embedding = embedding_record(
        "f33e4567-e89b-12d3-a456-426614174000",
        &session.session_id,
        Some(&source_message_id),
        vec![0.3, 0.2, 0.1],
        "Archive the observatory route.",
        1_725_647_200_000,
        1,
    );
    repo.upsert_embedding_artifacts(std::slice::from_ref(&embedding))
        .unwrap();

    let conn = repo.open_session_connection(&session.session_id).unwrap();
    conn.execute(
        "UPDATE memory_artifacts
             SET source_start_message_id = ?2, source_end_message_id = ?3
             WHERE artifact_id = ?1",
        params![
            embedding.artifact_id.as_str(),
            missing_message_id.as_str(),
            missing_message_id.as_str()
        ],
    )
    .unwrap();

    let plan = repo
        .plan_garbage_collection(
            Some(&session.session_id),
            &StorageTierPolicy::new(100, 1_000),
            500,
            168,
            &GarbageCollectionPolicy::new(10, true),
        )
        .unwrap();

    assert_eq!(plan.candidate_count, 1);
    assert_eq!(
        plan.reason_counts
            .get(&GarbageCollectionReason::OrphanedSource),
        Some(&1)
    );
    assert_eq!(
        plan.candidates[0].artifact.artifact_id,
        embedding.artifact_id
    );
    assert_eq!(
        plan.candidates[0].reasons,
        vec![GarbageCollectionReason::OrphanedSource]
    );
    assert!(!plan.candidates[0].artifact.source_exists);
}

#[test]
fn embedding_cap_marks_oldest_remaining_embeddings() {
    let sandbox = TestSandbox::new("gc-embedding-cap");
    let (repo, _) = test_repo(&sandbox, 1_725_647_200_000);
    let session = repo
        .create_session(CreateSessionRequest::new("Embedding Cap"))
        .unwrap();

    let first = embedding_record(
        "133e4567-e89b-12d3-a456-426614174000",
        &session.session_id,
        None,
        vec![0.1, 0.0, 0.0],
        "First embedding",
        1_725_647_200_000,
        0,
    );
    let second = embedding_record(
        "233e4567-e89b-12d3-a456-426614174000",
        &session.session_id,
        None,
        vec![0.0, 0.1, 0.0],
        "Second embedding",
        1_725_647_200_010,
        0,
    );
    let third = embedding_record(
        "333e4567-e89b-12d3-a456-426614174000",
        &session.session_id,
        None,
        vec![0.0, 0.0, 0.1],
        "Third embedding",
        1_725_647_200_020,
        0,
    );
    repo.upsert_embedding_artifacts(&[first.clone(), second.clone(), third.clone()])
        .unwrap();

    let plan = repo
        .plan_garbage_collection(
            Some(&session.session_id),
            &StorageTierPolicy::new(100, 1_000),
            500,
            168,
            &GarbageCollectionPolicy::new(2, false),
        )
        .unwrap();

    assert_eq!(plan.candidate_count, 1);
    assert_eq!(
        plan.reason_counts
            .get(&GarbageCollectionReason::OverEmbeddingLimit),
        Some(&1)
    );
    assert_eq!(plan.candidates[0].artifact.artifact_id, first.artifact_id);
    assert_eq!(
        plan.candidates[0].reasons,
        vec![GarbageCollectionReason::OverEmbeddingLimit]
    );
}

#[test]
fn applying_gc_plan_deletes_only_planned_derived_artifacts() {
    let sandbox = TestSandbox::new("gc-apply");
    let (repo, clock) = test_repo(&sandbox, 1_725_647_200_000);
    let session = repo
        .create_session(CreateSessionRequest::new("Apply GC"))
        .unwrap();

    let messages = (0..4)
        .map(|index| {
            repo.insert_message(
                &session.session_id,
                CreateMessageRequest::user(format!("Apply message #{index}")),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let first_message_id = message_id(&messages[0].message_id);
    let second_message_id = message_id(&messages[1].message_id);

    let pinned = repo
        .pin_message_memory(
            &session.session_id,
            &first_message_id,
            PinMessageMemoryRequest::new(AuthorId::User, Provenance::UserAuthored),
        )
        .unwrap();

    let embedding = embedding_record(
        "433e4567-e89b-12d3-a456-426614174000",
        &session.session_id,
        Some(&first_message_id),
        vec![0.9, 0.1, 0.2],
        "Apply message #0",
        1_725_647_200_000,
        1,
    );
    repo.upsert_embedding_artifacts(std::slice::from_ref(&embedding))
        .unwrap();

    clock.store(1_725_647_200_010, Ordering::SeqCst);
    let chunk = repo
        .store_chunk_summary(
            &session.session_id,
            "Chunk ready for cleanup.",
            2,
            &first_message_id,
            &second_message_id,
            1,
        )
        .unwrap();

    clock.store(1_725_647_200_020, Ordering::SeqCst);
    let old_synopsis = repo
        .store_session_synopsis(&session.session_id, "Old synopsis.", 2, 1)
        .unwrap();

    clock.store(1_725_647_200_030, Ordering::SeqCst);
    let new_synopsis = repo
        .store_session_synopsis(&session.session_id, "Current synopsis.", 4, 4)
        .unwrap();

    let plan = repo
        .plan_garbage_collection(
            Some(&session.session_id),
            &StorageTierPolicy::new(1, 2),
            500,
            168,
            &GarbageCollectionPolicy::new(10, false),
        )
        .unwrap();
    assert_eq!(plan.candidate_count, 3);

    let outcome = repo.apply_garbage_collection_plan(&plan).unwrap();
    assert_eq!(outcome.deleted_count, 3);
    assert_eq!(
        outcome
            .deleted_artifact_ids
            .get(&session.session_id)
            .map(Vec::len),
        Some(3)
    );

    assert!(repo
        .list_embedding_artifacts(Some(&session.session_id))
        .unwrap()
        .is_empty());
    assert!(repo
        .list_chunk_summaries(&session.session_id)
        .unwrap()
        .is_empty());
    assert_eq!(
        repo.get_latest_session_synopsis(&session.session_id)
            .unwrap()
            .map(|artifact| artifact.artifact_id),
        Some(new_synopsis.artifact_id.clone())
    );
    let pinned_memories = repo.list_pinned_memories(&session.session_id).unwrap();
    assert_eq!(pinned_memories.len(), 1);
    assert_eq!(pinned_memories[0].record.artifact_id, pinned.artifact_id);

    let remaining_derived = repo
        .list_derived_artifacts(Some(&session.session_id))
        .unwrap();
    assert_eq!(remaining_derived.len(), 1);
    assert_eq!(remaining_derived[0].artifact_id, new_synopsis.artifact_id);
    assert_eq!(
        remaining_derived[0].kind,
        DerivedArtifactKind::SessionSynopsis
    );

    let deleted_ids = outcome
        .deleted_artifact_ids
        .get(&session.session_id)
        .unwrap();
    assert!(deleted_ids.contains(&embedding.artifact_id));
    assert!(deleted_ids.contains(&chunk.artifact_id));
    assert!(deleted_ids.contains(&old_synopsis.artifact_id));
}

fn candidate_reasons_by_artifact(
    plan: &GarbageCollectionPlan,
) -> BTreeMap<(SessionId, MemoryArtifactId), Vec<GarbageCollectionReason>> {
    plan.candidates
        .iter()
        .map(|candidate| {
            (
                (
                    candidate.artifact.session_id.clone(),
                    candidate.artifact.artifact_id.clone(),
                ),
                candidate.reasons.clone(),
            )
        })
        .collect()
}
