    use super::*;
    use ozone_core::engine::ConversationMessage;
    use std::time::Instant;
    use std::sync::mpsc::{self, Receiver};
    use ozone_engine::ThinkingBlockDecoder;
    use ozone_inference::MemoryConfig;
    use ozone_core::{
        engine::{
            BranchState, CommitMessageCommand, ConversationBranch, CreateBranchCommand,
            MessageId,
        },
        session::SessionId,
    };
    use ozone_persist::{
        AuthorId, CreateMessageRequest, CreateSessionRequest, MemoryArtifactId,
        PersistencePaths, PinnedMemoryContent, PinnedMemoryRecord, PinnedMemoryView,
        Provenance,
    };
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicUsize, Ordering},
    };

    static TEST_COUNTER: AtomicUsize = AtomicUsize::new(1);

    struct TestSandbox {
        root: PathBuf,
    }

    impl TestSandbox {
        fn new(prefix: &str) -> Self {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("ozone-plus-runtime-tests")
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
    }

    impl Drop for TestSandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn seed_reroll_runtime(
        prefix: &str,
    ) -> (
        TestSandbox,
        Phase1dRuntime,
        TuiSessionContext,
        BranchId,
        MessageId,
        MessageId,
        MessageId,
        MessageId,
    ) {
        let sandbox = TestSandbox::new(prefix);
        let repo = sandbox.repo();
        let session = repo
            .create_session(CreateSessionRequest::new("Pinned Session Title"))
            .unwrap();
        let context = TuiSessionContext::new(session.session_id.clone(), session.name.clone());

        let user_one = repo
            .insert_message(
                &session.session_id,
                CreateMessageRequest::user("first user prompt".to_owned()),
            )
            .unwrap();
        let user_one_id = MessageId::parse(user_one.message_id).unwrap();

        let main_branch_id = crate::generate_branch_id().unwrap();
        let mut main_branch = ConversationBranch::new(
            main_branch_id.clone(),
            session.session_id.clone(),
            "main",
            user_one_id.clone(),
            user_one.created_at,
        );
        main_branch.state = BranchState::Active;
        repo.create_branch(CreateBranchCommand {
            branch: main_branch,
            forked_from: user_one_id.clone(),
        })
        .unwrap();

        let mut assistant_one = ConversationMessage::new(
            session.session_id.clone(),
            crate::generate_message_id().unwrap(),
            "assistant",
            "first assistant reply".to_owned(),
            crate::now_timestamp_ms(),
        );
        assistant_one.parent_id = Some(user_one_id.clone());
        let assistant_one = repo
            .commit_message(CommitMessageCommand {
                branch_id: main_branch_id.clone(),
                message: assistant_one,
            })
            .unwrap();

        let mut user_two = ConversationMessage::new(
            session.session_id.clone(),
            crate::generate_message_id().unwrap(),
            "user",
            "second user prompt".to_owned(),
            crate::now_timestamp_ms(),
        );
        user_two.parent_id = Some(assistant_one.message_id.clone());
        let user_two = repo
            .commit_message(CommitMessageCommand {
                branch_id: main_branch_id.clone(),
                message: user_two,
            })
            .unwrap();

        let mut assistant_two = ConversationMessage::new(
            session.session_id.clone(),
            crate::generate_message_id().unwrap(),
            "assistant",
            "second assistant reply".to_owned(),
            crate::now_timestamp_ms(),
        );
        assistant_two.parent_id = Some(user_two.message_id.clone());
        let assistant_two = repo
            .commit_message(CommitMessageCommand {
                branch_id: main_branch_id.clone(),
                message: assistant_two,
            })
            .unwrap();

        let runtime = Phase1dRuntime::open(repo, session.session_id.clone()).unwrap();
        (
            sandbox,
            runtime,
            context,
            main_branch_id,
            user_one_id,
            assistant_one.message_id,
            user_two.message_id,
            assistant_two.message_id,
        )
    }

    fn pending_generation(branch_id: BranchId, text: &str, completion: PendingCompletion) -> PendingGeneration {
        let (_sender, receiver) = mpsc::channel();
        PendingGeneration {
            branch_id,
            request_id: crate::generate_request_id().unwrap(),
            started_at: Instant::now(),
            partial_content: text.to_owned(),
            thinking_content: String::new(),
            thinking_decoder: ThinkingBlockDecoder::new(ThinkingDisplayMode::Hidden),
            tokens_generated: 4,
            receiver,
            cancel_tx: None,
            completion,
        }
    }

    fn pending_generation_with_receiver(
        branch_id: BranchId,
        receiver: Receiver<WorkerEvent>,
        thinking_mode: ThinkingDisplayMode,
    ) -> PendingGeneration {
        PendingGeneration {
            branch_id,
            request_id: crate::generate_request_id().unwrap(),
            started_at: Instant::now(),
            partial_content: String::new(),
            thinking_content: String::new(),
            thinking_decoder: ThinkingBlockDecoder::new(thinking_mode),
            tokens_generated: 0,
            receiver,
            cancel_tx: None,
            completion: PendingCompletion::Standard,
        }
    }

    fn pinned_memory(
        text: &str,
        ordinal: u8,
        remaining_turns: Option<u32>,
        is_active: bool,
    ) -> PinnedMemoryView {
        pinned_memory_with_turns(
            text,
            ordinal,
            remaining_turns,
            if is_active { 0 } else { 1 },
            is_active,
        )
    }

    fn pinned_memory_with_turns(
        text: &str,
        ordinal: u8,
        remaining_turns: Option<u32>,
        turns_elapsed: u64,
        is_active: bool,
    ) -> PinnedMemoryView {
        let artifact_id =
            MemoryArtifactId::parse(format!("123e4567-e89b-12d3-a456-4266141740{ordinal:02}"))
                .unwrap();
        let message_id =
            MessageId::parse(format!("223e4567-e89b-12d3-a456-4266141740{ordinal:02}")).unwrap();
        let session_id = SessionId::parse("323e4567-e89b-12d3-a456-426614174000").unwrap();

        PinnedMemoryView {
            record: PinnedMemoryRecord {
                artifact_id,
                session_id,
                content: PinnedMemoryContent {
                    text: text.to_owned(),
                    pinned_by: AuthorId::User,
                    expires_after_turns: remaining_turns.or(Some(1)),
                },
                source_message_id: Some(message_id),
                provenance: Provenance::UserAuthored,
                created_at: crate::now_timestamp_ms(),
                snapshot_version: 1,
            },
            turns_elapsed,
            remaining_turns,
            is_active,
        }
    }

    #[test]
    fn parses_memory_search_and_memories_commands() {
        assert_eq!(
            shell_commands::parse_shell_command("/session retitle"),
            Ok(ShellCommand::Session(SessionCommand::Retitle))
        );
        assert_eq!(
            shell_commands::parse_shell_command("/session reroll"),
            Ok(ShellCommand::Session(SessionCommand::Reroll))
        );
        assert_eq!(
            shell_commands::parse_shell_command("/memory list"),
            Ok(ShellCommand::Memory(MemoryCommand::List))
        );
        assert_eq!(
            shell_commands::parse_shell_command(":memories"),
            Ok(ShellCommand::Memory(MemoryCommand::List))
        );
        assert_eq!(
            shell_commands::parse_shell_command("/memory note Remember the blue lamp"),
            Ok(ShellCommand::Memory(MemoryCommand::Note(
                "Remember the blue lamp".into()
            )))
        );
        assert_eq!(
            shell_commands::parse_shell_command("/memory unpin 123e4567-e89b-12d3-a456-426614174000"),
            Ok(ShellCommand::Memory(MemoryCommand::Unpin(
                MemoryArtifactId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap()
            )))
        );
        assert_eq!(
            shell_commands::parse_shell_command("/search session observatory key"),
            Ok(ShellCommand::Search(SearchCommand::Session(
                "observatory key".into()
            )))
        );
        assert_eq!(
            shell_commands::parse_shell_command("/search global observatory"),
            Ok(ShellCommand::Search(SearchCommand::Global(
                "observatory".into()
            )))
        );
    }

    #[test]
    fn load_session_into_tui_boots_directly_into_conversation() {
        let sandbox = TestSandbox::new("open-conversation");
        let repo = sandbox.repo();
        let session = repo
            .create_session(CreateSessionRequest::new("Open Conversation"))
            .unwrap();

        let mut runtime = Phase1dRuntime::open(repo, session.session_id.clone()).unwrap();
        let loaded = runtime.load_session_into_tui(session.session_id.clone()).unwrap();

        assert_eq!(loaded.session_id, session.session_id.to_string());
        assert_eq!(
            loaded.bootstrap.screen,
            Some(ozone_tui::ScreenState::Conversation)
        );
    }

    #[test]
    fn toggle_bookmark_updates_bookmark_state_and_status() {
        let sandbox = TestSandbox::new("toggle-bookmark");
        let repo = sandbox.repo();
        let session = repo
            .create_session(CreateSessionRequest::new("Bookmark Session"))
            .unwrap();
        let context = TuiSessionContext::new(session.session_id.clone(), session.name.clone());
        let message = repo
            .insert_message(
                &session.session_id,
                CreateMessageRequest::user("bookmark me".to_owned()),
            )
            .unwrap();

        let mut runtime = Phase1dRuntime::open(repo, session.session_id.clone()).unwrap();
        let added = runtime
            .toggle_bookmark(&context, &message.message_id)
            .unwrap()
            .expect("bookmark add refresh");
        assert_eq!(
            added.status_line.as_deref(),
            Some("Bookmark added to selected message")
        );
        assert_eq!(
            runtime.repo.list_bookmarks(&context.session_id).unwrap().len(),
            1
        );

        let removed = runtime
            .toggle_bookmark(&context, &message.message_id)
            .unwrap()
            .expect("bookmark remove refresh");
        assert_eq!(
            removed.status_line.as_deref(),
            Some("Bookmark removed from selected message")
        );
        assert!(runtime
            .repo
            .list_bookmarks(&context.session_id)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn cancel_generation_strips_hidden_thinking_blocks_from_partial_message() {
        let (_sandbox, mut runtime, context, main_branch_id, ..) =
            seed_reroll_runtime("cancel-hidden-thinking");
        let (sender, receiver) = mpsc::channel();
        runtime.pending_generation = Some(pending_generation_with_receiver(
            main_branch_id,
            receiver,
            ThinkingDisplayMode::Hidden,
        ));

        sender
            .send(WorkerEvent::Token(
                "<think>internal reasoning</think>visible reply".to_owned(),
            ))
            .unwrap();
        drop(sender);

        let cancellation = runtime.cancel_generation(&context).unwrap().unwrap();
        let partial = cancellation
            .partial_assistant_message
            .expect("partial assistant message should be preserved");

        assert_eq!(partial.author_kind, "assistant");
        assert_eq!(partial.content, "visible reply");
    }

    #[test]
    fn poll_generation_streams_tokens_and_sets_streaming_state() {
        let (_sandbox, mut runtime, context, main_branch_id, ..) =
            seed_reroll_runtime("poll-gen-stream");
        let (sender, receiver) = mpsc::channel();
        runtime.pending_generation = Some(pending_generation_with_receiver(
            main_branch_id,
            receiver,
            ThinkingDisplayMode::Hidden,
        ));

        // Emit a token into the worker event channel and poll the runtime.
        sender.send(WorkerEvent::Token("hello".to_owned())).unwrap();

        let poll_opt = runtime.poll_generation(&context).unwrap();
        let poll = poll_opt.expect("expected a generation poll");
        match poll {
            ozone_tui::GenerationPoll::Pending { partial: Some(progress) } => {
                assert!(progress.content.contains("hello"));
            }
            other => panic!("unexpected poll variant: {:?}", other),
        }

        // runtime should re-install the pending generation after a progress update
        assert!(runtime.pending_generation.is_some());
    }

    #[test]
    fn mark_generation_failure_sets_failed_state_midstream() {
        let (_sandbox, mut runtime, _context, main_branch_id, ..) =
            seed_reroll_runtime("mark-failure");

        // Create a pending generation that already has partial content (mid-stream)
        let pending = pending_generation(
            main_branch_id,
            "partial text",
            PendingCompletion::Standard,
        );

        let failure = runtime
            .mark_generation_failure(pending, "boom".to_string())
            .unwrap();

        assert!(failure.message.contains("boom"));
        assert_eq!(failure.reason, "boom");
        // mark_generation_failure does not re-install pending_generation
        assert!(runtime.pending_generation.is_none());
    }

    #[test]
    fn run_command_session_rename_updates_session_metadata() {
        let sandbox = TestSandbox::new("command-session-rename");
        let repo = sandbox.repo();
        let session = repo
            .create_session(CreateSessionRequest::new("Rename Me"))
            .unwrap();
        let context = TuiSessionContext::new(session.session_id.clone(), session.name.clone());

        let mut runtime = Phase1dRuntime::open(repo, session.session_id.clone()).unwrap();
        let refresh = runtime
            .run_command(&context, "/session rename Renamed Session")
            .unwrap()
            .expect("command refresh");

        assert_eq!(
            refresh.status_line.as_deref(),
            Some("Session renamed to Renamed Session")
        );
        let stored = runtime
            .repo
            .get_session(&context.session_id)
            .unwrap()
            .expect("stored session");
        assert_eq!(stored.name, "Renamed Session");
    }

    #[test]
    fn recall_browser_includes_lifecycle_labels_for_memories_and_hits() {
        let memory = MemoryConfig::default();
        let result = ozone_memory::RetrievalResultSet {
            query: "observatory".into(),
            status: ozone_memory::RetrievalStatus {
                mode: ozone_memory::RetrievalSearchMode::Hybrid,
                reason: None,
                filtered_stale_embeddings: 0,
                downranked_embeddings: 0,
            },
            hits: vec![ozone_memory::RetrievalHit {
                session: ozone_memory::SearchSessionMetadata {
                    session_id: SessionId::parse("423e4567-e89b-12d3-a456-426614174000").unwrap(),
                    session_name: "Observatory".into(),
                    character_name: None,
                    tags: vec!["phase2b".into()],
                },
                hit_kind: ozone_memory::RetrievalHitKind::Message,
                artifact_id: None,
                message_id: Some(MessageId::parse("523e4567-e89b-12d3-a456-426614174000").unwrap()),
                source_message_id: None,
                author_kind: Some("assistant".into()),
                text: "The key rests under the lamp.".into(),
                created_at: crate::now_timestamp_ms(),
                provenance: Provenance::UtilityModel,
                source_state: ozone_memory::RetrievalSourceState::Current,
                is_active_memory: None,
                lifecycle: Some(ozone_memory::ArtifactLifecycleSummary {
                    storage_tier: ozone_memory::StorageTier::Minimal,
                    age_messages: 800,
                    age_hours: 2,
                    is_stale: true,
                    adjusted_provenance_score: 0.61,
                }),
                score: ozone_memory::HybridScoreInput {
                    mode: ozone_memory::RetrievalSearchMode::Hybrid,
                    hybrid_alpha: 0.5,
                    bm25_score: Some(-1.2),
                    text_score: 0.9,
                    vector_similarity: Some(0.8),
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
        let browser = tui_recall_browser_from_state(
            &[
                pinned_memory("Remember the observatory key.", 1, Some(2), true),
                pinned_memory_with_turns("Expired fallback.", 2, Some(0), 1_000, false),
            ],
            Some(&recall_helpers::recent_search_section("session", &result, false)),
            &memory,
        );

        assert_eq!(browser.title, "Recall");
        assert!(browser.summary.contains("1 active"));
        assert!(browser.summary.contains("1 expired"));
        assert!(browser.summary.contains("1 recent hit"));
        assert!(browser
            .lines
            .iter()
            .any(|line| line.contains("Remember the observatory key.")));
        assert!(browser
            .lines
            .iter()
            .any(|line| line.contains("Expired fallback.")));
        assert!(browser
            .lines
            .iter()
            .any(|line| line.contains("session search \"observatory\"")));
        assert!(browser
            .lines
            .iter()
            .any(|line| line.contains("Expired fallback.") && line.contains("tier minimal")));
        assert!(browser
            .lines
            .iter()
            .any(|line| line.contains("Expired fallback.") && line.contains("⚠ stale")));
        assert!(browser.lines.iter().any(
            |line| line.contains("The key rests under the lamp.") && line.contains("prov 0.61")
        ));
    }

    #[test]
    fn recent_search_section_displays_mode_and_score_breakdown() {
        let result = ozone_memory::RetrievalResultSet {
            query: "observatory".into(),
            status: ozone_memory::RetrievalStatus {
                mode: ozone_memory::RetrievalSearchMode::Hybrid,
                reason: None,
                filtered_stale_embeddings: 1,
                downranked_embeddings: 0,
            },
            hits: vec![ozone_memory::RetrievalHit {
                session: ozone_memory::SearchSessionMetadata {
                    session_id: SessionId::parse("423e4567-e89b-12d3-a456-426614174000").unwrap(),
                    session_name: "Observatory".into(),
                    character_name: None,
                    tags: vec!["phase2b".into()],
                },
                hit_kind: ozone_memory::RetrievalHitKind::Message,
                artifact_id: None,
                message_id: Some(MessageId::parse("523e4567-e89b-12d3-a456-426614174000").unwrap()),
                source_message_id: None,
                author_kind: Some("assistant".into()),
                text: "The key rests under the lamp.".into(),
                created_at: 1_700_000_000_100,
                provenance: Provenance::UtilityModel,
                source_state: ozone_memory::RetrievalSourceState::Current,
                is_active_memory: None,
                lifecycle: Some(ozone_memory::ArtifactLifecycleSummary {
                    storage_tier: ozone_memory::StorageTier::Minimal,
                    age_messages: 600,
                    age_hours: 12,
                    is_stale: true,
                    adjusted_provenance_score: 0.61,
                }),
                score: ozone_memory::HybridScoreInput {
                    mode: ozone_memory::RetrievalSearchMode::Hybrid,
                    hybrid_alpha: 0.5,
                    bm25_score: Some(-1.2),
                    text_score: 0.9,
                    vector_similarity: Some(0.8),
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

        let section = recall_helpers::recent_search_section("session", &result, false);
        assert!(section.summary.contains("hybrid"));
        assert!(section.summary.contains("filtered 1 stale embedding"));
        assert!(section.lines[0].contains("s="));
        assert!(section.lines[0].contains("t="));
        assert!(section.lines[0].contains("v="));
        assert!(section.lines[0].contains("tier minimal"));
        assert!(section.lines[0].contains("⚠ stale"));
        assert!(section.lines[0].contains("prov 0.61"));
        assert!(section.lines[0].contains("The key rests under the lamp."));
    }

    // ── Phase 3 cleanup-e: integration tests ────────────────────────────────

    #[test]
    fn parse_thinking_commands() {
        assert_eq!(
            shell_commands::parse_shell_command("/thinking"),
            Ok(ShellCommand::Thinking(ThinkingCommand::Status))
        );
        assert_eq!(
            shell_commands::parse_shell_command("/thinking status"),
            Ok(ShellCommand::Thinking(ThinkingCommand::Status))
        );
        assert_eq!(
            shell_commands::parse_shell_command("/thinking hidden"),
            Ok(ShellCommand::Thinking(ThinkingCommand::SetMode(
                ThinkingDisplayMode::Hidden
            )))
        );
        assert_eq!(
            shell_commands::parse_shell_command("/thinking assisted"),
            Ok(ShellCommand::Thinking(ThinkingCommand::SetMode(
                ThinkingDisplayMode::Assisted
            )))
        );
        assert_eq!(
            shell_commands::parse_shell_command("/thinking debug"),
            Ok(ShellCommand::Thinking(ThinkingCommand::SetMode(
                ThinkingDisplayMode::Debug
            )))
        );
        assert!(shell_commands::parse_shell_command("/thinking bogus").is_err());
    }

    #[test]
    fn parse_tierb_commands() {
        assert_eq!(
            shell_commands::parse_shell_command("/tierb"),
            Ok(ShellCommand::TierB(TierBCommand::Status))
        );
        assert_eq!(
            shell_commands::parse_shell_command("/tierb status"),
            Ok(ShellCommand::TierB(TierBCommand::Status))
        );
        assert_eq!(
            shell_commands::parse_shell_command("/tierb toggle"),
            Ok(ShellCommand::TierB(TierBCommand::Toggle))
        );
        assert!(shell_commands::parse_shell_command("/tierb bogus").is_err());
    }

    #[test]
    fn parse_hooks_commands() {
        assert_eq!(
            shell_commands::parse_shell_command("/hooks"),
            Ok(ShellCommand::Hooks(HooksCommand::Status))
        );
        assert_eq!(
            shell_commands::parse_shell_command("/hooks status"),
            Ok(ShellCommand::Hooks(HooksCommand::Status))
        );
        assert_eq!(
            shell_commands::parse_shell_command("/hooks list"),
            Ok(ShellCommand::Hooks(HooksCommand::List))
        );
        assert!(shell_commands::parse_shell_command("/hooks bogus").is_err());
    }

    #[test]
    fn parse_safemode_commands() {
        assert_eq!(
            shell_commands::parse_shell_command("/safemode"),
            Ok(ShellCommand::SafeMode(SafeModeCommand::Status))
        );
        assert_eq!(
            shell_commands::parse_shell_command("/safemode status"),
            Ok(ShellCommand::SafeMode(SafeModeCommand::Status))
        );
        assert_eq!(
            shell_commands::parse_shell_command("/safemode on"),
            Ok(ShellCommand::SafeMode(SafeModeCommand::On))
        );
        assert_eq!(
            shell_commands::parse_shell_command("/safemode off"),
            Ok(ShellCommand::SafeMode(SafeModeCommand::Off))
        );
        assert_eq!(
            shell_commands::parse_shell_command("/safemode toggle"),
            Ok(ShellCommand::SafeMode(SafeModeCommand::Toggle))
        );
        assert!(shell_commands::parse_shell_command("/safemode bogus").is_err());
    }

    #[test]
    fn resolve_reroll_source_uses_selected_assistant_parent_prompt() {
        let (_sandbox, runtime, context, main_branch_id, user_one_id, assistant_one_id, user_two_id, assistant_two_id) =
            seed_reroll_runtime("resolve-reroll");

        let tip = runtime
            .resolve_reroll_source(&context, assistant_two_id.as_str())
            .unwrap();
        assert_eq!(tip.active_branch_id, main_branch_id);
        assert_eq!(tip.parent_user_message.message_id, user_two_id);
        assert_eq!(
            tip.transcript_prefix
                .iter()
                .map(|message| message.message_id.clone())
                .collect::<Vec<_>>(),
            vec![user_one_id.clone(), assistant_one_id.clone(), user_two_id.clone()]
        );

        let historical = runtime
            .resolve_reroll_source(&context, assistant_one_id.as_str())
            .unwrap();
        assert_eq!(historical.parent_user_message.message_id, user_one_id);
        assert!(historical.parent_context_message_id.is_none());
        assert_eq!(historical.transcript_prefix.len(), 1);
    }

    #[test]
    fn complete_reroll_generation_on_current_branch_records_swipe_candidates() {
        let (_sandbox, mut runtime, context, main_branch_id, _user_one_id, _assistant_one_id, _user_two_id, assistant_two_id) =
            seed_reroll_runtime("reroll-current-branch");
        let source = runtime
            .resolve_reroll_source(&context, assistant_two_id.as_str())
            .unwrap();
        let pending = pending_generation(
            main_branch_id.clone(),
            "fresh current-branch reroll",
            PendingCompletion::Reroll(PendingReroll {
                source,
                branch_mode: RerollBranchMode::CurrentBranch,
            }),
        );

        let completion = runtime.complete_generation(&context, pending).unwrap();
        let refresh = completion.refresh.expect("reroll completion refresh");
        assert_eq!(refresh.status_line.as_deref(), Some("Rerolled reply on current branch"));

        let active_branch = runtime
            .repo
            .get_active_branch(&context.session_id)
            .unwrap()
            .unwrap();
        assert_eq!(active_branch.branch.branch_id, main_branch_id);
        assert_eq!(
            active_branch.branch.tip_message_id,
            MessageId::parse(completion.message.message_id.unwrap()).unwrap()
        );

        let groups = runtime.repo.list_swipe_groups(&context.session_id).unwrap();
        assert_eq!(groups.len(), 1);
        let candidates = runtime
            .repo
            .list_swipe_candidates(&context.session_id, &groups[0].swipe_group_id)
            .unwrap();
        assert_eq!(
            candidates.iter().map(|candidate| candidate.ordinal).collect::<Vec<_>>(),
            vec![0, 1]
        );
        assert_eq!(groups[0].active_ordinal, 1);
        assert_eq!(candidates[0].message_id, assistant_two_id);
    }

    #[test]
    fn complete_reroll_generation_on_historical_message_creates_new_branch() {
        let (_sandbox, mut runtime, context, main_branch_id, _user_one_id, assistant_one_id, _user_two_id, assistant_two_id) =
            seed_reroll_runtime("reroll-new-branch");
        let source = runtime
            .resolve_reroll_source(&context, assistant_one_id.as_str())
            .unwrap();
        let pending = pending_generation(
            main_branch_id.clone(),
            "fresh historical reroll",
            PendingCompletion::Reroll(PendingReroll {
                source,
                branch_mode: RerollBranchMode::NewBranch,
            }),
        );

        let completion = runtime.complete_generation(&context, pending).unwrap();
        let refresh = completion.refresh.expect("reroll completion refresh");
        assert_eq!(refresh.status_line.as_deref(), Some("Rerolled reply on new branch"));

        let active_branch = runtime
            .repo
            .get_active_branch(&context.session_id)
            .unwrap()
            .unwrap();
        assert_ne!(active_branch.branch.branch_id, main_branch_id);
        assert_eq!(
            active_branch.branch.tip_message_id,
            MessageId::parse(completion.message.message_id.unwrap()).unwrap()
        );

        let main_branch = runtime
            .repo
            .list_branches(&context.session_id)
            .unwrap()
            .into_iter()
            .find(|record| record.branch.branch_id == main_branch_id)
            .unwrap();
        assert_eq!(main_branch.branch.tip_message_id, assistant_two_id);

        let transcript = runtime
            .repo
            .get_active_branch_transcript(&context.session_id)
            .unwrap();
        assert_eq!(transcript.len(), 2);
        assert_eq!(transcript[1].author_kind, "assistant");
        assert_eq!(transcript[1].content, "fresh historical reroll");
    }

    #[test]
    fn thinking_decoder_feed_splits_think_blocks() {
        use ozone_engine::{ThinkingDisplayMode, ThinkingOutput};
        let mut dec = ozone_engine::ThinkingBlockDecoder::new(ThinkingDisplayMode::Debug);
        let outputs = dec.feed("hello <think>reasoning</think> world");
        let texts: Vec<_> = outputs
            .iter()
            .filter_map(|o| {
                if let ThinkingOutput::Content(t) = o {
                    Some(t.as_str())
                } else {
                    None
                }
            })
            .collect();
        let thinking: Vec<_> = outputs
            .iter()
            .filter_map(|o| {
                if let ThinkingOutput::Thinking(t) = o {
                    Some(t.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert!(texts.iter().any(|t| t.contains("hello")));
        assert!(texts.iter().any(|t| t.contains("world")));
        assert!(thinking.iter().any(|t| t.contains("reasoning")));
    }

    #[test]
    fn thinking_decoder_feed_empty_input() {
        use ozone_engine::ThinkingDisplayMode;
        let mut dec = ozone_engine::ThinkingBlockDecoder::new(ThinkingDisplayMode::Hidden);
        let outputs = dec.feed("");
        assert!(outputs.is_empty());
    }

    #[test]
    fn thinking_decoder_feed_partial_chunks() {
        use ozone_engine::{ThinkingDisplayMode, ThinkingOutput};
        let mut dec = ozone_engine::ThinkingBlockDecoder::new(ThinkingDisplayMode::Assisted);
        // feed in two partial chunks that together form a complete think block
        let o1 = dec.feed("start <thi");
        let o2 = dec.feed("nk>inner</think> end");
        let all: Vec<_> = o1.into_iter().chain(o2).collect();
        let has_content = all.iter().any(|o| matches!(o, ThinkingOutput::Content(_)));
        assert!(
            has_content,
            "expected some Content output across both chunks"
        );
    }

    #[test]
    fn unknown_shell_command_message_lists_new_commands() {
        let msg = shell_commands::unknown_shell_command_message();
        assert!(msg.contains("retitle"));
        assert!(msg.contains("/thinking"));
        assert!(msg.contains("/tierb"));
        assert!(msg.contains("/hooks"));
        assert!(msg.contains("/safemode"));
    }
