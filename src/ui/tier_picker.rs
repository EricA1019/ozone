//! Tier picker screen for first-run or --pick flag

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::prefs::Tier;
use crate::theme::{
    style_bold_lime, style_cyan, style_gray, style_lime, style_violet,
    HEX, HEX_FILLED, LIME, TAGLINE, VIOLET,
};

/// State for the tier picker
pub struct TierPickerState {
    pub selected: usize,
}

impl Default for TierPickerState {
    fn default() -> Self {
        Self { selected: 1 } // Default to base (ozone)
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

/// Render the tier picker screen
pub fn render_tier_picker(f: &mut Frame, area: Rect, state: &TierPickerState) {
    // Center the picker vertically and horizontally
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

    // Outer block
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(style_lime());
    let inner = block.inner(picker_area);
    f.render_widget(block, picker_area);

    // Layout inside block
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

    // Hex flourish
    let flourish = Line::from(vec![
        Span::styled(format!(" {HEX}  {HEX}  {HEX}"), style_lime()),
    ]);
    f.render_widget(
        Paragraph::new(flourish).alignment(Alignment::Center),
        chunks[0],
    );

    // Title
    let title = Line::from(vec![
        Span::styled(" Choose Your Tier ", style_bold_lime()),
    ]);
    f.render_widget(
        Paragraph::new(title).alignment(Alignment::Center),
        chunks[2],
    );

    // Tagline
    let tagline = Line::from(vec![Span::styled(TAGLINE, style_gray())]);
    f.render_widget(
        Paragraph::new(tagline).alignment(Alignment::Center),
        chunks[3],
    );

    // Divider
    let divider = format!("{HEX}───────────────────────────{HEX}");
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(&divider, style_gray())))
            .alignment(Alignment::Center),
        chunks[4],
    );

    // Tier list
    let items: Vec<ListItem> = TIERS
        .iter()
        .enumerate()
        .map(|(i, (tier, name, desc))| {
            let selected = i == state.selected;
            let bullet = if selected { HEX_FILLED } else { HEX };
            
            // ozone+ gets violet accent, others get lime/cyan
            let (name_style, bullet_style) = if *tier == Tier::Plus {
                if selected {
                    (Style::default().fg(VIOLET).add_modifier(Modifier::BOLD), style_violet())
                } else {
                    (style_gray(), style_gray())
                }
            } else if selected {
                (Style::default().fg(LIME).add_modifier(Modifier::BOLD), style_lime())
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

    // Hint
    let hint = Line::from(vec![
        Span::styled("[↑↓] ", style_lime()),
        Span::styled("Select  ", style_gray()),
        Span::styled("[Enter] ", style_lime()),
        Span::styled("Confirm  ", style_gray()),
        Span::styled("[Q] ", style_lime()),
        Span::styled("Quit", style_gray()),
    ]);
    f.render_widget(
        Paragraph::new(hint).alignment(Alignment::Center),
        chunks[8],
    );

    // Hex footer
    let footer = Line::from(vec![
        Span::styled(format!(" {HEX}  {HEX}  {HEX}"), style_lime()),
    ]);
    f.render_widget(
        Paragraph::new(footer).alignment(Alignment::Center),
        chunks[9],
    );
}
