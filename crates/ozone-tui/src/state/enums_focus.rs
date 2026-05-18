#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    Transcript,
    Draft,
    Status,
    Inspector,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorFocus {
    Summary,
    Branches,
    Message,
    Recall,
}

#[derive(Debug, Clone)]
pub struct InspectorState {
    pub visible: bool,
    pub focus: InspectorFocus,
}

impl Default for InspectorState {
    fn default() -> Self {
        Self {
            visible: false,
            focus: InspectorFocus::Summary,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub shortcut: Option<char>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuState {
    pub items: Vec<MenuItem>,
    pub selected: usize,
}

impl MenuState {
    pub fn new() -> Self {
        Self {
            items: vec![
                MenuItem { id: "new-chat", label: "New Chat", description: "Start a fresh conversation session", shortcut: Some('1') },
                MenuItem { id: "sessions", label: "Sessions", description: "Browse and resume existing conversations", shortcut: Some('2') },
                MenuItem { id: "characters", label: "Characters", description: "Manage character cards and personas", shortcut: Some('3') },
                MenuItem { id: "settings", label: "Settings", description: "Configure backend, model, and preferences", shortcut: Some('4') },
                MenuItem { id: "help", label: "Help", description: "Keyboard shortcuts and usage guide", shortcut: Some('5') },
                MenuItem { id: "quit", label: "Quit", description: "Exit the application", shortcut: Some('q') },
            ],
            selected: 0,
        }
    }

    pub fn close(&mut self) {
        self.items.clear();
        self.selected = 0;
    }
}

impl Default for MenuState {
    fn default() -> Self {
        Self::new()
    }
}
