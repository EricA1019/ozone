use super::model_types::*;
use super::helpers::*;
use super::labels::*;
use super::coordinator::format_tags;
use super::overlays::selected_message_line;
use crate::app::ShellState;
use crate::layout::{LayoutMode, LayoutModel, PaneLayout};
use crate::state::{ContextPreview, InspectorFocus};
use crate::theme;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};

pub fn render_status(frame: &mut Frame, pane: &PaneLayout, model: &StatusPaneModel, _focused: bool) {
    if pane.area.height == 0 {
        return;
    }

    let (short_badge, badge_style) = match model.mode_badge.as_deref() {
        Some("INSERT") => (
            "INS",
            Style::default()
                .fg(Color::White)
                .bg(theme::MODE_INSERT_BG)
                .add_modifier(Modifier::BOLD),
        ),
        Some("COMMAND") => (
            "CMD",
            Style::default()
                .fg(Color::White)
                .bg(theme::MODE_CMD_BG)
                .add_modifier(Modifier::BOLD),
        ),
        _ => (
            "NOR",
            Style::default()
                .fg(theme::TEAL)
                .add_modifier(Modifier::BOLD),
        ),
    };

    let title = truncate_str(&model.session_title, 30);
    let msgs = if let Some(idx) = model.selected_index {
        if model.message_count > 1 {
            format!("{}/{} msgs", idx + 1, model.message_count)
        } else {
            format!("{} msgs", model.message_count)
        }
    } else {
        format!("{} msgs", model.message_count)
    };
    let sep = || Span::styled("  │  ", theme::muted_style());

    let mut spans = vec![
        Span::styled(format!(" {short_badge} "), badge_style),
        Span::styled(" ", Style::default()),
        Span::styled(title, theme::text_style()),
    ];

    if let Some(character_label) = model.character_label.as_deref() {
        spans.push(sep());
        spans.push(Span::styled(character_label.to_string(), theme::accent_style()));
    }

    spans.push(sep());
    spans.push(Span::styled(msgs, theme::dim_style()));

    let runtime_text = status_short_runtime(&model.summary);
    if !runtime_text.is_empty() {
        spans.push(sep());
        spans.push(Span::styled(runtime_text, theme::dim_style()));
    }

    if let Some(notice) = model.compact_notice.as_deref() {
        let notice = truncate_str(notice, 42);
        let notice_width = notice.chars().count() as u16 + 5;
        let current_width: u16 = spans.iter().map(|s| s.content.chars().count() as u16).sum();
        if current_width + notice_width < pane.area.width {
            spans.push(sep());
            spans.push(Span::styled(notice, theme::accent_style()));
        }
    }

    if let Some(bar) = model.context_bar.as_deref() {
        let bar_style = model.token_budget.map(|(used, max)| {
            let pct = used as f64 / max as f64;
            if pct >= 0.9 {
                theme::error_style()
            } else if pct >= 0.75 {
                theme::warning_style()
            } else {
                theme::dim_style()
            }
        }).unwrap_or_else(theme::dim_style);
        spans.push(Span::styled("  ", theme::muted_style()));
        spans.push(Span::styled(bar.to_string(), bar_style));
    }

    if let Some(vram) = model.vram_hint.as_deref() {
        let hint_width = vram.len() as u16 + 5; // "  ···  " (5) + vram text
        let current_width: u16 = spans.iter().map(|s| s.content.chars().count() as u16).sum();
        if current_width + hint_width < pane.area.width {
            spans.push(Span::styled("  ···  ", theme::muted_style()));
            spans.push(Span::styled(vram.to_string(), theme::highlight_style()));
        }
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), pane.area);
}

pub fn render_inspector(
    frame: &mut Frame,
    pane: &PaneLayout,
    model: &InspectorPaneModel,
    focused: bool,
) {
    let mut lines: Vec<Line> = model
        .lines
        .iter()
        .cloned()
        .map(|line| Line::from(Span::styled(line, theme::dim_style())))
        .collect();

    if let Some(info) = model.model_info.as_ref() {
        let pane_width = pane.area.width.saturating_sub(4) as usize;
        let divider = format!("─ Model Info {}", "─".repeat(pane_width.saturating_sub(13)));
        lines.push(Line::from(Span::styled(divider, theme::muted_style())));

        let vram_pct = if info.estimated_vram_mb > 0 {
            // Assume 8 GiB GPU as a display reference; show raw MB otherwise
            let pct = info.estimated_vram_mb as f64 / (8 * 1024) as f64 * 100.0;
            pct.min(999.0) as u32
        } else {
            0
        };
        let vram_color = if vram_pct > 95 {
            theme::RED
        } else if vram_pct > 80 {
            theme::AMBER
        } else {
            theme::GREEN
        };

        lines.push(Line::from(vec![
            Span::styled("  VRAM:  ", Style::default().fg(theme::TEAL)),
            Span::styled(
                format!("{} MB", format_mb(info.estimated_vram_mb)),
                Style::default().fg(vram_color),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  RAM:   ", Style::default().fg(theme::TEAL)),
            Span::styled(
                format!("{} MB", format_mb(info.estimated_ram_mb)),
                theme::text_style(),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  Layers:", Style::default().fg(theme::TEAL)),
            Span::styled(
                format!(" {} GPU + {} CPU", info.gpu_layers, info.cpu_layers),
                theme::text_style(),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  Mode:  ", Style::default().fg(theme::TEAL)),
            Span::styled(info.mode_label.clone(), theme::text_style()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  Source:", Style::default().fg(theme::TEAL)),
            Span::styled(format!(" {}", info.source_label), theme::dim_style()),
        ]));
    }

    // Render context token budget info if available
    if model.context_bar.is_some() || model.token_budget.is_some() {
        let pane_width = pane.area.width.saturating_sub(4) as usize;
        let divider = format!("─ Context / Token Budget {}",
            "─".repeat(pane_width.saturating_sub(25)));
        lines.push(Line::from(Span::styled(divider, theme::muted_style())));

        if let Some(bar) = model.context_bar.as_deref() {
            lines.push(Line::from(vec![
                Span::styled("  Bar:   ", Style::default().fg(theme::TEAL)),
                Span::styled(bar.to_string(), theme::dim_style()),
            ]));
        }

        if let Some(budget) = model.token_budget.as_deref() {
            lines.push(Line::from(vec![
                Span::styled("  Usage: ", Style::default().fg(theme::TEAL)),
                Span::styled(budget.to_string(), theme::dim_style()),
            ]));
        }
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(pane_block(&model.title, focused))
            .wrap(Wrap { trim: false }),
        pane.area,
    );
}

pub fn format_mb(mb: u32) -> String {
    if mb >= 1000 {
        format!("{},{:03}", mb / 1000, mb % 1000)
    } else {
        mb.to_string()
    }
}

pub fn inspector_lines(state: &ShellState, indicators: &ShellIndicators) -> Vec<String> {
    let mut lines = vec![
        format!("session {}", state.session.context.session_id),
        format!("name {}", state.session.context.title),
        indicators.branch.clone(),
        indicators.selection.clone(),
        format!("focus {}", inspector_focus_label(state.inspector.focus)),
        state
            .session_metadata
            .as_ref()
            .map(|metadata| {
                format!(
                    "character {}",
                    metadata
                        .character_name
                        .as_deref()
                        .filter(|value| !value.is_empty())
                        .unwrap_or("—")
                )
            })
            .unwrap_or_else(|| "character —".into()),
        state
            .session_metadata
            .as_ref()
            .map(|metadata| format!("tags {}", format_tags(&metadata.tags)))
            .unwrap_or_else(|| "tags —".into()),
        state
            .session_stats
            .as_ref()
            .map(|stats| {
                let pinned_suffix = state
                    .session_metadata
                    .as_ref()
                    .and_then(|m| m.pinned_count)
                    .map(|c| format!(" · {} pinned", c))
                    .unwrap_or_default();
                format!(
                    "stats {} messages · {} branches · {} bookmarks{}",
                    stats.message_count, stats.branch_count, stats.bookmark_count, pinned_suffix
                )
            })
            .unwrap_or_else(|| "stats pending".into()),
        selected_message_line(state),
        runtime_label(&state.session.runtime),
    ];

    if let Some(browser) = state.recall_browser.as_ref() {
        lines.push(format!("{} · {}", browser.title, browser.summary));
        for line in &browser.lines {
            lines.push(format!("· {line}"));
        }
    } else {
        lines.push("recall browser idle (use :memories or /memories or /search …)".into());
    }

    if let Some(preview) = state.context_preview.as_ref() {
        lines.push(format!("context preview · {}", preview.summary));
        if let Some(selected_items) = preview.selected_items {
            lines.push(format!("included items {selected_items}"));
        }
        if let Some(omitted_items) = preview.omitted_items {
            lines.push(format!("omitted items {omitted_items}"));
        }
        if let Some(token_budget) = preview.token_budget.as_ref() {
            lines.push(format!(
                "token budget {} / {}",
                token_budget.used_tokens, token_budget.max_tokens
            ));
        }
        append_context_preview_lines(&mut lines, preview);
    } else {
        lines.push("context preview unavailable (send a prompt to build one)".into());
    }

    match state.context_dry_run.as_ref() {
        Some(dry_run) => lines.push(format!(
            "dry run captured at {} · {}",
            dry_run.built_at, dry_run.summary
        )),
        None => lines.push("dry run not captured yet".into()),
    }

    lines
}

pub fn append_context_preview_lines(lines: &mut Vec<String>, preview: &ContextPreview) {
    for line in &preview.lines {
        lines.push(format!("· {line}"));
    }
}

pub fn inspector_visibility_label(layout: &LayoutModel, state: &ShellState) -> String {
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

pub fn inspector_focus_label(focus: InspectorFocus) -> &'static str {
    match focus {
        InspectorFocus::Summary => "summary",
        InspectorFocus::Branches => "branches",
        InspectorFocus::Message => "message",
        InspectorFocus::Recall => "recall",
    }
}

