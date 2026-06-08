use super::model_types::*;
use super::helpers::*;
use crate::layout::PaneLayout;
use crate::theme;
use ratatui::{
    layout::{Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

pub struct ConversationContent {
    pub lines: Vec<Line<'static>>,
    pub total_visual_lines: usize,
    pub selected_range: Option<(usize, usize)>,
}

pub fn render_conversation(frame: &mut Frame, pane: &PaneLayout, model: &RenderModel, focused: bool) {
    let viewport = conversation_viewport(pane.area, &model.title, &model.conversation);
    let block = pane_block(&model.conversation.title, focused);
    let inner = block.inner(pane.area);
    let content_width = inner.width.saturating_sub(1).max(1);
    let content = build_conversation_content(&model.title, &model.conversation, content_width);
    let scroll_offset = model
        .conversation
        .scroll_offset
        .unwrap_or(viewport.default_scroll_offset)
        .min(viewport.max_scroll);

    frame.render_widget(
        Paragraph::new(content.lines)
            .block(block)
            .scroll((scroll_offset as u16, 0)),
        pane.area,
    );

    if content.total_visual_lines > viewport.visible_height && viewport.visible_height > 0 {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));
        let mut scrollbar_state =
            ScrollbarState::new(content.total_visual_lines).position(scroll_offset);
        frame.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
    }
}

pub fn auto_conversation_scroll_offset(
    selected_range: Option<(usize, usize)>,
    visible_height: usize,
    max_scroll: usize,
) -> usize {
    let Some((selected_start, selected_end)) = selected_range else {
        return 0;
    };

    let mut scroll_offset = 0usize;
    if selected_end > visible_height {
        scroll_offset = selected_end.saturating_sub(visible_height);
    }
    if selected_start < scroll_offset {
        scroll_offset = selected_start;
    }
    scroll_offset.min(max_scroll)
}

pub fn build_conversation_content(
    app_title: &str,
    model: &ConversationPaneModel,
    content_width: u16,
) -> ConversationContent {
    const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!("{} ", theme::HEX), theme::brand_hex_style()),
            Span::styled(
                app_title.to_owned(),
                theme::text_style().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::default(),
    ];
    let mut total_visual_lines = rewrap_lines(&mut lines, content_width);
    let mut selected_range: Option<(usize, usize)> = None;

    if model.entries.is_empty() {
        let line = Line::from(Span::styled(model.empty_state.clone(), theme::dim_style()));
        total_visual_lines += push_wrapped_line(&mut lines, line, content_width);
    } else {
        let entry_count = model.entries.len();
        for (i, entry) in model.entries.iter().enumerate() {
            let marker = if entry.selected {
                format!("{} ", theme::HEX_FILLED)
            } else {
                "  ".into()
            };
            let marker_style = if entry.selected {
                theme::highlight_style()
            } else {
                theme::muted_style()
            };
            let author_style = if entry.selected {
                theme::author_selected_style()
            } else if entry.author == "user" {
                theme::author_user_style()
            } else {
                theme::author_style()
            };
            let bookmark_indicator = if entry.is_bookmarked {
                Span::styled("★ ", theme::bookmark_style())
            } else {
                Span::styled("  ", theme::muted_style())
            };

            // Colored left-border gutter per author role
            let gutter_color = if entry.author == "user" {
                theme::TEAL
            } else if entry.author == "system" {
                theme::TEXT_DIM
            } else {
                theme::VIOLET
            };
            let gutter = Span::styled("│ ", Style::default().fg(gutter_color));

            let author_display = if entry.is_streaming {
                let frame_str =
                    SPINNER_FRAMES[(model.tick_count / 3) as usize % SPINNER_FRAMES.len()];
                format!("{} {:<9}", frame_str, &entry.author)
            } else {
                format!("{:<10}", entry.author)
            };

            // Build the author line spans with optional dim timestamp
            let mut msg_spans = vec![
                Span::styled(marker, marker_style),
                bookmark_indicator,
                gutter,
                Span::styled(author_display, author_style),
            ];
            if let Some(ts) = &entry.timestamp {
                msg_spans.push(Span::styled(format!(" {ts}"), theme::dim_style()));
            }
            msg_spans.push(Span::raw(" "));
            msg_spans.push(Span::styled(entry.content.clone(), theme::text_style()));

            let line = Line::from(msg_spans);
            let line_height = push_wrapped_line(&mut lines, line, content_width);
            if entry.selected {
                selected_range = Some((total_visual_lines, total_visual_lines + line_height));
            }
            total_visual_lines += line_height;

            // Author-aware separator between messages
            if i + 1 < entry_count {
                let next_author = &model.entries[i + 1].author;
                let sep = if next_author != &entry.author {
                    format!("     │ ─── {} ───", next_author)
                } else {
                    "     │ · · ·".to_string()
                };
                let line = Line::from(Span::styled(
                    sep,
                    Style::default().fg(Color::Rgb(50, 50, 50)),
                ));
                total_visual_lines += push_wrapped_line(&mut lines, line, content_width);
            }
        }
    }

    let spacer = Line::default();
    total_visual_lines += push_wrapped_line(&mut lines, spacer, content_width);
    let hint_line = Line::from(Span::styled(model.hint.clone(), theme::dim_style()));
    total_visual_lines += push_wrapped_line(&mut lines, hint_line, content_width);

    ConversationContent {
        lines,
        total_visual_lines,
        selected_range,
    }
}

pub fn rewrap_lines(lines: &mut Vec<Line<'static>>, width: u16) -> usize {
    let original = std::mem::take(lines);
    for line in original {
        push_wrapped_line(lines, line, width);
    }
    lines.len()
}

pub fn push_wrapped_line(target: &mut Vec<Line<'static>>, line: Line<'static>, width: u16) -> usize {
    let wrapped = wrap_line(&line, width);
    let added = wrapped.len();
    target.extend(wrapped);
    added
}

pub fn wrap_line(line: &Line, width: u16) -> Vec<Line<'static>> {
    let width = width.max(1) as usize;
    let mut wrapped = Vec::new();
    let mut current_line: Vec<Span<'static>> = Vec::new();
    let mut current_span_text = String::new();
    let mut current_span_style: Option<Style> = None;
    let mut current_width = 0usize;
    let mut ended_with_newline = false;

    let flush_span = |current_line: &mut Vec<Span<'static>>,
                      current_span_text: &mut String,
                      current_span_style: &mut Option<Style>| {
        if !current_span_text.is_empty() {
            current_line.push(Span::styled(
                std::mem::take(current_span_text),
                current_span_style.take().unwrap_or_default(),
            ));
        }
    };
    let flush_line = |wrapped: &mut Vec<Line<'static>>,
                      current_line: &mut Vec<Span<'static>>,
                      current_span_text: &mut String,
                      current_span_style: &mut Option<Style>,
                      current_width: &mut usize| {
        flush_span(current_line, current_span_text, current_span_style);
        wrapped.push(if current_line.is_empty() {
            Line::default()
        } else {
            Line::from(std::mem::take(current_line))
        });
        *current_width = 0;
    };

    for span in &line.spans {
        let style = span.style;
        for ch in span.content.chars() {
            if ch == '\n' {
                ended_with_newline = true;
                flush_line(
                    &mut wrapped,
                    &mut current_line,
                    &mut current_span_text,
                    &mut current_span_style,
                    &mut current_width,
                );
                continue;
            }

            ended_with_newline = false;
            if current_width >= width {
                flush_line(
                    &mut wrapped,
                    &mut current_line,
                    &mut current_span_text,
                    &mut current_span_style,
                    &mut current_width,
                );
            }

            if current_span_style != Some(style) && !current_span_text.is_empty() {
                flush_span(
                    &mut current_line,
                    &mut current_span_text,
                    &mut current_span_style,
                );
            }
            current_span_style = Some(style);
            current_span_text.push(ch);
            current_width += 1;

            if current_width >= width {
                flush_line(
                    &mut wrapped,
                    &mut current_line,
                    &mut current_span_text,
                    &mut current_span_style,
                    &mut current_width,
                );
            }
        }
    }

    flush_span(
        &mut current_line,
        &mut current_span_text,
        &mut current_span_style,
    );
    if !current_line.is_empty() || wrapped.is_empty() || ended_with_newline {
        wrapped.push(if current_line.is_empty() {
            Line::default()
        } else {
            Line::from(current_line)
        });
    }

    wrapped
}

pub(crate) fn conversation_viewport(
    area: Rect,
    app_title: &str,
    model: &ConversationPaneModel,
) -> ConversationViewport {
    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    let content_width = inner.width.saturating_sub(1).max(1);
    let content = build_conversation_content(app_title, model, content_width);
    let visible_height = inner.height as usize;
    let max_scroll = if visible_height == 0 {
        0
    } else {
        content.total_visual_lines.saturating_sub(visible_height)
    };
    let default_scroll_offset = if visible_height == 0 {
        0
    } else if model.entries.last().is_some_and(|entry| entry.is_streaming) {
        max_scroll
    } else {
        auto_conversation_scroll_offset(content.selected_range, visible_height, max_scroll)
    };

    ConversationViewport {
        visible_height,
        max_scroll,
        default_scroll_offset,
    }
}

