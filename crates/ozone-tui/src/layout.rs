use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::{
    app::{FocusTarget, ScreenState, ShellState},
    input::InputMode,
};

pub const DEFAULT_VIEWPORT_WIDTH: u16 = 120;
pub const DEFAULT_VIEWPORT_HEIGHT: u16 = 40;
const INSPECTOR_THRESHOLD_WIDTH: u16 = 120;
const INSPECTOR_MIN_WIDTH: u16 = 28;
const INSPECTOR_MAX_WIDTH: u16 = 36;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    Compact,
    Wide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneId {
    Conversation,
    Composer,
    Status,
    Inspector,
    HelpOverlay,
    QuitOverlay,
    FullScreen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneLayout {
    pub pane: PaneId,
    pub area: Rect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutModel {
    pub viewport: Rect,
    pub mode: LayoutMode,
    pub conversation: PaneLayout,
    pub composer: PaneLayout,
    pub status: PaneLayout,
    pub inspector: Option<PaneLayout>,
    pub overlay: Option<PaneLayout>,
    pub menu_area: Option<PaneLayout>,
    pub focused: PaneId,
}

impl LayoutModel {
    pub fn pane(&self, pane: PaneId) -> Option<Rect> {
        match pane {
            PaneId::Conversation => Some(self.conversation.area),
            PaneId::Composer => Some(self.composer.area),
            PaneId::Status => Some(self.status.area),
            PaneId::Inspector => self.inspector.map(|pane| pane.area),
            PaneId::FullScreen => self.menu_area.map(|pane| pane.area),
            PaneId::HelpOverlay | PaneId::QuitOverlay => self
                .overlay
                .filter(|overlay| overlay.pane == pane)
                .map(|overlay| overlay.area),
        }
    }

    pub fn is_wide(&self) -> bool {
        matches!(self.mode, LayoutMode::Wide)
    }
}

pub fn build_layout(state: &ShellState) -> LayoutModel {
    build_layout_for_area(
        state,
        Rect::new(0, 0, DEFAULT_VIEWPORT_WIDTH, DEFAULT_VIEWPORT_HEIGHT),
    )
}

pub fn build_layout_for_area(state: &ShellState, viewport: Rect) -> LayoutModel {
    // For menu screens, use a full-screen centered layout
    if is_menu_screen(state.screen) {
        return build_menu_layout(state, viewport);
    }

    let mode = if viewport.width >= INSPECTOR_THRESHOLD_WIDTH {
        LayoutMode::Wide
    } else {
        LayoutMode::Compact
    };
    let show_inspector = matches!(mode, LayoutMode::Wide) && state.inspector.visible;

    let (main_area, inspector) = if show_inspector {
        let width = inspector_width(viewport.width);
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1), Constraint::Length(width)])
            .split(viewport);

        (
            columns[0],
            Some(PaneLayout {
                pane: PaneId::Inspector,
                area: columns[1],
            }),
        )
    } else {
        (viewport, None)
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(composer_height(state.input_mode, viewport.height)),
            Constraint::Length(status_height(viewport.height)),
        ])
        .split(main_area);

    LayoutModel {
        viewport,
        mode,
        conversation: PaneLayout {
            pane: PaneId::Conversation,
            area: rows[0],
        },
        composer: PaneLayout {
            pane: PaneId::Composer,
            area: rows[1],
        },
        status: PaneLayout {
            pane: PaneId::Status,
            area: rows[2],
        },
        inspector,
        overlay: overlay_for_screen(state.screen, viewport),
        menu_area: None,
        focused: focused_pane(state.focus),
    }
}

fn is_menu_screen(screen: ScreenState) -> bool {
    matches!(
        screen,
        ScreenState::MainMenu
            | ScreenState::SessionList
            | ScreenState::CharacterManager
            | ScreenState::CharacterCreate
            | ScreenState::CharacterImport
            | ScreenState::Settings
    )
}

fn build_menu_layout(state: &ShellState, viewport: Rect) -> LayoutModel {
    let mode = if viewport.width >= INSPECTOR_THRESHOLD_WIDTH {
        LayoutMode::Wide
    } else {
        LayoutMode::Compact
    };

    // Menu content area: centered with padding
    let content_width = viewport.width.min(80);
    let content_height = viewport.height.saturating_sub(2); // 1 row top/bottom padding
    let x = viewport.x + (viewport.width.saturating_sub(content_width)) / 2;
    let y = viewport.y + 1; // 1 row top padding

    let menu_area = Some(PaneLayout {
        pane: PaneId::FullScreen,
        area: Rect::new(x, y, content_width, content_height),
    });

    // Still need conversation/composer/status rects for the LayoutModel struct,
    // but they won't be rendered on menu screens. Use zero-height rects.
    let zero_area = Rect::new(0, 0, 0, 0);

    LayoutModel {
        viewport,
        mode,
        conversation: PaneLayout {
            pane: PaneId::Conversation,
            area: zero_area,
        },
        composer: PaneLayout {
            pane: PaneId::Composer,
            area: zero_area,
        },
        status: PaneLayout {
            pane: PaneId::Status,
            area: zero_area,
        },
        inspector: None,
        overlay: overlay_for_screen(state.screen, viewport),
        menu_area,
        focused: PaneId::FullScreen,
    }
}

fn focused_pane(focus: FocusTarget) -> PaneId {
    match focus {
        FocusTarget::Transcript => PaneId::Conversation,
        FocusTarget::Draft => PaneId::Composer,
        FocusTarget::Status => PaneId::Status,
    }
}

fn composer_height(input_mode: InputMode, viewport_height: u16) -> u16 {
    let base = match input_mode {
        InputMode::Normal => 3,
        InputMode::Insert => 5,
        InputMode::Command => 4,
    };

    if viewport_height >= 36 {
        base + 1
    } else {
        base
    }
}

fn status_height(_viewport_height: u16) -> u16 {
    1
}

fn inspector_width(viewport_width: u16) -> u16 {
    (((viewport_width as u32) * 28) / 100)
        .clamp(INSPECTOR_MIN_WIDTH as u32, INSPECTOR_MAX_WIDTH as u32) as u16
}

fn overlay_for_screen(screen: ScreenState, viewport: Rect) -> Option<PaneLayout> {
    let pane = match screen {
        ScreenState::MainMenu
        | ScreenState::SessionList
        | ScreenState::CharacterManager
        | ScreenState::CharacterCreate
        | ScreenState::CharacterImport
        | ScreenState::Settings
        | ScreenState::ModelIntelligence
        | ScreenState::Conversation => return None,
        ScreenState::Help => PaneId::HelpOverlay,
        ScreenState::Quit => PaneId::QuitOverlay,
    };

    Some(PaneLayout {
        pane,
        area: centered_rect(
            viewport,
            viewport.width.saturating_sub(8).clamp(32, 72),
            viewport.height.saturating_sub(8).clamp(8, 12),
        ),
    })
}

fn centered_rect(viewport: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(viewport.width);
    let height = height.min(viewport.height);
    let x = viewport.x + viewport.width.saturating_sub(width) / 2;
    let y = viewport.y + viewport.height.saturating_sub(height) / 2;

    Rect::new(x, y, width, height)
}

#[cfg(test)]
mod tests {
    use ozone_core::session::SessionId;
    use ratatui::layout::Rect;

    use super::{build_layout_for_area, LayoutMode, PaneId};
    use crate::{
        app::{ScreenState, SessionContext, ShellState},
        input::{InputMode, KeyAction},
    };

    fn seeded_state() -> ShellState {
        let session_id = SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap();
        let mut state = ShellState::new(SessionContext::new(session_id, "Phase 1C"));
        state.enter_conversation();
        state
    }

    #[test]
    fn layout_80x24_uses_compact_shell_without_inspector() {
        let mut state = seeded_state();
        state.inspector.visible = true;
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));

        assert_eq!(layout.mode, LayoutMode::Compact);
        assert!(layout.inspector.is_none());
        assert_eq!(layout.conversation.area, Rect::new(0, 0, 80, 20));
        assert_eq!(layout.composer.area, Rect::new(0, 20, 80, 3));
        assert_eq!(layout.status.area, Rect::new(0, 23, 80, 1));
        assert_eq!(layout.focused, PaneId::Composer);
    }

    #[test]
    fn layout_120x40_shows_inspector_placeholder() {
        let mut state = seeded_state();
        state.inspector.visible = true;
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 120, 40));
        let inspector = layout.inspector.expect("wide layout should show inspector");

        assert_eq!(layout.mode, LayoutMode::Wide);
        assert_eq!(layout.conversation.area, Rect::new(0, 0, 87, 35));
        assert_eq!(layout.composer.area, Rect::new(0, 35, 87, 4));
        assert_eq!(layout.status.area, Rect::new(0, 39, 87, 1));
        assert_eq!(inspector.area, Rect::new(87, 0, 33, 40));
    }

    #[test]
    fn help_overlay_is_centered_and_insert_mode_expands_composer() {
        let mut state = seeded_state();
        state.apply_action(KeyAction::ToggleHelp);
        state.input_mode = InputMode::Insert;

        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));
        let overlay = layout.overlay.expect("help screen should allocate overlay");

        assert_eq!(state.screen, ScreenState::Help);
        assert_eq!(overlay.pane, PaneId::HelpOverlay);
        assert_eq!(layout.composer.area.height, 5);
        assert_eq!(overlay.area, Rect::new(4, 6, 72, 12));
    }

    #[test]
    fn menu_screen_uses_fullscreen_layout() {
        let mut state = seeded_state();
        state.screen = ScreenState::MainMenu;
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 120, 40));

        assert!(layout.menu_area.is_some());
        let menu = layout.menu_area.unwrap();
        assert_eq!(menu.pane, PaneId::FullScreen);
        // Content should be centered, max 80 wide
        assert_eq!(menu.area.width, 80);
        assert_eq!(menu.area.height, 38); // 40 - 2 padding
        assert_eq!(menu.area.x, 20); // (120-80)/2
        assert_eq!(menu.area.y, 1); // 1 top padding
        assert_eq!(layout.focused, PaneId::FullScreen);
    }

    #[test]
    fn session_list_screen_uses_fullscreen_layout() {
        let mut state = seeded_state();
        state.screen = ScreenState::SessionList;
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));

        assert!(layout.menu_area.is_some());
        let menu = layout.menu_area.unwrap();
        assert_eq!(menu.area.width, 80);
        assert_eq!(menu.area.height, 22);
        assert_eq!(layout.focused, PaneId::FullScreen);
    }

    #[test]
    fn conversation_screen_has_no_menu_area() {
        let state = seeded_state();
        let layout = build_layout_for_area(&state, Rect::new(0, 0, 80, 24));

        assert!(layout.menu_area.is_none());
        assert_eq!(layout.focused, PaneId::Composer);
    }
}
