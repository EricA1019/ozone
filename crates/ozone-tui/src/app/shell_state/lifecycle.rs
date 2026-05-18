use crate::state::*;
use crate::app::textareas::new_themed_textarea;
use crate::input::InputMode;


impl ShellState {
    fn _new_deprecated(context: SessionContext) -> Self {
        Self {
            screen: ScreenState::MainMenu,
            input_mode: InputMode::Normal,
            focus: FocusTarget::Transcript,
            inspector: InspectorState::default(),
            menu: MenuState::default(),
            session_list: SessionListState::default(),
            folder_picker: FolderPickerState::default(),
            character_list: CharacterListState::default(),
            character_create: CharacterCreateState::default(),
            character_import: CharacterImportState::default(),
            settings: SettingsState::default(),
            session: SessionState::new(context),
            draft: DraftState::default(),
            message_edit: None,
            textarea: new_themed_textarea(),
            history: InputHistoryState::default(),
            status_line: Some("ozone+ TUI shell skeleton ready".into()),
            session_metadata: None,
            session_stats: None,
            context_preview: None,
            context_dry_run: None,
            recall_browser: None,
            pending_actions: Vec::new(),
            runtime_commands: Vec::new(),
            should_quit: false,
            command_palette: CommandPaletteState::default(),
            active_launch_plan: None,
            conversation_scroll: None,
            slash_selected: None,
            slash_dismissed: false,
            tick_count: 0,
            toast: None,
            normal_mode_count: None,
            pane_prefix_active: false,
            last_context_compression: None,
            insert_escape_buffer: Vec::new(),
            hardware: crate::hardware::HardwareInfo::default(),
            hardware_poll_interval: 0,
        }
    }

    pub fn hydrate(&mut self, bootstrap: AppBootstrap) {
        self.session.transcript = bootstrap.transcript;
        self.session.selected_message = (!self.session.transcript.is_empty())
            .then_some(self.session.transcript.len().saturating_sub(1));

        self.session.branches = bootstrap.branches;
        self.session.selected_branch = self
            .session
            .branches
            .iter()
            .position(|branch| branch.is_active)
            .or_else(|| (!self.session.branches.is_empty()).then_some(0));
        self.session.runtime = RuntimePhase::Idle;
        self.message_edit = None;
        self.conversation_scroll = None;

        if let Some(status_line) = bootstrap.status_line {
            self.status_line = Some(status_line);
        }

        let draft = bootstrap.draft.unwrap_or_default();
        if !draft.text.is_empty() {
            self.focus = FocusTarget::Draft;
            self.input_mode = InputMode::Insert;
            self.sync_textarea_from_draft(&draft.text, draft.cursor);
        } else {
            self.focus = FocusTarget::Draft;
            self.input_mode = InputMode::Normal;
            self.textarea = new_themed_textarea();
        }
        self.draft = draft;
        self.command_palette.close();
        self.slash_selected = None;
        self.slash_dismissed = false;

        if let Some(screen) = bootstrap.screen {
            self.screen = screen;
        }

        self.session_metadata = bootstrap.session_metadata;
        self.session_stats = bootstrap.session_stats;
        self.context_preview = bootstrap.context_preview;
        self.context_dry_run = bootstrap.context_dry_run;
        self.recall_browser = bootstrap.recall_browser;
        if let Some(plan) = bootstrap.active_launch_plan {
            self.active_launch_plan = Some(plan);
        }
    }

    /// Transition from a menu screen into the conversation view for the current session.
    pub fn enter_conversation(&mut self) {
        self.screen = ScreenState::Conversation;
        self.focus = FocusTarget::Draft;
        self.input_mode = InputMode::Normal;
    }

    pub fn reset_for_new_conversation(&mut self) {
        self.session.transcript.clear();
        self.session.branches.clear();
        self.session.selected_message = None;
        self.session.selected_branch = None;
        self.session.runtime = RuntimePhase::Idle;
        self.message_edit = None;
        self.draft = DraftState::default();
        self.textarea = new_themed_textarea();
        self.conversation_scroll = None;
        self.session_metadata = None;
        self.session_stats = None;
        self.context_preview = None;
        self.context_dry_run = None;
        self.last_context_compression = None;
        self.recall_browser = None;
        self.command_palette.close();
        self.slash_selected = None;
        self.slash_dismissed = false;
    }

    /// Return to the main menu from any screen.
    pub fn return_to_menu(&mut self) {
        self.screen = ScreenState::MainMenu;
        self.input_mode = InputMode::Normal;
        self.focus = FocusTarget::Transcript;
    }

}
