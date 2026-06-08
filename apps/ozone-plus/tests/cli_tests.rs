use clap::Parser;
use ozone_memory::VectorIndexManager;
use ozone_persist::{
    AuthorId, CreateMessageRequest,
    CreateNoteMemoryRequest, CreateSessionRequest,
    PersistencePaths, Provenance, SessionId,
    SqliteRepository, SwipeCandidateState,
};
use ozone_core::engine::{BranchState, MessageId};
use std::{
    fs,
    path::Path,
    process::Command as ProcessCommand,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};

// Re-import the app's CLI module items
use ozone_plus::cli;
use ozone_plus::cli::args::*;
use ozone_plus::cli::util::*;
use ozone_plus::cli::print::*;
use ozone_plus::run_cli;
use ozone_plus::store::{ManualSwipeCandidateRequest, RepoConversationStore};
use ozone_plus::runtime::OzonePlusRuntime;
use ozone_tui::mock::SessionRuntime;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);
static ENV_LOCK: Mutex<()> = Mutex::new(());

struct TestSandbox {
    root: std::path::PathBuf,
}

impl TestSandbox {
    fn new(prefix: &str) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("ozone-plus-tests")
            .join(format!(
                "{prefix}-{}-{}",
                std::process::id(),
                TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn repo(&self) -> SqliteRepository {
        SqliteRepository::new(PersistencePaths::from_data_dir(self.root.clone()))
    }

    fn xdg_data_home(&self) -> std::path::PathBuf {
        self.root.join("xdg-data")
    }

    fn xdg_config_home(&self) -> std::path::PathBuf {
        self.root.join("xdg-config")
    }

    fn global_config_path(&self) -> std::path::PathBuf {
        self.xdg_config_home().join("ozone").join("config.toml")
    }
}

impl Drop for TestSandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct ScopedEnvVar {
    key: &'static str,
    previous: Option<String>,
}

impl ScopedEnvVar {
    fn set(key: &'static str, value: impl AsRef<Path>) -> Self {
        let previous = std::env::var(key).ok();
        std::env::set_var(key, value.as_ref());
        Self { key, previous }
    }
}

impl Drop for ScopedEnvVar {
    fn drop(&mut self) {
        match self.previous.as_ref() {
            Some(previous) => std::env::set_var(self.key, previous),
            None => std::env::remove_var(self.key),
        }
    }
}

fn write_global_config(sandbox: &TestSandbox, contents: &str) {
    let path = sandbox.global_config_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

#[test]
fn formatted_search_report_surfaces_mode_and_breakdown() {
    let result = ozone_memory::RetrievalResultSet {
        query: "observatory key".into(),
        status: ozone_memory::RetrievalStatus {
            mode: ozone_memory::RetrievalSearchMode::Hybrid,
            reason: None,
            filtered_stale_embeddings: 1,
            downranked_embeddings: 0,
        },
        hits: vec![ozone_memory::RetrievalHit {
            session: ozone_memory::SearchSessionMetadata {
                session_id: SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap(),
                session_name: "Observatory".into(),
                character_name: Some("Aster".into()),
                tags: vec!["phase2b".into()],
            },
            hit_kind: ozone_memory::RetrievalHitKind::Message,
            artifact_id: None,
            message_id: Some(MessageId::parse("223e4567-e89b-12d3-a456-426614174000").unwrap()),
            source_message_id: None,
            author_kind: Some("assistant".into()),
            text: "The key rests under the blue lamp.".into(),
            created_at: 1_700_000_000_000,
            provenance: Provenance::UtilityModel,
            source_state: ozone_memory::RetrievalSourceState::Current,
            is_active_memory: None,
            lifecycle: Some(ozone_memory::ArtifactLifecycleSummary {
                storage_tier: ozone_memory::StorageTier::Minimal,
                age_messages: 1_024,
                age_hours: 169,
                is_stale: true,
                adjusted_provenance_score: 0.61,
            }),
            score: ozone_memory::HybridScoreInput {
                mode: ozone_memory::RetrievalSearchMode::Hybrid,
                hybrid_alpha: 0.5,
                bm25_score: Some(-1.1),
                text_score: 0.8,
                vector_similarity: Some(0.9),
                importance_score: 0.45,
                recency_score: 0.7,
                provenance: Provenance::UtilityModel,
                stale_penalty: 1.0,
            }
            .score(
                &ozone_memory::RetrievalWeights::default(),
                &ozone_memory::ProvenanceWeights::default(),
            ),
        }],
    };

    let report = format_search_report("Session search", None, &result, true);
    assert!(report.contains("mode            hybrid"));
    assert!(report.contains("status          filtered 1 stale embedding"));
    assert!(report.contains("text/vector     text"));
    assert!(report.contains("ranking         provenance"));
    assert!(report.contains("lifecycle       tier minimal · ⚠ stale · prov 0.61"));
    assert!(report.contains("session name    Observatory"));
}

#[test]
fn ozone_plus_runtime_restores_persisted_draft_on_bootstrap() {
    let sandbox = TestSandbox::new("runtime-draft");
    let repo = sandbox.repo();
    let session = repo
        .create_session(CreateSessionRequest::new("Draft Session"))
        .unwrap();
    let context = ozone_tui::SessionContext::new(session.session_id.clone(), session.name.clone());

    let mut runtime = OzonePlusRuntime::open(repo.clone(), session.session_id.clone()).unwrap();
    runtime
        .persist_draft(&context, Some("restored from app runtime"))
        .unwrap();
    runtime.release_lock().unwrap();

    let mut reopened = OzonePlusRuntime::open(repo, session.session_id.clone()).unwrap();
    let bootstrap = reopened.bootstrap(&context).unwrap();
    reopened.release_lock().unwrap();

    assert_eq!(
        bootstrap.draft,
        Some(ozone_tui::DraftState {
            text: "restored from app runtime".to_owned(),
            cursor: "restored from app runtime".chars().count(),
            dirty: false,
            persisted: Some(ozone_tui::DraftCheckpoint::new(
                "restored from app runtime",
                "restored from app runtime".chars().count(),
            )),
        })
    );
}

#[test]
fn import_and_export_commands_use_xdg_paths() {
    let _env_guard = ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let sandbox = TestSandbox::new("import-export-smoke");
    fs::create_dir_all(sandbox.xdg_data_home()).unwrap();
    let _xdg_data_home = ScopedEnvVar::set("XDG_DATA_HOME", sandbox.xdg_data_home());
    let _home = ScopedEnvVar::set("HOME", sandbox.root.join("home"));

    let card_path = sandbox.root.join("fixtures").join("aster.json");
    fs::create_dir_all(card_path.parent().unwrap()).unwrap();
    fs::write(
        &card_path,
        r#"{
            "name": "Aster",
            "description": "A patient observatory guide.",
            "first_mes": "Welcome back to the observatory."
        }"#,
    )
    .unwrap();

    cli::import::import_character_card(ImportCharacterCardArgs {
        input: card_path.clone(),
        session_name: Some("Smoke Session".to_owned()),
        tags: vec!["smoke".to_owned()],
    })
    .unwrap();

    let repo = open_repository().unwrap();
    let sessions = repo.list_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    let session = sessions[0].clone();
    assert_eq!(session.name, "Smoke Session");
    assert_eq!(session.character_name.as_deref(), Some("Aster"));

    let transcript_path = sandbox.root.join("exports").join("transcript.txt");
    cli::export::export_transcript(cli::args::ExportTranscriptArgs {
        session_id: session.session_id.to_string(),
        branch_id: None,
        format: TranscriptExportFormat::Text,
        output: transcript_path.clone(),
    })
    .unwrap();

    let session_path = sandbox.root.join("exports").join("session.json");
    cli::export::export_session(cli::args::ExportSessionArgs {
        session_id: session.session_id.to_string(),
        format: SessionExportFormat::Json,
        output: session_path.clone(),
    })
    .unwrap();

    let transcript_text = fs::read_to_string(&transcript_path).unwrap();
    assert!(transcript_text.contains("ozone+ transcript export"));
    assert!(transcript_text.contains("Welcome back to the observatory."));

    let session_json = fs::read_to_string(&session_path).unwrap();
    assert!(session_json.contains("\"format\": \"ozone-plus.session-export.v1\""));
    assert!(session_json.contains("\"name\": \"Smoke Session\""));
}

#[test]
fn handoff_candidates_create_launcher_session_when_empty() {
    let sandbox = TestSandbox::new("handoff-empty");
    let repo = sandbox.repo();

    let candidates = cli::open::handoff_candidates(&repo, HandoffArgs::default()).unwrap();

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].name, "Launcher Session");
    let sessions = repo.list_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].name, "Launcher Session");
}

#[test]
fn handoff_candidates_reuse_existing_sessions() {
    let sandbox = TestSandbox::new("handoff-existing");
    let repo = sandbox.repo();
    let existing = repo
        .create_session(CreateSessionRequest::new("Existing Session"))
        .unwrap();

    let candidates = cli::open::handoff_candidates(&repo, HandoffArgs::default()).unwrap();

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].session_id, existing.session_id);
    assert_eq!(candidates[0].name, "Existing Session");
}

#[test]
fn swipe_activation_does_not_retip_unrelated_active_branch() {
    let sandbox = TestSandbox::new("swipe-branch-activation");
    let repo = sandbox.repo();
    let session = repo
        .create_session(CreateSessionRequest::new("Swipe Branch Session"))
        .unwrap();

    let user_record = repo
        .insert_message(
            &session.session_id,
            CreateMessageRequest::user("hello from user".to_owned()),
        )
        .unwrap();
    let user_message_id = MessageId::parse(user_record.message_id.clone()).unwrap();

    let main_branch_id = generate_branch_id().unwrap();
    let mut main_branch = ozone_core::engine::ConversationBranch::new(
        main_branch_id.clone(),
        session.session_id.clone(),
        "main",
        user_message_id.clone(),
        user_record.created_at,
    );
    main_branch.state = BranchState::Active;
    repo.create_branch(ozone_core::engine::CreateBranchCommand {
        branch: main_branch,
        forked_from: user_message_id.clone(),
    })
    .unwrap();

    let mut assistant_message = ozone_core::engine::ConversationMessage::new(
        session.session_id.clone(),
        generate_message_id().unwrap(),
        "assistant",
        "assistant reply".to_owned(),
        now_timestamp_ms(),
    );
    assistant_message.parent_id = Some(user_message_id.clone());
    let assistant_message = repo
        .commit_message(ozone_core::engine::CommitMessageCommand {
            branch_id: main_branch_id.clone(),
            message: assistant_message,
        })
        .unwrap();

    let fork_branch_id = generate_branch_id().unwrap();
    let mut fork_branch = ozone_core::engine::ConversationBranch::new(
        fork_branch_id.clone(),
        session.session_id.clone(),
        "deep-fork",
        assistant_message.message_id.clone(),
        now_timestamp_ms(),
    );
    fork_branch.state = BranchState::Active;
    repo.create_branch(ozone_core::engine::CreateBranchCommand {
        branch: fork_branch,
        forked_from: assistant_message.message_id.clone(),
    })
    .unwrap();

    let mut store = RepoConversationStore::new(repo.clone());
    let (group, candidate) = store
        .create_swipe_candidate(ManualSwipeCandidateRequest {
            session_id: session.session_id.clone(),
            parent_message_id: user_message_id.clone(),
            parent_context_message_id: None,
            swipe_group_id: Some(generate_swipe_group_id().unwrap()),
            ordinal: Some(0),
            author_kind: "assistant".to_owned(),
            author_name: None,
            content: "alternate reply".to_owned(),
            state: SwipeCandidateState::Active,
        })
        .unwrap();

    let activated = store
        .activate_swipe_candidate(ozone_engine::ActivateSwipeRequest {
            session_id: session.session_id.clone(),
            command: ozone_core::engine::ActivateSwipeCommand {
                swipe_group_id: group.swipe_group_id.clone(),
                ordinal: candidate.ordinal,
            },
        })
        .unwrap();

    assert_eq!(activated.active_ordinal, candidate.ordinal);

    let active_branch = repo
        .get_active_branch(&session.session_id)
        .unwrap()
        .unwrap();
    assert_eq!(active_branch.branch.branch_id, fork_branch_id);
    assert_eq!(
        active_branch.branch.tip_message_id,
        assistant_message.message_id
    );
}

#[test]
fn handoff_candidates_create_or_reuse_launcher_session_when_requested() {
    let sandbox = TestSandbox::new("handoff-launcher-session");
    let repo = sandbox.repo();
    repo.create_session(CreateSessionRequest::new("Existing Session"))
        .unwrap();

    let candidates = cli::open::handoff_candidates(
        &repo,
        HandoffArgs {
            launcher_session: true,
        },
    )
    .unwrap();

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].name, LAUNCHER_SESSION_NAME);

    let second = cli::open::handoff_candidates(
        &repo,
        HandoffArgs {
            launcher_session: true,
        },
    )
    .unwrap();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].session_id, candidates[0].session_id);
}

#[test]
fn memory_and_search_commands_parse() {
    let cli = Cli::try_parse_from([
        "ozone-plus",
        "memory",
        "pin",
        "session-1",
        "message-1",
        "--expires-after-turns",
        "3",
    ])
    .unwrap();
    match cli.command {
        Some(Command::Memory(MemoryArgs {
            command: MemoryCommand::Pin(args),
        })) => {
            assert_eq!(args.session_id, "session-1");
            assert_eq!(args.message_id, "message-1");
            assert_eq!(args.expires_after_turns, Some(3));
        }
        _ => panic!("unexpected cli parse result"),
    }

    let cli = Cli::try_parse_from([
        "ozone-plus",
        "search",
        "session",
        "session-2",
        "observatory key",
    ])
    .unwrap();
    match cli.command {
        Some(Command::Search(SearchArgs {
            command: SearchCommand::Session(args),
        })) => {
            assert_eq!(args.session_id, "session-2");
            assert_eq!(args.query, "observatory key");
        }
        _ => panic!("unexpected cli parse result"),
    }

    let cli = Cli::try_parse_from(["ozone-plus", "index", "rebuild"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Index(IndexArgs {
            command: IndexCommand::Rebuild
        }))
    ));

    let cli = Cli::try_parse_from(["ozone-plus", "handoff", "--launcher-session"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Handoff(HandoffArgs {
            launcher_session: true
        }))
    ));
}

#[test]
fn memory_and_search_commands_execute_against_xdg_repo() {
    let _env_guard = ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let sandbox = TestSandbox::new("memory-search-smoke");
    fs::create_dir_all(sandbox.xdg_data_home()).unwrap();
    let _xdg_data_home = ScopedEnvVar::set("XDG_DATA_HOME", sandbox.xdg_data_home());
    let _home = ScopedEnvVar::set("HOME", sandbox.root.join("home"));
    let keyword = "observatory-phase2a";

    let repo = open_repository().unwrap();
    let session = repo
        .create_session(CreateSessionRequest::new("Memory Search Session"))
        .unwrap();
    let message = repo
        .insert_message(
            &session.session_id,
            CreateMessageRequest::user(format!("The {keyword} rests under the blue lamp.")),
        )
        .unwrap();

    run_cli(Cli::try_parse_from([
        "ozone-plus",
        "memory",
        "pin",
        session.session_id.as_str(),
        &message.message_id,
    ])
    .unwrap())
    .unwrap();

    let repo = open_repository().unwrap();
    assert_eq!(
        repo.list_pinned_memories(&session.session_id)
            .unwrap()
            .len(),
        1
    );

    run_cli(Cli::try_parse_from([
        "ozone-plus",
        "search",
        "session",
        session.session_id.as_str(),
        keyword,
    ])
    .unwrap())
    .unwrap();
    run_cli(
        Cli::try_parse_from(["ozone-plus", "search", "global", keyword]).unwrap(),
    )
    .unwrap();

    assert_eq!(
        repo.search_messages(&session.session_id, keyword)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(repo.search_across_sessions(keyword).unwrap().len(), 1);
}

#[test]
fn index_rebuild_command_persists_embeddings_and_builds_vector_index() {
    let _env_guard = ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let sandbox = TestSandbox::new("index-rebuild");
    fs::create_dir_all(sandbox.xdg_data_home()).unwrap();
    let _xdg_data_home = ScopedEnvVar::set("XDG_DATA_HOME", sandbox.xdg_data_home());
    let _xdg_config_home = ScopedEnvVar::set("XDG_CONFIG_HOME", sandbox.xdg_config_home());
    let _home = ScopedEnvVar::set("HOME", sandbox.root.join("home"));
    write_global_config(
        &sandbox,
        r#"
[memory.embedding]
provider = "mock"
model = "mock/stable"
expected_dimensions = 8
batch_size = 2
mock_seed = 11
"#,
    );

    let repo = open_repository().unwrap();
    let session = repo
        .create_session(CreateSessionRequest::new("Index Session"))
        .unwrap();
    repo.insert_message(
        &session.session_id,
        CreateMessageRequest::user("Remember the brass lantern under the stairs."),
    )
    .unwrap();
    repo.create_note_memory(
        &session.session_id,
        CreateNoteMemoryRequest::new(
            "Pack the spare lens before leaving camp.",
            AuthorId::User,
            Provenance::UserAuthored,
        ),
    )
    .unwrap();

    run_cli(Cli::try_parse_from(["ozone-plus", "index", "rebuild"]).unwrap()).unwrap();
    let repo = open_repository().unwrap();
    let first_records = repo.list_embedding_artifacts(None).unwrap();
    assert_eq!(first_records.len(), 2);
    let first_ids = first_records
        .iter()
        .map(|record| record.artifact_id.clone())
        .collect::<Vec<_>>();

    let manager = VectorIndexManager::new(repo.paths().data_dir().join("vector-index"));
    let first_state = manager.open().unwrap().unwrap();
    assert_eq!(first_state.vector_count, 2);
    assert_eq!(first_state.metadata.model, "mock/stable");
    assert_eq!(first_state.metadata.dimensions, 8);

    run_cli(Cli::try_parse_from(["ozone-plus", "index", "rebuild"]).unwrap()).unwrap();
    let repo = open_repository().unwrap();
    let second_records = repo.list_embedding_artifacts(None).unwrap();
    let second_ids = second_records
        .iter()
        .map(|record| record.artifact_id.clone())
        .collect::<Vec<_>>();
    assert_eq!(first_ids, second_ids);
    let second_state = manager.open().unwrap().unwrap();
    assert_eq!(first_state.metadata, second_state.metadata);
}

#[test]
fn index_rebuild_fails_cleanly_when_provider_is_disabled() {
    let _env_guard = ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let sandbox = TestSandbox::new("index-rebuild-disabled");
    fs::create_dir_all(sandbox.xdg_data_home()).unwrap();
    let _xdg_data_home = ScopedEnvVar::set("XDG_DATA_HOME", sandbox.xdg_data_home());
    let _xdg_config_home = ScopedEnvVar::set("XDG_CONFIG_HOME", sandbox.xdg_config_home());
    let _home = ScopedEnvVar::set("HOME", sandbox.root.join("home"));

    let repo = open_repository().unwrap();
    let session = repo
        .create_session(CreateSessionRequest::new("Disabled Index Session"))
        .unwrap();
    repo.insert_message(
        &session.session_id,
        CreateMessageRequest::user("This should remain FTS-only."),
    )
    .unwrap();

    let err = run_cli(Cli::try_parse_from(["ozone-plus", "index", "rebuild"]).unwrap())
        .unwrap_err();
    assert!(
        err.contains("enabled embedding provider"),
        "unexpected error: {err}"
    );

    let repo = open_repository().unwrap();
    assert!(repo.list_embedding_artifacts(None).unwrap().is_empty());
    let manager =
        VectorIndexManager::new(repo.paths().data_dir().join("vector-index"));
    assert!(manager.load_metadata().unwrap().is_none());
}

#[test]
fn save_prefs_preserves_unknown_json_fields() {
    const UNKNOWN_KEY: &str = "custom_field";
    const UNKNOWN_VALUE: &str = "keep-me";
    const UPDATED_THEME: &str = "high-contrast";
    const UPDATED_TIMESTAMP_STYLE: &str = "relative";
    const UPDATED_MESSAGE_DENSITY: &str = "comfortable";

    let _env_guard = ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let sandbox = TestSandbox::new("prefs-save");
    fs::create_dir_all(sandbox.xdg_data_home()).unwrap();
    let _xdg_data_home = ScopedEnvVar::set("XDG_DATA_HOME", sandbox.xdg_data_home());
    let _home = ScopedEnvVar::set("HOME", sandbox.root.join("home"));

    let prefs_path = ozone_core::paths::preferences_path().expect("preferences path");
    fs::create_dir_all(prefs_path.parent().unwrap()).unwrap();
    fs::write(
        &prefs_path,
        format!(
            "{{\n  \"theme_preset\": \"dark-mint\",\n  \"{UNKNOWN_KEY}\": \"{UNKNOWN_VALUE}\"\n}}\n"
        ),
    )
    .unwrap();

    let prefs = cli::prefs::OzonePlusPrefs {
        theme_preset: UPDATED_THEME.to_string(),
        side_by_side_monitor: true,
        show_inspector: true,
        timestamp_style: UPDATED_TIMESTAMP_STYLE.to_string(),
        message_density: UPDATED_MESSAGE_DENSITY.to_string(),
    };
    cli::prefs::save_prefs_sync(&prefs).unwrap();

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&prefs_path).unwrap()).unwrap();
    assert_eq!(written[UNKNOWN_KEY], serde_json::json!(UNKNOWN_VALUE));
    assert_eq!(written["theme_preset"], serde_json::json!(UPDATED_THEME));
    assert_eq!(written["side_by_side_monitor"], serde_json::json!(true));
    assert_eq!(written["show_inspector"], serde_json::json!(true));
    assert_eq!(
        written["timestamp_style"],
        serde_json::json!(UPDATED_TIMESTAMP_STYLE)
    );
    assert_eq!(
        written["message_density"],
        serde_json::json!(UPDATED_MESSAGE_DENSITY)
    );
}

#[test]
fn load_theme_preset_reads_saved_preferences() {
    const SAVED_THEME: &str = "high-contrast";

    let _env_guard = ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let sandbox = TestSandbox::new("prefs-theme");
    fs::create_dir_all(sandbox.xdg_data_home()).unwrap();
    let _xdg_data_home = ScopedEnvVar::set("XDG_DATA_HOME", sandbox.xdg_data_home());
    let _home = ScopedEnvVar::set("HOME", sandbox.root.join("home"));

    let prefs_path = ozone_core::paths::preferences_path().expect("preferences path");
    fs::create_dir_all(prefs_path.parent().unwrap()).unwrap();
    fs::write(
        &prefs_path,
        format!("{{\n  \"theme_preset\": \"{SAVED_THEME}\"\n}}\n"),
    )
    .unwrap();

    assert_eq!(cli::prefs::load_theme_preset().unwrap(), ozone_tui::ThemePreset::HighContrast);
}

#[test]
fn docs_command_describes_current_scope() {
    const STALE_SCOPE_TEXT: &str = "These docs describe the future ozone+ scope.";
    const EXPECTED_SCOPE_TEXT: &str = "These docs describe the current shipped ozone+ scope.";

    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_ozone-plus"))
        .arg("docs")
        .output()
        .unwrap();
    assert!(output.status.success(), "docs command failed: {output:?}");

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        !stdout.contains(STALE_SCOPE_TEXT),
        "stale docs scope text was still present: {stdout}"
    );
    assert!(
        stdout.contains(EXPECTED_SCOPE_TEXT),
        "docs output did not contain the current-scope guidance: {stdout}"
    );
}