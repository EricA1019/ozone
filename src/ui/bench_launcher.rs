//! Bench Launcher screen — dedicated screen for benchmark/profiling actions.
//!
//! Separated from eval for cleaner UX.

use ratatui::{
    layout::{Constraint, Direction, Layout},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use super::{App, Screen};
use crate::theme::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BenchLauncherAction {
    ProfileModel,
    QuickSweep,
    ExportServer,
    Back,
}

#[derive(Debug, Clone)]
pub(super) struct BenchLauncherEntry {
    pub action: BenchLauncherAction,
    pub label: String,
    pub description: String,
    pub command: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BenchLauncherOutcome {
    Continue,
    ExitLauncher,
}

pub(super) fn entries(_app: &App) -> Vec<BenchLauncherEntry> {
    vec![
        BenchLauncherEntry {
            action: BenchLauncherAction::ProfileModel,
            label: "Profile Model".into(),
            description: "Benchmark/sweep workflow — auto-tune GPU layers".into(),
            command: "profile",
        },
        BenchLauncherEntry {
            action: BenchLauncherAction::QuickSweep,
            label: "Quick Sweep".into(),
            description: "Run context sweep against loaded model".into(),
            command: "quick-sweep",
        },
        BenchLauncherEntry {
            action: BenchLauncherAction::ExportServer,
            label: "Export Server".into(),
            description: "Generate a standalone launch script".into(),
            command: "export-server",
        },
        BenchLauncherEntry {
            action: BenchLauncherAction::Back,
            label: "Back".into(),
            description: "Return to main launcher".into(),
            command: "back",
        },
    ]
}

pub(super) fn render(f: &mut Frame, app: &App) {
    let area = f.area();
    let center = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Min(18),
            Constraint::Fill(1),
        ])
        .split(area)[1];
    let center_h = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Max(70),
            Constraint::Fill(1),
        ])
        .split(center)[1];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Fill(1),
            Constraint::Length(2),
        ])
        .split(center_h);

    let header = Paragraph::new(Line::from(vec![
        Span::styled(" Benchmarks ", style_bold_cyan()),
        Span::styled("  ·  profile, sweep & export", style_muted()),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(style_cyan()),
    );
    f.render_widget(header, chunks[0]);

    let bench_entries = entries(app);
    let items: Vec<ListItem> = bench_entries
        .iter()
        .enumerate()
        .map(|(idx, entry)| {
            let is_sel = idx == app.bench_launcher_selected;
            let marker = if is_sel { "▶ " } else { "  " };
            let marker_span =
                Span::styled(marker, if is_sel { style_lime() } else { style_muted() });
            let label_style = if is_sel {
                style_bold_lime()
            } else {
                style_bold_cyan()
            };
            let desc_style = if is_sel { style_lime() } else { style_gray() };
            ListItem::new(Line::from(vec![
                marker_span,
                Span::styled(format!("{} ", entry.label), label_style),
                Span::styled(entry.description.as_str(), desc_style),
                Span::styled(format!("  /{}", entry.command), style_muted()),
            ]))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(Span::styled(" Benchmarks ", style_bold_cyan()))
            .borders(Borders::ALL)
            .border_style(style_gray()),
    );

    f.render_stateful_widget(
        list,
        chunks[1],
        &mut ratatui::widgets::ListState::default()
            .with_selected(Some(app.bench_launcher_selected)),
    );

    let hints = Paragraph::new(Line::from(vec![
        Span::styled("↑↓", style_hint_key()),
        Span::styled(" navigate  ", style_muted()),
        Span::styled("Enter", style_hint_key()),
        Span::styled(" select  ", style_muted()),
        Span::styled("Esc/q", style_hint_key()),
        Span::styled(" back", style_muted()),
    ]))
    .block(Block::default().borders(Borders::NONE));
    f.render_widget(hints, chunks[2]);
}

pub(super) async fn handle_key(
    app: &mut App,
    key: crossterm::event::KeyEvent,
) -> BenchLauncherOutcome {
    let bench_entries = entries(app);

    match key.code {
        crossterm::event::KeyCode::Esc | crossterm::event::KeyCode::Char('q') => {
            app.screen = Screen::Launcher;
            return BenchLauncherOutcome::ExitLauncher;
        }
        crossterm::event::KeyCode::Up | crossterm::event::KeyCode::Char('k') => {
            if bench_entries.is_empty() {
                return BenchLauncherOutcome::Continue;
            }
            app.bench_launcher_selected = if app.bench_launcher_selected == 0 {
                bench_entries.len() - 1
            } else {
                app.bench_launcher_selected - 1
            };
        }
        crossterm::event::KeyCode::Down | crossterm::event::KeyCode::Char('j') => {
            if bench_entries.is_empty() {
                return BenchLauncherOutcome::Continue;
            }
            app.bench_launcher_selected = if app.bench_launcher_selected + 1 >= bench_entries.len()
            {
                0
            } else {
                app.bench_launcher_selected + 1
            };
        }
        crossterm::event::KeyCode::Enter => {
            let idx = app.bench_launcher_selected;
            if let Some(entry) = bench_entries.get(idx) {
                dispatch_action(app, entry.action);
            }
        }
        _ => {}
    }

    BenchLauncherOutcome::Continue
}

fn dispatch_action(app: &mut App, action: BenchLauncherAction) {
    match action {
        BenchLauncherAction::ProfileModel => {
            #[cfg(feature = "profiling-ui")]
            {
                app.screen = Screen::ProfileAdvisory;
            }
            #[cfg(not(feature = "profiling-ui"))]
            {
                app.set_error(
                    "Profiling UI not compiled in. Rebuild with --features profiling-ui.".into(),
                );
            }
        }
        BenchLauncherAction::QuickSweep => {
            #[cfg(feature = "profiling-ui")]
            {
                use crate::profiling::ProfilingAction;
                app.profiling_pending_action = Some(ProfilingAction::QuickSweep);
                app.screen = Screen::ProfileConfirm;
            }
            #[cfg(not(feature = "profiling-ui"))]
            {
                app.set_error(
                    "Profiling UI not compiled in. Rebuild with --features profiling-ui.".into(),
                );
            }
        }
        BenchLauncherAction::ExportServer => {
            app.screen = Screen::Launcher;
            app.set_status("Export server: run `oz export-server` from CLI".into());
        }
        BenchLauncherAction::Back => {
            app.screen = Screen::Launcher;
        }
    }
}
