//! Tier picker screen for first-run or --pick flag

use std::path::{Path, PathBuf};

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::prefs::Tier;
use crate::theme::{
    style_bold_bright_violet, style_bold_lime, style_bright_violet, style_cyan, style_gray,
    style_green, style_lime, style_red, HEX, HEX_FILLED, LIME, TAGLINE,
};

/// Phases of the tier picker flow
#[derive(Clone)]
pub enum TierPickerPhase {
    /// Main selection list
    Picking,
    /// Asking user to confirm a GitHub download
    ConfirmingDownload { tier: Tier, binary: String },
    /// Download in progress (background thread running)
    Installing { tier: Tier, binary: String },
    /// Download succeeded
    InstallDone { tier: Tier, path: PathBuf },
    /// Download failed
    InstallError { _tier: Tier, msg: String },
}

/// State for the tier picker
pub struct TierPickerState {
    pub selected: usize,
    pub phase: TierPickerPhase,
    /// Receives the result from the background install thread.
    pub install_rx: Option<std::sync::mpsc::Receiver<Result<PathBuf, String>>>,
}

impl Default for TierPickerState {
    fn default() -> Self {
        Self {
            selected: 1, // Default to base (ozone)
            phase: TierPickerPhase::Picking,
            install_rx: None,
        }
    }
}

impl TierPickerState {
    pub fn selected_tier(&self) -> Tier {
        match self.selected {
            0 => Tier::Lite,
            1 => Tier::Base,
            2 => Tier::Plus,
            _ => Tier::Base,
        }
    }

    pub fn up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn down(&mut self) {
        if self.selected < 2 {
            self.selected += 1;
        }
    }
}

const TIERS: [(Tier, &str, &str); 3] = [
    (Tier::Lite, "ozonelite", "Launch + monitor only"),
    (Tier::Base, "ozone", "Launch + bench + sweep + analyze"),
    (Tier::Plus, "ozone+", "Chat shell with memory & sessions"),
];

/// Render the tier picker screen — dispatches based on current phase.
pub fn render_tier_picker(f: &mut Frame, area: Rect, state: &TierPickerState, ticker: u64) {
    match &state.phase {
        TierPickerPhase::Picking => render_picking(f, area, state),
        TierPickerPhase::ConfirmingDownload { tier: _, binary } => {
            render_confirm_download(f, area, binary)
        }
        TierPickerPhase::Installing { binary, .. } => render_installing(f, area, binary, ticker),
        TierPickerPhase::InstallDone { tier, path } => {
            let binary = super::tier_install::binary_name_for_tier(*tier);
            render_install_done(f, area, binary, path)
        }
        TierPickerPhase::InstallError { msg, .. } => render_install_error(f, area, msg),
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Centers a fixed-size dialog box and returns (outer_rect, inner_rect).
fn centered_dialog(area: Rect, width: u16, height: u16) -> (Rect, Rect) {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(height),
            Constraint::Min(1),
        ])
        .split(area);
    let horiz = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(width),
            Constraint::Min(1),
        ])
        .split(vert[1]);
    let outer = horiz[1];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(style_lime());
    let inner = block.inner(outer);
    (outer, inner)
}

fn render_dialog_frame(f: &mut Frame, outer: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(style_lime());
    f.render_widget(block, outer);
}

fn hex_line() -> Line<'static> {
    Line::from(Span::styled(format!(" {HEX}  {HEX}  {HEX}"), style_lime()))
}

fn divider_line() -> Line<'static> {
    let d = format!("{HEX}───────────────────────────{HEX}");
    Line::from(Span::styled(d, style_gray()))
}

// ── phase renders ─────────────────────────────────────────────────────────────

fn render_picking(f: &mut Frame, area: Rect, state: &TierPickerState) {
    let center_v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(16),
            Constraint::Min(1),
        ])
        .split(area);

    let center_h = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(60),
            Constraint::Min(1),
        ])
        .split(center_v[1]);

    let picker_area = center_h[1];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(style_lime());
    let inner = block.inner(picker_area);
    f.render_widget(block, picker_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // hex flourish
            Constraint::Length(1), // spacer
            Constraint::Length(1), // title
            Constraint::Length(1), // tagline
            Constraint::Length(1), // divider
            Constraint::Length(1), // spacer
            Constraint::Length(5), // tier list (3 tiers + spacing)
            Constraint::Length(1), // spacer
            Constraint::Length(1), // hint
            Constraint::Length(1), // hex footer
        ])
        .split(inner);

    f.render_widget(
        Paragraph::new(hex_line()).alignment(Alignment::Center),
        chunks[0],
    );

    let title = Line::from(Span::styled(" Choose Your Tier ", style_bold_lime()));
    f.render_widget(
        Paragraph::new(title).alignment(Alignment::Center),
        chunks[2],
    );

    let tagline = Line::from(Span::styled(TAGLINE, style_gray()));
    f.render_widget(
        Paragraph::new(tagline).alignment(Alignment::Center),
        chunks[3],
    );

    f.render_widget(
        Paragraph::new(divider_line()).alignment(Alignment::Center),
        chunks[4],
    );

    let items: Vec<ListItem> = TIERS
        .iter()
        .enumerate()
        .map(|(i, (tier, name, desc))| {
            let selected = i == state.selected;
            let bullet = if selected { HEX_FILLED } else { HEX };

            let (name_style, bullet_style) = if *tier == Tier::Plus {
                if selected {
                    (style_bold_bright_violet(), style_bright_violet())
                } else {
                    (style_gray(), style_gray())
                }
            } else if selected {
                (
                    Style::default().fg(LIME).add_modifier(Modifier::BOLD),
                    style_lime(),
                )
            } else {
                (style_gray(), style_gray())
            };

            let desc_style = if selected { style_cyan() } else { style_gray() };

            ListItem::new(Line::from(vec![
                Span::styled(format!("  {bullet} "), bullet_style),
                Span::styled(*name, name_style),
                Span::raw("  "),
                Span::styled(*desc, desc_style),
            ]))
        })
        .collect();

    f.render_widget(List::new(items), chunks[6]);

    let hint = Line::from(vec![
        Span::styled("[↑↓] ", style_lime()),
        Span::styled("Select  ", style_gray()),
        Span::styled("[Enter] ", style_lime()),
        Span::styled("Confirm  ", style_gray()),
        Span::styled("[Q] ", style_lime()),
        Span::styled("Quit", style_gray()),
    ]);
    f.render_widget(Paragraph::new(hint).alignment(Alignment::Center), chunks[8]);

    f.render_widget(
        Paragraph::new(hex_line()).alignment(Alignment::Center),
        chunks[9],
    );
}

fn render_confirm_download(f: &mut Frame, area: Rect, binary: &str) {
    let (outer, inner) = centered_dialog(area, 60, 16);
    render_dialog_frame(f, outer);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // hex flourish
            Constraint::Length(1), // spacer
            Constraint::Length(1), // title
            Constraint::Length(1), // divider
            Constraint::Length(1), // spacer
            Constraint::Length(1), // line 1: not installed
            Constraint::Length(1), // line 2: download prompt
            Constraint::Length(1), // spacer
            Constraint::Length(1), // install dir note
            Constraint::Length(1), // spacer
            Constraint::Length(1), // hint
            Constraint::Length(1), // spacer
            Constraint::Length(1), // hex footer
            Constraint::Min(0),
        ])
        .split(inner);

    f.render_widget(
        Paragraph::new(hex_line()).alignment(Alignment::Center),
        chunks[0],
    );

    let title = Line::from(Span::styled(
        format!(" Install {binary}? "),
        style_bold_lime(),
    ));
    f.render_widget(
        Paragraph::new(title).alignment(Alignment::Center),
        chunks[2],
    );

    f.render_widget(
        Paragraph::new(divider_line()).alignment(Alignment::Center),
        chunks[3],
    );

    let not_installed = Line::from(vec![
        Span::styled(format!("  {binary}"), style_lime()),
        Span::styled(" is not installed on this system.", style_gray()),
    ]);
    f.render_widget(Paragraph::new(not_installed), chunks[5]);

    let prompt = Line::from(Span::styled(
        "  Download from GitHub releases? (~5-10 MB)",
        style_cyan(),
    ));
    f.render_widget(Paragraph::new(prompt), chunks[6]);

    let dir_note = Line::from(Span::styled(
        "  Installs to: ~/.local/bin  or  ~/.cargo/bin",
        style_gray(),
    ));
    f.render_widget(Paragraph::new(dir_note), chunks[8]);

    let hint = Line::from(vec![
        Span::styled("[Y/Enter] ", style_lime()),
        Span::styled("Download   ", style_gray()),
        Span::styled("[N/Esc] ", style_lime()),
        Span::styled("Use Lite instead", style_gray()),
    ]);
    f.render_widget(
        Paragraph::new(hint).alignment(Alignment::Center),
        chunks[10],
    );

    f.render_widget(
        Paragraph::new(hex_line()).alignment(Alignment::Center),
        chunks[12],
    );
}

fn render_installing(f: &mut Frame, area: Rect, binary: &str, ticker: u64) {
    let (outer, inner) = centered_dialog(area, 60, 16);
    render_dialog_frame(f, outer);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // hex flourish
            Constraint::Length(1), // spacer
            Constraint::Length(1), // title
            Constraint::Length(1), // divider
            Constraint::Min(0),    // flex padding
            Constraint::Length(1), // spinner + text
            Constraint::Length(1), // note
            Constraint::Min(0),    // flex padding
            Constraint::Length(1), // hex footer
        ])
        .split(inner);

    f.render_widget(
        Paragraph::new(hex_line()).alignment(Alignment::Center),
        chunks[0],
    );

    let title = Line::from(Span::styled(" Downloading... ", style_bold_lime()));
    f.render_widget(
        Paragraph::new(title).alignment(Alignment::Center),
        chunks[2],
    );

    f.render_widget(
        Paragraph::new(divider_line()).alignment(Alignment::Center),
        chunks[3],
    );

    const SPINNER: [char; 4] = ['|', '/', '-', '\\'];
    let spin = SPINNER[(ticker / 4 % 4) as usize];
    let downloading = Line::from(vec![
        Span::styled(format!("  {spin}  "), style_lime()),
        Span::styled(format!("Downloading {binary}..."), style_cyan()),
    ]);
    f.render_widget(Paragraph::new(downloading), chunks[5]);

    let note = Line::from(Span::styled(
        "  Please wait. Do not close this window.",
        style_gray(),
    ));
    f.render_widget(Paragraph::new(note), chunks[6]);

    f.render_widget(
        Paragraph::new(hex_line()).alignment(Alignment::Center),
        chunks[8],
    );
}

fn render_install_done(f: &mut Frame, area: Rect, binary: &str, path: &Path) {
    let (outer, inner) = centered_dialog(area, 60, 16);
    render_dialog_frame(f, outer);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // hex flourish
            Constraint::Length(1), // spacer
            Constraint::Length(1), // title
            Constraint::Length(1), // divider
            Constraint::Min(0),    // flex
            Constraint::Length(1), // success line
            Constraint::Length(1), // path line
            Constraint::Length(1), // spacer
            Constraint::Length(1), // run hint
            Constraint::Min(0),    // flex
            Constraint::Length(1), // continue hint
            Constraint::Length(1), // hex footer
        ])
        .split(inner);

    f.render_widget(
        Paragraph::new(hex_line()).alignment(Alignment::Center),
        chunks[0],
    );

    let title = Line::from(Span::styled(" Install Complete ", style_bold_lime()));
    f.render_widget(
        Paragraph::new(title).alignment(Alignment::Center),
        chunks[2],
    );

    f.render_widget(
        Paragraph::new(divider_line()).alignment(Alignment::Center),
        chunks[3],
    );

    let success = Line::from(vec![
        Span::styled("  ✓ ", style_green()),
        Span::styled(binary.to_string(), style_bold_lime()),
        Span::styled(" installed successfully.", style_gray()),
    ]);
    f.render_widget(Paragraph::new(success), chunks[5]);

    let path_str = path.display().to_string();
    let path_line = Line::from(vec![
        Span::styled("  Path: ", style_gray()),
        Span::styled(path_str, style_cyan()),
    ]);
    f.render_widget(Paragraph::new(path_line), chunks[6]);

    let run_hint = Line::from(vec![
        Span::styled("  Run: ", style_gray()),
        Span::styled(binary.to_string(), style_lime()),
    ]);
    f.render_widget(Paragraph::new(run_hint), chunks[8]);

    let cont = Line::from(vec![
        Span::styled("[Enter] ", style_lime()),
        Span::styled("Continue", style_gray()),
    ]);
    f.render_widget(
        Paragraph::new(cont).alignment(Alignment::Center),
        chunks[10],
    );

    f.render_widget(
        Paragraph::new(hex_line()).alignment(Alignment::Center),
        chunks[11],
    );
}

fn render_install_error(f: &mut Frame, area: Rect, msg: &str) {
    let (outer, inner) = centered_dialog(area, 60, 16);
    render_dialog_frame(f, outer);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // hex flourish
            Constraint::Length(1), // spacer
            Constraint::Length(1), // title
            Constraint::Length(1), // divider
            Constraint::Length(1), // spacer
            Constraint::Length(3), // error message (up to 3 wrapped lines)
            Constraint::Length(1), // spacer
            Constraint::Length(1), // fallback note
            Constraint::Min(0),    // flex
            Constraint::Length(1), // hint
            Constraint::Length(1), // hex footer
        ])
        .split(inner);

    f.render_widget(
        Paragraph::new(hex_line()).alignment(Alignment::Center),
        chunks[0],
    );

    let title = Line::from(Span::styled(" Install Failed ", style_bold_lime()));
    f.render_widget(
        Paragraph::new(title).alignment(Alignment::Center),
        chunks[2],
    );

    f.render_widget(
        Paragraph::new(divider_line()).alignment(Alignment::Center),
        chunks[3],
    );

    let err_para = Paragraph::new(vec![
        Line::from(Span::styled("  ✗ ", style_red())),
        Line::from(Span::styled(format!("  {msg}"), style_gray())),
    ])
    .wrap(Wrap { trim: false });
    f.render_widget(err_para, chunks[5]);

    let fallback = Line::from(vec![
        Span::styled("  Falling back to ", style_gray()),
        Span::styled("Lite", style_lime()),
        Span::styled(" mode.", style_gray()),
    ]);
    f.render_widget(Paragraph::new(fallback), chunks[7]);

    let hint = Line::from(vec![
        Span::styled("[Enter] ", style_lime()),
        Span::styled("Continue with Lite", style_gray()),
    ]);
    f.render_widget(Paragraph::new(hint).alignment(Alignment::Center), chunks[9]);

    f.render_widget(
        Paragraph::new(hex_line()).alignment(Alignment::Center),
        chunks[10],
    );
}
