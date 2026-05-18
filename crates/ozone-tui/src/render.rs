mod model_types;
pub use model_types::*;

mod helpers;
pub use helpers::*;

mod labels;
pub use labels::*;

pub mod conversation;
pub use conversation::*;

mod composer;
pub use composer::*;

mod status_inspector;
pub use status_inspector::*;

mod menu_screens;
pub use menu_screens::*;

mod overlays;
pub use overlays::*;

mod coordinator;
pub use coordinator::*;


#[cfg(test)]
mod tests {
    use ozone_core::session::SessionId;
    use ratatui::{backend::TestBackend, buffer::Buffer, layout::Rect, Terminal};

    use super::{
        build_conversation_content, build_render_model, conversation_viewport, pane_block,
        render_shell,
        SessionListItemRenderModel,
    };
    use crate::{
        app::{
            AppBootstrap, BranchItem, DraftState, ScreenState, SessionContext, ShellState,
            TranscriptItem,
        },
        input::InputMode,
        layout::build_layout_for_area,
    };

    fn seeded_state() -> ShellState {
        let session_id = SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap();
        let context = SessionContext::new(session_id, "Phase 1C");
        let mut state = ShellState::new(context);
        state.enter_conversation();
        state.hydrate(AppBootstrap {
            transcript: vec![
                TranscriptItem::persisted("msg-1", "user", "hello skeleton", false),
                TranscriptItem::persisted("msg-2", "assistant", "believable shell ready", true),
            ],
            branches: vec![
                BranchItem::new("main", "main", true),
                BranchItem::new("plan", "plan", false),
            ],
            status_line: Some("mock runtime ready".into()),
            draft: Some(DraftState::with_text("draft reply")),
            screen: None,
            session_metadata: Some(crate::app::SessionMetadata {
                character_name: Some("Beatrice".into()),
                tags: vec!["story".into()],
                pinned_count: None,
                greeting: None,
                memory_metadata: None,
            }),
            session_stats: Some(crate::app::SessionStats {
                message_count: 2,
                branch_count: 2,
                bookmark_count: 1,
            }),
            context_preview: None,
            context_dry_run: None,
            recall_browser: None,
            active_launch_plan: None,
        });
        state.session.selected_message = Some(1);
        state
    }

    #[test]
    fn render_model_tracks_compact_and_wide_shell_states() {
        let mut state = seeded_state();
        state.input_mode = InputMode::Insert;

        let compact = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let compact_model = build_render_model(&state, &compact);
        assert_eq!(compact_model.indicators.input_mode, "INSERT");
        assert!(compact_model.inspector.is_none());
        assert_eq!(compact_model.status.summary, "mock runtime ready");
        assert!(compact_model.conversation.entries[1].selected);

        state.inspector.visible = true;
        let wide = build_layout_for_area(&state, Rect::new(0, 0, 120, 40));
        let wide_model = build_render_model(&state, &wide);
        assert!(wide_model
            .inspector
            .as_ref()
            .expect("wide inspector should be present")
            .lines
            .iter()
            .any(|line| line.contains("123e4567-e89b-12d3-a456-426614174000")));
        assert!(wide_model.inspector.is_some());
        assert!(wide_model
            .status
            .notifications
            .iter()
            .any(|line| line.contains("wide shell")));
    }

    #[test]
    fn test_backend_renders_compact_shell_without_inspector_title() {
        let state = seeded_state();
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);

        let rendered = render_to_string(80, 24, &layout, &model);

        assert!(
            rendered.contains("Ozone+"),
            "breadcrumb should be visible on top row"
        );
        assert!(rendered.contains("Composer"));
        assert!(
            rendered.contains("Phase 1C"),
            "session title should appear in footer"
        );
        assert!(
            rendered.contains("char Beatrice"),
            "active character should appear in the footer"
        );
        assert!(rendered.contains("mock runtime ready"));
        assert!(!rendered.contains(" Inspector "));
    }

    #[test]
    fn test_backend_renders_wide_shell_with_inspector_placeholder() {
        let mut state = seeded_state();
        state.input_mode = InputMode::Insert;
        state.inspector.visible = true;
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 120, 40));
        let model = build_render_model(&state, &layout);

        let rendered = render_to_string(120, 40, &layout, &model);

        assert!(rendered.contains("Inspector"));
        assert!(rendered.contains("branch main"));
        assert!(
            rendered.contains(" INS "),
            "Insert mode badge should appear in footer"
        );
        assert!(rendered.contains("123e4567"));
        assert!(rendered.contains("context preview unavailable"));
    }

    #[test]
    fn render_model_surfaces_recall_browser_in_status_and_inspector() {
        let mut state = seeded_state();
        state.recall_browser = Some(crate::app::RecallBrowser {
            title: "Recall".into(),
            summary: "2 active · 1 recent hit".into(),
            lines: vec![
                "active pinned 2".into(),
                "session search \"nebula\" · 1 hit".into(),
            ],
        });
        state.inspector.visible = true;

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 120, 40));
        let model = build_render_model(&state, &layout);

        assert!(model
            .status
            .notifications
            .iter()
            .any(|line| line.contains("Recall · 2 active · 1 recent hit")));
        assert!(model
            .inspector
            .as_ref()
            .expect("inspector should render")
            .lines
            .iter()
            .any(|line| line.contains("session search \"nebula\" · 1 hit")));
    }

    #[test]
    fn compact_status_surfaces_recall_notice_when_inspector_is_unavailable() {
        let mut state = seeded_state();
        state.recall_browser = Some(crate::app::RecallBrowser {
            title: "Recall".into(),
            summary: "2 active · 1 recent hit".into(),
            lines: vec!["active pinned 2".into()],
        });

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 110, 24));
        let model = build_render_model(&state, &layout);
        let rendered = render_to_string(110, 24, &layout, &model);

        assert!(rendered.contains("Recall · 2 active · 1 recent hit"));
    }

    #[test]
    fn memories_overlay_renders_recall_browser_contents() {
        let mut state = seeded_state();
        state.screen = ScreenState::MemoriesOverlay;
        state.recall_browser = Some(crate::app::RecallBrowser {
            title: "Memories".into(),
            summary: "2 active · 1 recent hit".into(),
            lines: vec![
                "active pinned 2".into(),
                "session search \"nebula\" · 1 hit".into(),
            ],
        });

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 120, 40));
        let model = build_render_model(&state, &layout);
        let rendered = render_to_string(120, 40, &layout, &model);

        assert!(model.overlay.is_some(), "memories overlay should build a render model");
        assert!(rendered.contains("Memories"));
        assert!(rendered.contains("2 active · 1 recent hit"));
        assert!(rendered.contains("active pinned 2"));
        assert!(rendered.contains("Esc/q close"));
    }

    #[test]
    fn help_overlay_closes_on_escape_q_or_question() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        for key in [
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE),
        ] {
            let mut state = seeded_state();
            state.screen = ScreenState::Help;

            let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
            state.handle_key_event_with_layout(key, &layout);

            assert_eq!(state.screen, ScreenState::Conversation);
            assert_eq!(state.input_mode, InputMode::Normal);
        }
    }

    #[test]
    fn help_overlay_renders_updated_close_and_redo_copy() {
        let mut state = seeded_state();
        state.screen = ScreenState::Help;

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);
        let rendered = render_to_string(80, 24, &layout, &model);

        assert!(rendered.contains("Esc/q/? close"));
    }

    #[test]
    fn command_palette_uses_memories_label() {
        let mut state = seeded_state();
        state.command_palette.open(crate::app::textareas::new_themed_textarea());
        state.command_palette.restore_input_text("mem", 3);

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);
        let palette = model.command_palette.expect("command palette should be visible");

        assert!(palette.entries.iter().any(|entry| entry.name == "/memories"));
        assert!(!palette.entries.iter().any(|entry| entry.name == "/memory list"));
    }

    #[test]
    fn command_palette_matches_description_queries() {
        let mut state = seeded_state();
        state.command_palette.open(crate::app::textareas::new_themed_textarea());
        state.command_palette.restore_input_text("recall", 6);

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);
        let palette = model.command_palette.expect("command palette should be visible");

        assert_eq!(palette.entries.first().map(|entry| entry.name.as_str()), Some("/memories"));
    }

    #[test]
    fn slash_popup_matches_multiword_commands_by_word_prefix() {
        let mut state = seeded_state();
        state.input_mode = InputMode::Insert;
        state.draft.text = "/ret".into();
        state.draft.cursor = 4;

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);

        assert_eq!(
            model
                .composer
                .slash_suggestions
                .first()
                .map(|suggestion| suggestion.name.as_str()),
            Some("/session retitle")
        );
    }

    #[test]
    fn streaming_partial_content_appears_as_transient_entry_while_generating() {
        use crate::state::{RuntimeProgress, RuntimeSendReceipt, TranscriptItem};

        let mut state = seeded_state();
        state.apply_send_receipt(RuntimeSendReceipt {
            request_id: "req-stream-1".into(),
            user_message: TranscriptItem::new("user", "stream test"),
            context_preview: None,
            context_dry_run: None,
            refresh: None,
            context_compression: None,
        });
        state.apply_runtime_progress(RuntimeProgress {
            request_id: "req-stream-1".into(),
            content: "streaming reply so far".into(),
        });

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);

        // The partial content should appear as an extra entry with a cursor marker
        let partial_entry = model
            .conversation
            .entries
            .last()
            .expect("at least one entry");
        assert_eq!(partial_entry.author, "assistant");
        assert!(
            partial_entry.content.contains("streaming reply so far"),
            "partial entry should contain streamed text"
        );
        assert!(
            partial_entry.content.contains('▍'),
            "partial entry should have cursor marker"
        );
    }

    #[test]
    fn generating_without_partial_content_shows_thinking_placeholder() {
        use crate::state::{RuntimeSendReceipt, TranscriptItem};

        let mut state = seeded_state();
        state.apply_send_receipt(RuntimeSendReceipt {
            request_id: "req-stream-1".into(),
            user_message: TranscriptItem::new("user", "stream test"),
            context_preview: None,
            context_dry_run: None,
            refresh: None,
            context_compression: None,
        });

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);

        let partial_entry = model
            .conversation
            .entries
            .last()
            .expect("at least one entry");
        assert_eq!(partial_entry.author, "assistant");
        assert_eq!(partial_entry.content, "Thinking…▍");
        assert!(partial_entry.is_streaming);
    }

    #[test]
    fn failed_runtime_label_includes_message() {
        use crate::state::{RuntimeFailure, RuntimeSendReceipt, TranscriptItem};

        let mut state = seeded_state();
        state.apply_send_receipt(RuntimeSendReceipt {
            request_id: "req-fail-1".into(),
            user_message: TranscriptItem::new("user", "fail test"),
            context_preview: None,
            context_dry_run: None,
            refresh: None,
            context_compression: None,
        });
        state.apply_runtime_failure(RuntimeFailure {
            request_id: "req-fail-1".into(),
            message: "context window exceeded".into(),
                prompt: "test".into(),
        reason: "test".into(),
    });

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);

        assert!(
            model
                .status
                .notifications
                .iter()
                .any(|n| n.contains("runtime failed") && n.contains("req-fail-1")),
            "status notifications should mention runtime failed"
        );
    }

    fn render_to_string(
        width: u16,
        height: u16,
        layout: &crate::layout::LayoutModel,
        model: &crate::render::RenderModel,
    ) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_shell(frame, layout, model, None, None))
            .unwrap();

        buffer_to_string(terminal.backend().buffer(), width, height)
    }

    fn buffer_to_string(buffer: &Buffer, width: u16, height: u16) -> String {
        (0..height)
            .map(|y| {
                let mut line = String::new();
                for x in 0..width {
                    line.push_str(buffer[(x, y)].symbol());
                }
                line
            })
            .collect::<Vec<_>>()
            .join(
                "
",
            )
    }

    #[test]
    fn render_shell_clears_stale_conversation_rows_between_draws() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let state = seeded_state();
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);
        terminal
            .draw(|frame| render_shell(frame, &layout, &model, None, None))
            .unwrap();

        let mut cleared_state = state.clone();
        cleared_state.session.transcript.clear();
        cleared_state.session.selected_message = None;
        let cleared_layout = build_layout_for_area(&cleared_state, Rect::new(0, 0, 80, 24));
        let cleared_model = build_render_model(&cleared_state, &cleared_layout);
        terminal
            .draw(|frame| render_shell(frame, &cleared_layout, &cleared_model, None, None))
            .unwrap();

        let rendered = buffer_to_string(terminal.backend().buffer(), 80, 24);
        assert!(rendered.contains("Start a conversation"));
        assert!(!rendered.contains("hello skeleton"));
        assert!(!rendered.contains("believable shell ready"));
        assert!(!rendered.contains("assistant ───"));
    }

    #[test]
    fn render_conversation_shows_scrollbar_for_long_transcripts() {
        let mut state = seeded_state();
        state.enter_conversation();
        state.session.transcript = (0..24)
            .map(|index| {
                crate::app::TranscriptItem::persisted(
                    format!("msg-{index}"),
                    "assistant",
                    format!("line {index}"),
                    false,
                )
            })
            .collect();
        state.session.selected_message = Some(20);

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);
        let rendered = render_to_string(80, 24, &layout, &model);

        assert!(rendered.contains("↑") || rendered.contains("↓"));
    }

    #[test]
    fn render_conversation_respects_manual_scroll_offset() {
        let mut state = seeded_state();
        state.enter_conversation();
        state.session.transcript = vec![crate::app::TranscriptItem::persisted(
            "msg-0",
            "assistant",
            "AAA0 AAA1 AAA2 AAA3 AAA4 AAA5 AAA6 AAA7 AAA8 AAA9 AAA10 AAA11 AAA12 AAA13 AAA14 AAA15",
            false,
        )];
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 40, 12));
        let model = build_render_model(&state, &layout);
        let block = pane_block(
            &model.conversation.title,
            layout.focused == crate::layout::PaneId::Conversation,
        );
        let inner = block.inner(layout.conversation.area);
        let content = build_conversation_content(
            &model.title,
            &model.conversation,
            inner.width.saturating_sub(1).max(1),
        );
        let wrapped_rows: Vec<(usize, String)> = content
            .lines
            .iter()
            .enumerate()
            .map(|line| {
                let text = line
                    .1
                    .spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<Vec<_>>()
                    .join("");
                (line.0, text)
            })
            .filter(|(_, line)| line.contains("AAA"))
            .collect();

        assert!(
            wrapped_rows.len() >= 3,
            "expected wrapped transcript rows, got {wrapped_rows:?}"
        );

        state.session.selected_message = Some(0);
        state.conversation_scroll = Some(wrapped_rows[2].0);

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 40, 12));
        let model = build_render_model(&state, &layout);
        let rendered = render_to_string(40, 12, &layout, &model);

        assert!(rendered.contains(&wrapped_rows[2].1));
        assert!(!rendered.contains(&wrapped_rows[0].1));
    }

    #[test]
    fn streaming_conversation_defaults_to_live_edge() {
        use crate::state::{RuntimeProgress, RuntimeSendReceipt, TranscriptItem};

        let mut state = seeded_state();
        state.session.transcript = (0..16)
            .map(|index| {
                TranscriptItem::persisted(
                    format!("msg-{index}"),
                    "assistant",
                    format!("history line {index}"),
                    false,
                )
            })
            .collect();
        state.session.selected_message = Some(15);

        state.apply_send_receipt(RuntimeSendReceipt {
            request_id: "req-stream-1".into(),
            user_message: TranscriptItem::new("user", "stream test"),
            context_preview: None,
            context_dry_run: None,
            refresh: None,
            context_compression: None,
        });
        state.apply_runtime_progress(RuntimeProgress {
            request_id: "req-stream-1".into(),
            content: vec!["stream chunk"; 16].join(" "),
        });

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 40, 12));
        let model = build_render_model(&state, &layout);
        let viewport = conversation_viewport(layout.conversation.area, &model.title, &model.conversation);

        assert_eq!(viewport.default_scroll_offset, viewport.max_scroll);
        assert!(viewport.max_scroll > 0);
    }

    #[test]
    fn conversation_scroll_resumes_follow_latest_when_returning_to_bottom() {
        use crate::state::{RuntimeProgress, RuntimeSendReceipt, TranscriptItem};

        let mut state = seeded_state();
        state.session.transcript = (0..16)
            .map(|index| {
                TranscriptItem::persisted(
                    format!("msg-{index}"),
                    "assistant",
                    format!("history line {index}"),
                    false,
                )
            })
            .collect();
        state.session.selected_message = Some(15);

        state.apply_send_receipt(RuntimeSendReceipt {
            request_id: "req-stream-1".into(),
            user_message: TranscriptItem::new("user", "stream test"),
            context_preview: None,
            context_dry_run: None,
            refresh: None,
            context_compression: None,
        });
        state.apply_runtime_progress(RuntimeProgress {
            request_id: "req-stream-1".into(),
            content: vec!["stream chunk"; 16].join(" "),
        });

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 40, 12));
        state.scroll_conversation(&layout, -1);
        let locked_offset = state.conversation_scroll.expect("manual scroll should lock offset");

        state.apply_runtime_progress(RuntimeProgress {
            request_id: "req-stream-1".into(),
            content: vec!["stream chunk"; 24].join(" "),
        });
        let updated_layout = build_layout_for_area(&state, Rect::new(0, 0, 40, 12));
        let updated_model = build_render_model(&state, &updated_layout);
        let updated_viewport = conversation_viewport(
            updated_layout.conversation.area,
            &updated_model.title,
            &updated_model.conversation,
        );
        assert_eq!(state.conversation_scroll, Some(locked_offset));

        state.scroll_conversation(
            &updated_layout,
            updated_viewport
                .default_scroll_offset
                .saturating_sub(locked_offset) as isize,
        );
        assert_eq!(state.conversation_scroll, None);
    }

    #[test]
    fn main_menu_screen_produces_menu_render_model() {
        let mut state = seeded_state();
        state.screen = ScreenState::MainMenu;
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 120, 40));
        let model = build_render_model(&state, &layout);

        assert!(model.main_menu.is_some());
        assert!(model.session_list.is_none());

        let menu = model.main_menu.unwrap();
        assert_eq!(menu.items.len(), 6);
        assert!(menu.items[0].selected); // first item selected by default
        assert!(!menu.items[1].selected);
        assert_eq!(menu.items[0].label, "New Chat");
        assert_eq!(menu.items[1].label, "Sessions");
        assert_eq!(menu.items[5].label, "Quit");
    }

    #[test]
    fn session_list_screen_produces_session_list_render_model() {
        let mut state = seeded_state();
        state.screen = ScreenState::SessionList;
        state.session_list.entries = vec![
            crate::state::SessionListEntry {
                session_id: "test-1".into(),
                name: "My First Chat".into(),
                character_name: Some("Aster".into()),
                message_count: 42,
                last_active: Some("2 hours ago".into()),
                folder: None,
                last_message_preview: Some(String::new()),
            },
            crate::state::SessionListEntry {
                session_id: "test-2".into(),
                name: "World Building".into(),
                character_name: None,
                message_count: 7,
                last_active: Some("yesterday".into()),
                folder: None,
                last_message_preview: Some(String::new()),
            },
        ];

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 120, 40));
        let model = build_render_model(&state, &layout);

        assert!(model.session_list.is_some());
        assert!(model.main_menu.is_none());

        let list = model.session_list.unwrap();
        // Both entries have no folder, so items = [Entry, Entry] (no headers)
        assert_eq!(list.items.len(), 2);
        let entry0 = match &list.items[0] {
            SessionListItemRenderModel::Entry(e) => e,
            _ => panic!("expected Entry"),
        };
        let entry1 = match &list.items[1] {
            SessionListItemRenderModel::Entry(e) => e,
            _ => panic!("expected Entry"),
        };
        assert!(entry0.selected);
        assert!(!entry1.selected);
        assert_eq!(entry0.name, "My First Chat");
        assert_eq!(entry0.character, "Aster");
        assert_eq!(entry1.character, "—");
        assert_eq!(list.total_count, 2);
        assert_eq!(list.visible_count, 2);
    }

    #[test]
    fn conversation_screen_has_no_menu_models() {
        let mut state = seeded_state();
        state.screen = ScreenState::Conversation;
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 120, 40));
        let model = build_render_model(&state, &layout);

        assert!(model.main_menu.is_none());
        assert!(model.session_list.is_none());
    }

    #[test]
    fn main_menu_renders_without_panic() {
        let mut state = seeded_state();
        state.screen = ScreenState::MainMenu;
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_shell(frame, &layout, &model, None, None);
            })
            .unwrap();
    }

    #[test]
    fn session_list_renders_without_panic() {
        let mut state = seeded_state();
        state.screen = ScreenState::SessionList;
        state.session_list.entries = vec![crate::state::SessionListEntry {
            session_id: "test-1".into(),
            name: "Test Session".into(),
            character_name: None,
            message_count: 10,
            last_active: None,
            folder: None,
            last_message_preview: Some(String::new()),
        }];

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_shell(frame, &layout, &model, None, None);
            })
            .unwrap();
    }

    #[test]
    fn empty_session_list_renders_without_panic() {
        let mut state = seeded_state();
        state.screen = ScreenState::SessionList;

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_shell(frame, &layout, &model, None, None);
            })
            .unwrap();
    }

    #[test]
    fn command_palette_render_shows_empty_match_copy() {
        let mut state = seeded_state();
        state.command_palette.open(crate::app::textareas::new_themed_textarea());
        state.command_palette.restore_input_text("zzzzz", 5);
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);
        let rendered = render_to_string(80, 24, &layout, &model);

        assert!(rendered.contains("No commands match the current input"));
        assert!(rendered.contains("Enter run"));
    }

    #[test]
    fn slash_popup_render_shows_accept_hint() {
        let mut state = seeded_state();
        state.enter_conversation();
        state.input_mode = InputMode::Insert;
        state.draft.text = "/he".into();
        state.draft.cursor = 3;
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);
        let rendered = render_to_string(80, 24, &layout, &model);

        assert!(rendered.contains("Tab/Enter accept"));
        assert!(rendered.contains("/help"));
    }

    #[test]
    fn slash_popup_not_shown_when_command_palette_open() {
        let mut state = seeded_state();
        state.input_mode = InputMode::Insert;
        // Put a `/` prefix in the draft so slash suggestions would normally appear.
        state.draft.text = "/he".into();
        state.draft.cursor = 3;
        // Open the command palette — this should suppress the slash popup.
        state.command_palette.open(crate::app::textareas::new_themed_textarea());

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);

        // The composer model still has suggestions (derived from draft text)…
        assert!(
            !model.composer.slash_suggestions.is_empty(),
            "slash_suggestions should be populated from the draft"
        );
        // …but the render model's command_palette is Some, so render_shell
        // skips render_slash_popup (guarded by `model.command_palette.is_none()`).
        assert!(
            model.command_palette.is_some(),
            "command palette should be present when open"
        );
    }

    #[test]
    fn message_edit_hides_slash_suggestions_and_updates_hint_copy() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut state = seeded_state();
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        state.handle_key_event_with_layout(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::CONTROL), &layout);
        state.draft.text = "/he".into();
        state.draft.cursor = 3;

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);

        assert!(model.composer.slash_suggestions.is_empty());
        assert!(model.composer.hint.contains("arrows/tab stay in editor"));
        assert!(model.composer.hint.contains("Ctrl+Y redo"));
        assert!(!model.composer.hint.contains("Ctrl+R redo"));
        assert!(model.conversation.hint.contains("Enter save"));
        assert!(model.conversation.hint.contains("Ctrl+Y redo"));
    }

    #[test]
    fn render_composer_shows_scrollbar_for_long_edit_buffers() {
        use crate::app::DraftCheckpoint;

        let mut state = seeded_state();
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        state.handle_key_event_with_layout(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('i'),
            crossterm::event::KeyModifiers::CONTROL,
        ), &layout);
        state.draft = DraftState::restore(DraftCheckpoint::new(
            (0..18)
                .map(|index| format!("line {index}"))
                .collect::<Vec<_>>()
                .join("\n"),
            0,
        ));
        state.focus = crate::state::FocusTarget::Draft;
        state.input_mode = InputMode::Insert;

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);
        let rendered = render_to_string(80, 24, &layout, &model);

        assert!(rendered.contains("↑") || rendered.contains("↓"));
    }

    #[test]
    fn slash_suggestions_clear_on_space_after_slash() {
        use crate::input::KeyAction;

        let mut state = seeded_state();
        // Clear the hydrated draft so we start fresh.
        state.draft.text.clear();
        state.draft.cursor = 0;
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        state.apply_action_with_layout(KeyAction::EnterInsert, &layout);

        // Type `/he` — suggestions should be populated.
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        state.apply_action_with_layout(KeyAction::DraftInsertChar('/'), &layout);
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        state.apply_action_with_layout(KeyAction::DraftInsertChar('h'), &layout);
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        state.apply_action_with_layout(KeyAction::DraftInsertChar('e'), &layout);

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);
        assert!(
            !model.composer.slash_suggestions.is_empty(),
            "suggestions should appear for `/he`"
        );

        // Insert a space — draft becomes `/he `, suggestions should clear.
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        state.apply_action_with_layout(KeyAction::DraftInsertChar(' '), &layout);

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let model = build_render_model(&state, &layout);
        assert!(
            model.composer.slash_suggestions.is_empty(),
            "suggestions should be empty after a space (command name complete)"
        );
    }
}

