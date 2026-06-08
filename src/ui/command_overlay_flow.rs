use crossterm::event::{KeyEvent, KeyCode};
use ratatui::style::{Color, Modifier, Style};
use tui_textarea::TextArea;
use std::time::Instant;

use super::{launcher, App, Screen};

pub(super) fn new_command_overlay() -> TextArea<'static> {
    let mut textarea = TextArea::default();
    textarea.set_block(ratatui::widgets::Block::default());
    textarea.set_cursor_style(
        Style::default()
            .fg(Color::Black)
            .bg(crate::theme::LIME)
            .add_modifier(Modifier::BOLD),
    );
    textarea.set_selection_style(Style::default().fg(Color::Black).bg(crate::theme::CYAN));
    textarea.set_placeholder_text("Type a launcher command");
    textarea.set_placeholder_style(crate::theme::style_muted());
    textarea.set_style(crate::theme::style_cyan());
    textarea.set_max_histories(64);
    textarea.set_tab_length(0);
    textarea
}

pub(super) fn overlay_supported(screen: &Screen) -> bool {
    matches!(
        screen,
        Screen::Launcher
            | Screen::BenchEval
            | Screen::BenchEvalRunning
            | Screen::BenchEvalReport
            | Screen::ModelPicker
            | Screen::ConfigureHub
            | Screen::Confirm
            | Screen::Settings
            | Screen::Monitor
    )
}

pub(super) fn open_command_overlay(app: &mut App) {
    app.command_overlay_open = true;
    app.command_overlay = new_command_overlay();
    app.command_overlay_selected = 0;
}

pub(super) fn close_command_overlay(app: &mut App) {
    app.command_overlay_open = false;
    app.command_overlay = new_command_overlay();
    app.command_overlay_selected = 0;
}

pub(super) fn sync_command_overlay_selection(app: &mut App) {
    let count = launcher::filtered_launcher_actions(app).len();
    if count == 0 {
        app.command_overlay_selected = 0;
    } else if app.command_overlay_selected >= count {
        app.command_overlay_selected = count - 1;
    }
}

pub(super) fn normalize_command_overlay(app: &mut App) {
    let normalized = app.command_overlay_query();
    app.command_overlay = new_command_overlay();
    if !normalized.is_empty() {
        app.command_overlay.insert_str(normalized);
    }
    app.command_overlay_selected = 0;
}

pub(super) fn input_command_overlay(app: &mut App, key: KeyEvent) {
    app.command_overlay.input(key);
    normalize_command_overlay(app);
    sync_command_overlay_selection(app);
}

pub(super) async fn handle_command_overlay_key(
    app: &mut App,
    key: KeyEvent,
    last_refresh: &mut Instant,
) -> anyhow::Result<super::LauncherActionOutcome> {
    match key.code {
        KeyCode::Esc => {
            close_command_overlay(app);
        }
        KeyCode::Up => {
            if app.command_overlay_selected > 0 {
                app.command_overlay_selected -= 1;
            }
        }
        KeyCode::Down => {
            let count = launcher::filtered_launcher_actions(app).len();
            if app.command_overlay_selected + 1 < count {
                app.command_overlay_selected += 1;
            }
        }
        KeyCode::Enter => {
            let selected = launcher::filtered_launcher_actions(app)
                .get(app.command_overlay_selected)
                .map(|action| action.id);
            close_command_overlay(app);
            if let Some(action) = selected {
                return Ok(super::run_launcher_action(app, action, last_refresh).await);
            }
        }
        _ => {
            input_command_overlay(app, key);
        }
    }

    Ok(super::LauncherActionOutcome::Continue)
}
