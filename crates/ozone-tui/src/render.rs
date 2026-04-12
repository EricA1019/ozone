use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::{
    app::{FocusTarget, InspectorFocus, RuntimePhase, ScreenState, ShellState},
    input::InputMode,
    layout::{LayoutMode, LayoutModel, PaneId, PaneLayout},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationEntryModel {
    pub author: String,
    pub content: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationPaneModel {
    pub title: String,
    pub subtitle: String,
    pub entries: Vec<ConversationEntryModel>,
    pub empty_state: String,
    pub hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerPaneModel {
    pub title: String,
    pub mode: String,
    pub lines: Vec<String>,
    pub placeholder: String,
    pub cursor: usize,
    pub dirty: bool,
    pub hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusPaneModel {
    pub title: String,
    pub summary: String,
    pub notifications: Vec<String>,
    pub hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorPaneModel {
    pub title: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellIndicators {
    pub screen: String,
    pub input_mode: String,
    pub focus: String,
    pub selection: String,
    pub branch: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayRenderModel {
    pub title: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderModel {
    pub title: String,
    pub subtitle: String,
    pub conversation: ConversationPaneModel,
    pub composer: ComposerPaneModel,
    pub status: StatusPaneModel,
    pub inspector: Option<InspectorPaneModel>,
    pub indicators: ShellIndicators,
    pub overlay: Option<OverlayRenderModel>,
}

pub fn build_render_model(state: &ShellState, layout: &LayoutModel) -> RenderModel {
    let indicators = ShellIndicators {
        screen: screen_label(state.screen).into(),
        input_mode: input_mode_label(state.input_mode).into(),
        focus: focus_label(state.focus).into(),
        selection: selection_label(state),
        branch: branch_label(state),
    };

    let title = format!("ozone+ — {}", state.session.context.title);
    let shell_label = match layout.mode {
        LayoutMode::Compact => "compact shell",
        LayoutMode::Wide => "wide shell",
    };
    let subtitle = format!(
        "{} · {} · {}",
        indicators.input_mode, indicators.focus, shell_label
    );

    let conversation = ConversationPaneModel {
        title: "Conversation".into(),
        subtitle: format!("{} · {}", indicators.selection, indicators.branch),
        entries: state
            .session
            .transcript
            .iter()
            .enumerate()
            .map(|(index, item)| ConversationEntryModel {
                author: item.author.clone(),
                content: item.content.clone(),
                selected: state.session.selected_message == Some(index),
            })
            .collect(),
        empty_state: "Transcript will appear here once ozone+ opens a live session.".into(),
        hint: "j/k move · Tab composer · i insert · Ctrl+I inspector · ? help".into(),
    };

    let composer = ComposerPaneModel {
        title: "Composer".into(),
        mode: indicators.input_mode.clone(),
        lines: if state.draft.text.is_empty() {
            Vec::new()
        } else {
            state.draft.text.lines().map(str::to_owned).collect()
        },
        placeholder: "Write a reply or slash-style command…".into(),
        cursor: state.draft.cursor,
        dirty: state.draft.dirty,
        hint: composer_hint(state.input_mode).into(),
    };

    let status = StatusPaneModel {
        title: "Status".into(),
        summary: state
            .status_line
            .clone()
            .unwrap_or_else(|| runtime_label(&state.session.runtime)),
        notifications: vec![
            format!("screen {} · focus {}", indicators.screen, indicators.focus),
            format!("{} · {}", indicators.selection, indicators.branch),
            runtime_label(&state.session.runtime),
            inspector_visibility_label(layout, state),
        ],
        hint: "? help · q quit".into(),
    };

    let inspector = layout.inspector.map(|_| InspectorPaneModel {
        title: "Inspector".into(),
        lines: vec![
            format!("session {}", state.session.context.session_id),
            indicators.branch.clone(),
            indicators.selection.clone(),
            format!("focus {}", inspector_focus_label(state.inspector.focus)),
            runtime_label(&state.session.runtime),
            "placeholder pane for metadata, tools, and branch context".into(),
            "later phases can replace this with richer ozone+ detail".into(),
        ],
    });

    RenderModel {
        title,
        subtitle,
        conversation,
        composer,
        status,
        inspector,
        indicators,
        overlay: overlay_model(state.screen, state.input_mode),
    }
}

pub fn render_shell(frame: &mut Frame, layout: &LayoutModel, model: &RenderModel) {
    render_conversation(
        frame,
        &layout.conversation,
        model,
        layout.focused == PaneId::Conversation,
    );
    render_composer(
        frame,
        &layout.composer,
        &model.composer,
        layout.focused == PaneId::Composer,
    );
    render_status(
        frame,
        &layout.status,
        &model.status,
        layout.focused == PaneId::Status,
    );

    if let (Some(pane), Some(model)) = (layout.inspector.as_ref(), model.inspector.as_ref()) {
        render_inspector(frame, pane, model, layout.focused == PaneId::Inspector);
    }

    if let (Some(pane), Some(model)) = (layout.overlay.as_ref(), model.overlay.as_ref()) {
        render_overlay(frame, pane, model);
    }
}

fn render_conversation(frame: &mut Frame, pane: &PaneLayout, model: &RenderModel, focused: bool) {
    let mut lines = vec![
        Line::from(Span::styled(
            model.title.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("{} · {}", model.subtitle, model.conversation.subtitle),
            muted_style(),
        )),
        Line::default(),
    ];

    if model.conversation.entries.is_empty() {
        lines.push(Line::from(Span::styled(
            model.conversation.empty_state.clone(),
            muted_style(),
        )));
    } else {
        for entry in &model.conversation.entries {
            let marker = if entry.selected { "> " } else { "  " };
            let marker_style = if entry.selected {
                highlight_style()
            } else {
                muted_style()
            };
            let author_style = if entry.selected {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::Gray)
            };

            lines.push(Line::from(vec![
                Span::styled(marker, marker_style),
                Span::styled(format!("{:<10}", entry.author), author_style),
                Span::raw(" "),
                Span::raw(entry.content.clone()),
            ]));
        }
    }

    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        model.conversation.hint.clone(),
        muted_style(),
    )));

    frame.render_widget(
        Paragraph::new(lines)
            .block(pane_block(&model.conversation.title, focused))
            .wrap(Wrap { trim: false }),
        pane.area,
    );
}

fn render_composer(frame: &mut Frame, pane: &PaneLayout, model: &ComposerPaneModel, focused: bool) {
    let mut lines: Vec<Line> = if model.lines.is_empty() {
        vec![Line::from(Span::styled(
            model.placeholder.clone(),
            muted_style(),
        ))]
    } else {
        model.lines.iter().cloned().map(Line::from).collect()
    };

    let draft_state = if model.dirty {
        "draft dirty"
    } else {
        "draft clean"
    };
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled("mode ", muted_style()),
        Span::styled(model.mode.clone(), warning_style()),
        Span::styled(
            format!(" · cursor {} · {}", model.cursor, draft_state),
            muted_style(),
        ),
    ]));
    lines.push(Line::from(Span::styled(model.hint.clone(), muted_style())));

    frame.render_widget(
        Paragraph::new(lines)
            .block(pane_block(&model.title, focused))
            .wrap(Wrap { trim: false }),
        pane.area,
    );
}

fn render_status(frame: &mut Frame, pane: &PaneLayout, model: &StatusPaneModel, focused: bool) {
    let mut lines = vec![Line::from(Span::styled(
        model.summary.clone(),
        Style::default().add_modifier(Modifier::BOLD),
    ))];
    lines.extend(
        model
            .notifications
            .iter()
            .cloned()
            .map(|line| Line::from(Span::styled(line, muted_style()))),
    );
    lines.push(Line::from(Span::styled(model.hint.clone(), muted_style())));

    frame.render_widget(
        Paragraph::new(lines)
            .block(pane_block(&model.title, focused))
            .wrap(Wrap { trim: false }),
        pane.area,
    );
}

fn render_inspector(
    frame: &mut Frame,
    pane: &PaneLayout,
    model: &InspectorPaneModel,
    focused: bool,
) {
    let lines: Vec<Line> = model
        .lines
        .iter()
        .cloned()
        .map(|line| Line::from(Span::styled(line, muted_style())))
        .collect();

    frame.render_widget(
        Paragraph::new(lines)
            .block(pane_block(&model.title, focused))
            .wrap(Wrap { trim: false }),
        pane.area,
    );
}

fn render_overlay(frame: &mut Frame, pane: &PaneLayout, model: &OverlayRenderModel) {
    let lines: Vec<Line> = model.lines.iter().cloned().map(Line::from).collect();

    frame.render_widget(Clear, pane.area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(overlay_block(&model.title))
            .wrap(Wrap { trim: false }),
        pane.area,
    );
}

fn pane_block(title: &str, focused: bool) -> Block<'static> {
    Block::default()
        .title(Span::styled(
            format!(" {} ", title),
            if focused {
                highlight_style()
            } else {
                Style::default().fg(Color::Gray)
            },
        ))
        .borders(Borders::ALL)
        .border_style(if focused {
            highlight_style()
        } else {
            Style::default().fg(Color::DarkGray)
        })
}

fn overlay_block(title: &str) -> Block<'static> {
    Block::default()
        .title(Span::styled(
            format!(" {} ", title),
            warning_style().add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(warning_style())
}

fn muted_style() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn highlight_style() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

fn warning_style() -> Style {
    Style::default().fg(Color::Yellow)
}

fn input_mode_label(input_mode: InputMode) -> &'static str {
    match input_mode {
        InputMode::Normal => "NORMAL",
        InputMode::Insert => "INSERT",
        InputMode::Command => "COMMAND",
    }
}

fn screen_label(screen: ScreenState) -> &'static str {
    match screen {
        ScreenState::Conversation => "conversation",
        ScreenState::Help => "help",
        ScreenState::Quit => "quit",
    }
}

fn focus_label(focus: FocusTarget) -> &'static str {
    match focus {
        FocusTarget::Transcript => "conversation",
        FocusTarget::Draft => "composer",
        FocusTarget::Status => "status",
    }
}

fn selection_label(state: &ShellState) -> String {
    match (
        state.session.selected_message,
        state.session.transcript.len(),
    ) {
        (_, 0) => "0 messages".into(),
        (Some(index), total) => format!("message {}/{}", index + 1, total),
        (None, total) => format!("{} messages", total),
    }
}

fn branch_label(state: &ShellState) -> String {
    state
        .session
        .selected_branch
        .and_then(|index| state.session.branches.get(index))
        .or_else(|| {
            state
                .session
                .branches
                .iter()
                .find(|branch| branch.is_active)
        })
        .map(|branch| format!("branch {}", branch.label))
        .unwrap_or_else(|| "no branches loaded".into())
}

fn runtime_label(runtime: &RuntimePhase) -> String {
    match runtime {
        RuntimePhase::Idle => "runtime idle".into(),
        RuntimePhase::Queued { .. } => "runtime queued".into(),
        RuntimePhase::Generating { request_id, .. } => {
            format!("runtime generating · {}", request_id)
        }
        RuntimePhase::Cancelling { request_id, .. } => match request_id {
            Some(request_id) => format!("runtime cancelling · {}", request_id),
            None => "runtime cancelling".into(),
        },
        RuntimePhase::Cancelled {
            request_id, reason, ..
        } => match request_id {
            Some(request_id) => format!("runtime cancelled · {} · {}", request_id, reason),
            None => format!("runtime cancelled · {}", reason),
        },
    }
}

fn inspector_visibility_label(layout: &LayoutModel, state: &ShellState) -> String {
    match (
        layout.mode,
        layout.inspector.is_some(),
        state.inspector.visible,
    ) {
        (LayoutMode::Compact, _, true) => "compact shell · inspector hidden below width".into(),
        (LayoutMode::Compact, _, false) => "compact shell · inspector closed".into(),
        (LayoutMode::Wide, true, _) => "wide shell · inspector visible".into(),
        (LayoutMode::Wide, false, true) => "wide shell · inspector unavailable".into(),
        (LayoutMode::Wide, false, false) => "wide shell · inspector closed".into(),
    }
}

fn inspector_focus_label(focus: InspectorFocus) -> &'static str {
    match focus {
        InspectorFocus::Summary => "summary",
        InspectorFocus::Branches => "branches",
        InspectorFocus::Message => "message",
    }
}

fn composer_hint(input_mode: InputMode) -> &'static str {
    match input_mode {
        InputMode::Normal => "i insert · Tab conversation · Ctrl+I inspector · ? help",
        InputMode::Insert => "Enter send · Ctrl+C cancel · Ctrl+I inspector",
        InputMode::Command => "Enter send · Ctrl+C cancel · Esc normal",
    }
}

fn overlay_model(screen: ScreenState, input_mode: InputMode) -> Option<OverlayRenderModel> {
    match screen {
        ScreenState::Conversation => None,
        ScreenState::Help => Some(OverlayRenderModel {
            title: "Help".into(),
            lines: vec![
                format!("current mode: {}", input_mode_label(input_mode)),
                "j / k move selection".into(),
                "Tab switch conversation and composer focus".into(),
                "i enter insert mode".into(),
                "Enter sends the current draft".into(),
                "Ctrl+C cancels mock generation".into(),
                "Ctrl+I toggles the inspector".into(),
                "q requests quit".into(),
            ],
        }),
        ScreenState::Quit => Some(OverlayRenderModel {
            title: "Quit".into(),
            lines: vec![
                "quit requested".into(),
                "the integration layer can now tear down the shell".into(),
                "pending runtime work stays outside this render slice".into(),
            ],
        }),
    }
}

#[cfg(test)]
mod tests {
    use ozone_core::session::SessionId;
    use ratatui::{backend::TestBackend, buffer::Buffer, layout::Rect, Terminal};

    use super::{build_render_model, render_shell};
    use crate::{
        app::{AppBootstrap, BranchItem, DraftState, SessionContext, ShellState, TranscriptItem},
        input::InputMode,
        layout::build_layout_for_area,
    };

    fn seeded_state() -> ShellState {
        let session_id = SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap();
        let context = SessionContext::new(session_id, "Phase 1C");
        let mut state = ShellState::new(context);
        state.hydrate(AppBootstrap {
            transcript: vec![
                TranscriptItem::new("user", "hello skeleton"),
                TranscriptItem::new("assistant", "believable shell ready"),
            ],
            branches: vec![
                BranchItem::new("main", "main", true),
                BranchItem::new("plan", "plan", false),
            ],
            status_line: Some("mock runtime ready".into()),
            draft: Some(DraftState::with_text("draft reply")),
            screen: None,
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

        assert!(rendered.contains("Conversation"));
        assert!(rendered.contains("Composer"));
        assert!(rendered.contains("Status"));
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
        assert!(rendered.contains("INSERT"));
        assert!(rendered.contains("123e4567"));
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
            .draw(|frame| render_shell(frame, layout, model))
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
}
