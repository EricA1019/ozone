use std::fs;

use ozone_persist::{CreateSessionRequest, SessionId};
use ozone_tui::{CharacterDetail, CharacterEntry, EntryKind, SessionListEntry};

use crate::{
    cli::prefs::{load_prefs_sync, save_prefs_sync},
    session_title,
};

use super::{OzonePlusRuntime, TuiRuntimeSessionLoad};

impl OzonePlusRuntime {
    pub(super) fn list_sessions_impl(&mut self) -> Result<Vec<SessionListEntry>, String> {
        let sessions = self.repo.list_sessions().map_err(|e| e.to_string())?;
        Ok(sessions
            .into_iter()
            .map(|s| SessionListEntry {
                session_id: s.session_id.to_string(),
                name: s.name.clone(),
                character_name: s.character_name.clone(),
                message_count: s.message_count as usize,
                last_active: Some(crate::format_timestamp_short(s.last_opened_at)),
                folder: s.folder().map(|f| f.to_owned()),
                last_message_preview: None,
            })
            .collect())
    }

    pub(super) fn get_settings_impl(&mut self) -> Result<Vec<ozone_tui::SettingsEntry>, String> {
        let config = self.inference.config();
        let prefs = load_prefs_sync()?;
        let mut entries = Vec::new();

        entries.push(ozone_tui::SettingsEntry {
            category: "Session".into(),
            key: "Session ID".into(),
            value: self.session_id.to_string(),
            kind: EntryKind::ReadOnly,
            pref_key: String::new(),
        });
        entries.push(ozone_tui::SettingsEntry {
            category: "Session".into(),
            key: "Lock instance".into(),
            value: self.lock_instance_id.clone(),
            kind: EntryKind::ReadOnly,
            pref_key: String::new(),
        });

        entries.push(ozone_tui::SettingsEntry {
            category: "Backend".into(),
            key: "Type".into(),
            value: config.backend.r#type.clone(),
            kind: EntryKind::ReadOnly,
            pref_key: String::new(),
        });
        entries.push(ozone_tui::SettingsEntry {
            category: "Backend".into(),
            key: "URL".into(),
            value: config.backend.url.clone(),
            kind: EntryKind::ReadOnly,
            pref_key: String::new(),
        });
        entries.push(ozone_tui::SettingsEntry {
            category: "Backend".into(),
            key: "Prompt template".into(),
            value: self.inference.selected_template().to_string(),
            kind: EntryKind::ReadOnly,
            pref_key: String::new(),
        });

        entries.push(ozone_tui::SettingsEntry {
            category: "Model".into(),
            key: "Max tokens".into(),
            value: config.context.max_tokens.to_string(),
            kind: EntryKind::ReadOnly,
            pref_key: String::new(),
        });
        entries.push(ozone_tui::SettingsEntry {
            category: "Model".into(),
            key: "Safety margin".into(),
            value: format!("{}%", config.context.safety_margin_pct),
            kind: EntryKind::ReadOnly,
            pref_key: String::new(),
        });

        let ts_options = vec![
            "relative".to_string(),
            "absolute".to_string(),
            "off".to_string(),
        ];
        let ts_cur = ts_options
            .iter()
            .position(|o| o == &prefs.timestamp_style)
            .unwrap_or(0);
        entries.push(ozone_tui::SettingsEntry {
            category: "Display".into(),
            key: "Timestamp style".into(),
            value: String::new(),
            kind: EntryKind::Cycle {
                options: ts_options,
                current: ts_cur,
            },
            pref_key: "timestamp_style".into(),
        });

        let density_options = vec!["comfortable".to_string(), "compact".to_string()];
        let density_cur = density_options
            .iter()
            .position(|o| o == &prefs.message_density)
            .unwrap_or(0);
        entries.push(ozone_tui::SettingsEntry {
            category: "Display".into(),
            key: "Message density".into(),
            value: String::new(),
            kind: EntryKind::Cycle {
                options: density_options,
                current: density_cur,
            },
            pref_key: "message_density".into(),
        });

        let theme_options = vec![
            "dark-mint".to_string(),
            "ozone-dark".to_string(),
            "high-contrast".to_string(),
        ];
        let theme_cur = theme_options
            .iter()
            .position(|o| o == &prefs.theme_preset)
            .unwrap_or(0);
        entries.push(ozone_tui::SettingsEntry {
            category: "Appearance".into(),
            key: "Theme".into(),
            value: String::new(),
            kind: EntryKind::Cycle {
                options: theme_options,
                current: theme_cur,
            },
            pref_key: "theme_preset".into(),
        });

        entries.push(ozone_tui::SettingsEntry {
            category: "Launch".into(),
            key: "Side-by-side monitor".into(),
            value: String::new(),
            kind: EntryKind::Toggle(prefs.side_by_side_monitor),
            pref_key: "side_by_side_monitor".into(),
        });
        entries.push(ozone_tui::SettingsEntry {
            category: "Launch".into(),
            key: "Inspector on start".into(),
            value: String::new(),
            kind: EntryKind::Toggle(prefs.show_inspector),
            pref_key: "show_inspector".into(),
        });

        Ok(entries)
    }

    pub(super) fn save_pref_impl(&mut self, pref_key: &str, value: &str) -> Result<(), String> {
        let mut prefs = load_prefs_sync()?;
        match pref_key {
            "theme_preset" => prefs.theme_preset = value.to_string(),
            "timestamp_style" => prefs.timestamp_style = value.to_string(),
            "message_density" => prefs.message_density = value.to_string(),
            "side_by_side_monitor" => {
                prefs.side_by_side_monitor = value.parse::<bool>().unwrap_or(false)
            }
            "show_inspector" => prefs.show_inspector = value.parse::<bool>().unwrap_or(false),
            _ => {}
        }
        save_prefs_sync(&prefs)
    }

    pub(super) fn set_session_folder_impl(
        &mut self,
        session_id: &str,
        folder: Option<&str>,
    ) -> Result<(), String> {
        let sid = SessionId::parse(session_id).map_err(|e| e.to_string())?;
        self.repo
            .set_session_folder(&sid, folder)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub(super) fn list_characters_impl(&mut self) -> Result<Vec<CharacterEntry>, String> {
        let chars = self
            .repo
            .list_characters_global()
            .map_err(|e| e.to_string())?;
        Ok(chars
            .into_iter()
            .map(|c| CharacterEntry {
                card_id: c.card_id,
                name: c.name,
                description: c.description,
                greeting: c.greeting,
                session_count: 0,
            })
            .collect())
    }

    pub(super) fn create_character_impl(
        &mut self,
        detail: CharacterDetail,
    ) -> Result<CharacterEntry, String> {
        let stored = self
            .repo
            .create_character_full(
                &detail.name,
                &detail.description,
                &detail.system_prompt,
                &detail.personality,
                &detail.scenario,
                &detail.greeting,
                &detail.example_dialogue,
            )
            .map_err(|e| e.to_string())?;
        Ok(CharacterEntry {
            card_id: stored.card_id,
            name: stored.name,
            description: stored.description,
            greeting: stored.greeting,
            session_count: 0,
        })
    }

    pub(super) fn update_character_impl(
        &mut self,
        detail: CharacterDetail,
    ) -> Result<CharacterEntry, String> {
        let stored = self
            .repo
            .update_character(
                &detail.card_id,
                &detail.name,
                &detail.description,
                &detail.system_prompt,
                &detail.personality,
                &detail.scenario,
                &detail.greeting,
                &detail.example_dialogue,
            )
            .map_err(|e| e.to_string())?;
        Ok(CharacterEntry {
            card_id: stored.card_id,
            name: stored.name,
            description: stored.description,
            greeting: stored.greeting,
            session_count: 0,
        })
    }

    pub(super) fn get_character_impl(
        &mut self,
        card_id: &str,
    ) -> Result<Option<CharacterDetail>, String> {
        let stored = self
            .repo
            .get_character(card_id)
            .map_err(|e| e.to_string())?;
        Ok(stored.map(|s| CharacterDetail {
            card_id: s.card_id,
            name: s.name,
            description: s.description,
            system_prompt: s.system_prompt,
            personality: s.personality,
            scenario: s.scenario,
            greeting: s.greeting,
            example_dialogue: s.example_dialogue,
        }))
    }

    pub(super) fn import_character_impl(&mut self, path: String) -> Result<CharacterEntry, String> {
        let contents = fs::read_to_string(&path).map_err(|e| format!("failed to read {path}: {e}"))?;
        let card = ozone_persist::CharacterCard::from_json_str(&contents).map_err(|e| e.to_string())?;
        let stored = self
            .repo
            .create_character_full(
                &card.name,
                card.description.as_deref().unwrap_or(""),
                card.system_prompt.as_deref().unwrap_or(""),
                card.personality.as_deref().unwrap_or(""),
                card.scenario.as_deref().unwrap_or(""),
                card.greeting.as_deref().unwrap_or(""),
                card.example_dialogue.as_deref().unwrap_or(""),
            )
            .map_err(|e| e.to_string())?;
        Ok(CharacterEntry {
            card_id: stored.card_id,
            name: stored.name,
            description: stored.description,
            greeting: stored.greeting,
            session_count: 0,
        })
    }

    pub(super) fn create_session_impl(
        &mut self,
        character_name: Option<&str>,
    ) -> Result<TuiRuntimeSessionLoad, String> {
        let mut request = CreateSessionRequest::new(session_title::DEFAULT_SESSION_TITLE);
        request.character_name = character_name
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let session = self
            .repo
            .create_session(request)
            .map_err(|error| error.to_string())?;
        self.load_session_into_tui(session.session_id)
    }

    pub(super) fn open_session_impl(
        &mut self,
        session_id: &str,
    ) -> Result<Option<TuiRuntimeSessionLoad>, String> {
        let new_sid = SessionId::parse(session_id).map_err(|e| e.to_string())?;
        if self
            .repo
            .get_session(&new_sid)
            .map_err(|error| error.to_string())?
            .is_none()
        {
            return Ok(None);
        }
        Ok(Some(self.load_session_into_tui(new_sid)?))
    }
}