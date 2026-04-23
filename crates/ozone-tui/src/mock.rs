use std::{collections::BTreeMap, convert::Infallible};

use ozone_core::engine::CancelReason;

use crate::{
    app::{
        AppBootstrap, BranchItem, DraftCheckpoint, DraftState, GenerationPoll, RuntimeCancellation,
        RuntimeCompletion, RuntimeContextRefresh, RuntimeSendReceipt, RuntimeSessionLoad,
        SessionContext, SessionListEntry, SessionMetadata, TranscriptItem,
    },
    input::KeyAction,
};

pub trait SessionRuntime {
    type Error: std::fmt::Debug;

    fn bootstrap(&mut self, context: &SessionContext) -> Result<AppBootstrap, Self::Error>;

    fn dispatch(
        &mut self,
        _context: &SessionContext,
        _action: KeyAction,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn send_draft(
        &mut self,
        _context: &SessionContext,
        _prompt: &str,
    ) -> Result<Option<RuntimeSendReceipt>, Self::Error> {
        Ok(None)
    }

    /// Poll the runtime for generation progress. Called on every event-loop
    /// tick while the shell is in `RuntimePhase::Generating`.
    ///
    /// - Return `Some(GenerationPoll::Pending { .. })` while still running.
    /// - Return `Some(GenerationPoll::Completed(...))` when done.
    /// - Return `Some(GenerationPoll::Failed(...))` on unrecoverable error.
    /// - Return `None` when no generation is active (idempotent, safe to call).
    ///
    /// The default implementation delegates to `complete_generation` for
    /// backward compatibility with runtimes that implemented the Phase 1C
    /// timer-based interface.
    fn poll_generation(
        &mut self,
        context: &SessionContext,
    ) -> Result<Option<GenerationPoll>, Self::Error> {
        Ok(self
            .complete_generation(context)?
            .map(GenerationPoll::Completed))
    }

    /// Legacy single-shot completion hook. Prefer `poll_generation` for new
    /// implementations; this method is retained so existing runtimes that only
    /// implement it still work via the `poll_generation` default.
    fn complete_generation(
        &mut self,
        _context: &SessionContext,
    ) -> Result<Option<RuntimeCompletion>, Self::Error> {
        Ok(None)
    }

    fn cancel_generation(
        &mut self,
        _context: &SessionContext,
    ) -> Result<Option<RuntimeCancellation>, Self::Error> {
        Ok(None)
    }

    fn build_context_dry_run(
        &mut self,
        _context: &SessionContext,
    ) -> Result<Option<RuntimeContextRefresh>, Self::Error> {
        Ok(None)
    }

    fn toggle_bookmark(
        &mut self,
        _context: &SessionContext,
        _message_id: &str,
    ) -> Result<Option<RuntimeContextRefresh>, Self::Error> {
        Ok(None)
    }

    fn toggle_pinned_memory(
        &mut self,
        _context: &SessionContext,
        _message_id: &str,
    ) -> Result<Option<RuntimeContextRefresh>, Self::Error> {
        Ok(None)
    }

    fn run_command(
        &mut self,
        _context: &SessionContext,
        _input: &str,
    ) -> Result<Option<RuntimeContextRefresh>, Self::Error> {
        Ok(None)
    }

    fn edit_message(
        &mut self,
        _context: &SessionContext,
        _message_id: &str,
        _content: &str,
    ) -> Result<Option<RuntimeContextRefresh>, Self::Error> {
        Ok(None)
    }

    fn persist_draft(
        &mut self,
        _context: &SessionContext,
        _draft: Option<&str>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// List available sessions for the session browser.
    /// Returns an empty list by default (runtimes that don't support session
    /// listing can use the default).
    fn list_sessions(&mut self) -> Result<Vec<SessionListEntry>, Self::Error> {
        Ok(Vec::new())
    }

    /// List imported character cards for the character manager.
    /// Returns an empty list by default.
    fn list_characters(&mut self) -> Result<Vec<crate::app::CharacterEntry>, Self::Error> {
        Ok(Vec::new())
    }

    /// Return current configuration entries for the settings screen.
    fn get_settings(&mut self) -> Result<Vec<crate::app::SettingsEntry>, Self::Error> {
        Ok(Vec::new())
    }

    /// Create a new character card in the global library.
    fn create_character(
        &mut self,
        _detail: crate::app::CharacterDetail,
    ) -> Result<crate::app::CharacterEntry, Self::Error>;

    /// Update an existing character card.
    fn update_character(
        &mut self,
        _detail: crate::app::CharacterDetail,
    ) -> Result<crate::app::CharacterEntry, Self::Error>;

    /// Load a character card by ID for editing.
    fn get_character(
        &mut self,
        _card_id: &str,
    ) -> Result<Option<crate::app::CharacterDetail>, Self::Error>;

    /// Import a character card from a JSON file path.
    fn import_character(
        &mut self,
        _path: String,
    ) -> Result<crate::app::CharacterEntry, Self::Error>;

    /// Persist a changed preference value.
    /// `pref_key` is the JSON field name (e.g. `"theme_preset"`); `value` is
    /// the new serialised string value.  The default implementation is a no-op
    /// so that runtimes that don't manage prefs don't need to implement this.
    fn save_pref(&mut self, _pref_key: &str, _value: &str) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Assign or remove the folder for a session.
    /// The default implementation is a no-op.
    fn set_session_folder(
        &mut self,
        _session_id: &str,
        _folder: Option<&str>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Create and switch into a fresh session.
    fn create_session(
        &mut self,
        character_name: Option<&str>,
    ) -> Result<RuntimeSessionLoad, Self::Error>;

    /// Switch to a different session — release the current lock, open the new
    /// session, and return its bootstrap data so the TUI can hydrate.
    /// The default returns `None` (session switching not supported).
    fn open_session(
        &mut self,
        _session_id: &str,
    ) -> Result<Option<RuntimeSessionLoad>, Self::Error> {
        Ok(None)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockGeneration {
    pub request_id: String,
    pub prompt: String,
    pub response: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockRuntime {
    pub bootstrap_state: AppBootstrap,
    pub bootstrapped_sessions: Vec<String>,
    pub dispatched_actions: Vec<KeyAction>,
    pub sent_prompts: Vec<String>,
    pub completed_requests: Vec<String>,
    pub cancelled_requests: Vec<String>,
    pub polled_requests: Vec<String>,
    pub persisted_drafts: BTreeMap<String, String>,
    pub edited_messages: Vec<(String, String)>,
    pub toggled_pinned_messages: Vec<String>,
    pub available_sessions: Vec<SessionListEntry>,
    pub available_characters: Vec<crate::app::CharacterEntry>,
    pub active_generation: Option<MockGeneration>,
    /// Per-session bootstrap data for `open_session()` testing.
    pub session_bootstraps: BTreeMap<String, AppBootstrap>,
    next_request_number: u64,
}

impl Default for MockRuntime {
    fn default() -> Self {
        Self {
            bootstrap_state: AppBootstrap::default(),
            bootstrapped_sessions: Vec::new(),
            dispatched_actions: Vec::new(),
            sent_prompts: Vec::new(),
            completed_requests: Vec::new(),
            cancelled_requests: Vec::new(),
            polled_requests: Vec::new(),
            persisted_drafts: BTreeMap::new(),
            edited_messages: Vec::new(),
            toggled_pinned_messages: Vec::new(),
            available_sessions: Vec::new(),
            available_characters: Vec::new(),
            active_generation: None,
            session_bootstraps: BTreeMap::new(),
            next_request_number: 1,
        }
    }
}

impl MockRuntime {
    pub fn with_bootstrap(bootstrap_state: AppBootstrap) -> Self {
        Self {
            bootstrap_state,
            ..Self::default()
        }
    }

    pub fn seeded() -> Self {
        Self::with_bootstrap(AppBootstrap {
            transcript: vec![TranscriptItem::new("user", "mock session bootstrap")],
            branches: vec![BranchItem::new("main", "main", true)],
            status_line: Some("mock runtime ready".into()),
            draft: Some(DraftState::default()),
            screen: None,
            session_metadata: None,
            session_stats: None,
            context_preview: None,
            context_dry_run: None,
            recall_browser: None,
            active_launch_plan: None,
        })
    }

    fn next_request_id(&mut self) -> String {
        let request_id = format!("mock-request-{}", self.next_request_number);
        self.next_request_number += 1;
        request_id
    }
}

impl SessionRuntime for MockRuntime {
    type Error = Infallible;

    fn bootstrap(&mut self, context: &SessionContext) -> Result<AppBootstrap, Self::Error> {
        self.bootstrapped_sessions
            .push(context.session_id.to_string());
        let mut bootstrap = self.bootstrap_state.clone();

        let needs_restored_draft = bootstrap
            .draft
            .as_ref()
            .map(|draft| draft.text.is_empty())
            .unwrap_or(true);

        if needs_restored_draft {
            if let Some(text) = self.persisted_drafts.get(context.session_id.as_str()) {
                bootstrap.draft = Some(DraftState::restore(DraftCheckpoint::new(
                    text.clone(),
                    text.chars().count(),
                )));
            }
        }

        Ok(bootstrap)
    }

    fn dispatch(
        &mut self,
        _context: &SessionContext,
        action: KeyAction,
    ) -> Result<(), Self::Error> {
        self.dispatched_actions.push(action);
        Ok(())
    }

    fn send_draft(
        &mut self,
        _context: &SessionContext,
        prompt: &str,
    ) -> Result<Option<RuntimeSendReceipt>, Self::Error> {
        if prompt.trim().is_empty() {
            return Ok(None);
        }

        let request_id = self.next_request_id();
        let prompt = prompt.to_owned();
        let user_message = TranscriptItem::new("user", prompt.clone());
        self.bootstrap_state.transcript.push(user_message.clone());
        self.sent_prompts.push(prompt.clone());
        self.active_generation = Some(MockGeneration {
            request_id: request_id.clone(),
            prompt: prompt.clone(),
            response: format!("Mock response to: {prompt}"),
        });

        Ok(Some(RuntimeSendReceipt {
            request_id,
            user_message,
            context_preview: None,
            context_dry_run: None,
        }))
    }

    fn complete_generation(
        &mut self,
        _context: &SessionContext,
    ) -> Result<Option<RuntimeCompletion>, Self::Error> {
        let generation = match self.active_generation.take() {
            Some(generation) => generation,
            None => return Ok(None),
        };

        self.completed_requests.push(generation.request_id.clone());
        let assistant_message = TranscriptItem::new("assistant", generation.response);
        self.bootstrap_state
            .transcript
            .push(assistant_message.clone());

        Ok(Some(RuntimeCompletion {
            request_id: generation.request_id,
            assistant_message,
            session_title: None,
        }))
    }

    /// Overrides the default to record polling activity and immediately return
    /// `Completed` — the mock has no real async work so it completes on the
    /// first poll rather than waiting for an external timer.
    fn poll_generation(
        &mut self,
        context: &SessionContext,
    ) -> Result<Option<GenerationPoll>, Self::Error> {
        if let Some(ref gen) = self.active_generation {
            self.polled_requests.push(gen.request_id.clone());
        }
        Ok(self
            .complete_generation(context)?
            .map(GenerationPoll::Completed))
    }

    fn cancel_generation(
        &mut self,
        _context: &SessionContext,
    ) -> Result<Option<RuntimeCancellation>, Self::Error> {
        let generation = match self.active_generation.take() {
            Some(generation) => generation,
            None => return Ok(None),
        };

        self.cancelled_requests.push(generation.request_id.clone());

        Ok(Some(RuntimeCancellation {
            request_id: generation.request_id,
            reason: CancelReason::UserRequested,
            partial_assistant_message: Some(TranscriptItem::new(
                "assistant",
                format!("Partial mock response for: {}", generation.prompt),
            )),
        }))
    }

    fn persist_draft(
        &mut self,
        context: &SessionContext,
        draft: Option<&str>,
    ) -> Result<(), Self::Error> {
        match draft {
            Some(text) if !text.is_empty() => {
                self.persisted_drafts
                    .insert(context.session_id.to_string(), text.to_owned());
            }
            _ => {
                self.persisted_drafts.remove(context.session_id.as_str());
            }
        }

        Ok(())
    }

    fn edit_message(
        &mut self,
        _context: &SessionContext,
        message_id: &str,
        content: &str,
    ) -> Result<Option<RuntimeContextRefresh>, Self::Error> {
        self.edited_messages
            .push((message_id.to_owned(), content.to_owned()));
        if let Some(item) = self
            .bootstrap_state
            .transcript
            .iter_mut()
            .find(|item| item.message_id.as_deref() == Some(message_id))
        {
            item.content = content.to_owned();
        }
        Ok(Some(RuntimeContextRefresh {
            transcript: Some(self.bootstrap_state.transcript.clone()),
            status_line: Some("Updated selected message".into()),
            ..RuntimeContextRefresh::default()
        }))
    }

    fn toggle_pinned_memory(
        &mut self,
        _context: &SessionContext,
        message_id: &str,
    ) -> Result<Option<RuntimeContextRefresh>, Self::Error> {
        self.toggled_pinned_messages.push(message_id.to_owned());
        Ok(None)
    }

    fn list_sessions(&mut self) -> Result<Vec<SessionListEntry>, Self::Error> {
        Ok(self.available_sessions.clone())
    }

    fn list_characters(&mut self) -> Result<Vec<crate::app::CharacterEntry>, Self::Error> {
        Ok(self.available_characters.clone())
    }

    fn create_character(
        &mut self,
        detail: crate::app::CharacterDetail,
    ) -> Result<crate::app::CharacterEntry, Self::Error> {
        let entry = crate::app::CharacterEntry {
            card_id: format!("mock-char-{}", self.available_characters.len() + 1),
            name: detail.name.clone(),
            description: String::new(),
            session_count: 0,
        };
        self.available_characters.push(entry.clone());
        Ok(entry)
    }

    fn update_character(
        &mut self,
        detail: crate::app::CharacterDetail,
    ) -> Result<crate::app::CharacterEntry, Self::Error> {
        if let Some(entry) = self
            .available_characters
            .iter_mut()
            .find(|e| e.card_id == detail.card_id)
        {
            entry.name = detail.name.clone();
            entry.description = detail.description.clone();
        }
        Ok(crate::app::CharacterEntry {
            card_id: detail.card_id,
            name: detail.name,
            description: detail.description,
            session_count: 0,
        })
    }

    fn get_character(
        &mut self,
        card_id: &str,
    ) -> Result<Option<crate::app::CharacterDetail>, Self::Error> {
        let entry = self
            .available_characters
            .iter()
            .find(|e| e.card_id == card_id);
        Ok(entry.map(|e| crate::app::CharacterDetail {
            card_id: e.card_id.clone(),
            name: e.name.clone(),
            description: e.description.clone(),
            ..Default::default()
        }))
    }

    fn import_character(
        &mut self,
        _path: String,
    ) -> Result<crate::app::CharacterEntry, Self::Error> {
        let entry = crate::app::CharacterEntry {
            card_id: format!("mock-import-{}", self.available_characters.len() + 1),
            name: "Imported Character".into(),
            description: "Imported from file".into(),
            session_count: 0,
        };
        self.available_characters.push(entry.clone());
        Ok(entry)
    }

    fn create_session(
        &mut self,
        character_name: Option<&str>,
    ) -> Result<RuntimeSessionLoad, Self::Error> {
        let session_number = self.available_sessions.len() + self.session_bootstraps.len() + 1;
        let session_id = format!("00000000-0000-0000-0000-{session_number:012}");
        let session_name = format!("New Conversation {session_number}");
        let character_name = character_name.map(str::to_owned);
        let bootstrap = AppBootstrap {
            status_line: Some("New conversation started".into()),
            session_metadata: Some(SessionMetadata {
                character_name: character_name.clone(),
                tags: Vec::new(),
            }),
            ..AppBootstrap::default()
        };
        self.available_sessions.push(SessionListEntry {
            session_id: session_id.clone(),
            name: session_name.clone(),
            character_name,
            message_count: 0,
            last_active: None,
            folder: None,
        });
        self.session_bootstraps
            .insert(session_id.clone(), bootstrap.clone());
        Ok(RuntimeSessionLoad {
            session_id,
            session_name,
            bootstrap,
        })
    }

    fn open_session(
        &mut self,
        session_id: &str,
    ) -> Result<Option<RuntimeSessionLoad>, Self::Error> {
        if let Some(bootstrap) = self.session_bootstraps.get(session_id) {
            let session_name = self
                .available_sessions
                .iter()
                .find(|entry| entry.session_id == session_id)
                .map(|entry| entry.name.clone())
                .unwrap_or_else(|| "Mock Session".into());
            Ok(Some(RuntimeSessionLoad {
                session_id: session_id.to_owned(),
                session_name,
                bootstrap: bootstrap.clone(),
            }))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use ozone_core::{engine::CancelReason, session::SessionId};

    use super::{MockRuntime, SessionRuntime};
    use crate::app::{DraftState, GenerationPoll, SessionContext, TranscriptItem};

    fn session_context() -> SessionContext {
        let session_id = SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap();
        SessionContext::new(session_id, "Phase 1C")
    }

    #[test]
    fn send_draft_starts_mock_generation_and_completion_commits_reply() {
        let context = session_context();
        let mut runtime = MockRuntime::seeded();

        let receipt = runtime.send_draft(&context, "hello mock").unwrap().unwrap();
        assert_eq!(receipt.request_id, "mock-request-1");
        assert_eq!(
            receipt.user_message,
            TranscriptItem::new("user", "hello mock")
        );
        assert_eq!(runtime.sent_prompts, vec!["hello mock".to_string()]);
        assert_eq!(
            runtime
                .active_generation
                .as_ref()
                .map(|generation| generation.prompt.as_str()),
            Some("hello mock")
        );

        let completion = runtime.complete_generation(&context).unwrap().unwrap();
        assert_eq!(completion.request_id, "mock-request-1");
        assert_eq!(
            completion.assistant_message,
            TranscriptItem::new("assistant", "Mock response to: hello mock")
        );
        assert!(runtime.active_generation.is_none());
        assert_eq!(
            runtime.completed_requests,
            vec!["mock-request-1".to_string()]
        );
    }

    #[test]
    fn cancel_generation_returns_user_requested_without_committing_assistant_reply() {
        let context = session_context();
        let mut runtime = MockRuntime::seeded();

        runtime.send_draft(&context, "cancel me").unwrap();
        let cancellation = runtime.cancel_generation(&context).unwrap().unwrap();

        assert_eq!(cancellation.request_id, "mock-request-1");
        assert_eq!(cancellation.reason, CancelReason::UserRequested);
        assert_eq!(
            cancellation.partial_assistant_message,
            Some(TranscriptItem::new(
                "assistant",
                "Partial mock response for: cancel me"
            ))
        );
        assert!(runtime.active_generation.is_none());
        assert_eq!(
            runtime.cancelled_requests,
            vec!["mock-request-1".to_string()]
        );
        assert_eq!(
            runtime.bootstrap_state.transcript.last(),
            Some(&TranscriptItem::new("user", "cancel me"))
        );
    }

    #[test]
    fn persisted_draft_is_restored_during_bootstrap() {
        let context = session_context();
        let mut runtime = MockRuntime::seeded();

        runtime
            .persist_draft(&context, Some("draft from persistence"))
            .unwrap();

        let bootstrap = runtime.bootstrap(&context).unwrap();

        assert_eq!(
            bootstrap.draft,
            Some(DraftState::restore(crate::app::DraftCheckpoint::new(
                "draft from persistence",
                "draft from persistence".chars().count()
            )))
        );
    }

    #[test]
    fn poll_generation_completes_immediately_and_records_poll() {
        let context = session_context();
        let mut runtime = MockRuntime::seeded();

        runtime.send_draft(&context, "poll me").unwrap();
        assert_eq!(runtime.polled_requests, Vec::<String>::new());

        let poll = runtime.poll_generation(&context).unwrap().unwrap();
        assert!(
            matches!(poll, GenerationPoll::Completed(ref c) if c.request_id == "mock-request-1")
        );
        assert_eq!(runtime.polled_requests, vec!["mock-request-1".to_string()]);
        assert!(runtime.active_generation.is_none());
        assert_eq!(
            runtime.completed_requests,
            vec!["mock-request-1".to_string()]
        );
    }

    #[test]
    fn poll_generation_returns_none_when_idle() {
        let context = session_context();
        let mut runtime = MockRuntime::seeded();

        let poll = runtime.poll_generation(&context).unwrap();
        assert!(poll.is_none());
        assert!(runtime.polled_requests.is_empty());
    }

    #[test]
    fn poll_generation_does_not_fire_after_cancel() {
        let context = session_context();
        let mut runtime = MockRuntime::seeded();

        runtime.send_draft(&context, "cancel before poll").unwrap();
        runtime.cancel_generation(&context).unwrap();

        let poll = runtime.poll_generation(&context).unwrap();
        assert!(poll.is_none());
        assert!(runtime.polled_requests.is_empty());
    }

    #[test]
    fn open_session_returns_registered_bootstrap() {
        use crate::app::{AppBootstrap, BranchItem};

        let mut runtime = MockRuntime::seeded();
        let other_bootstrap = AppBootstrap {
            transcript: vec![
                TranscriptItem::new("user", "hello from other session"),
                TranscriptItem::new("assistant", "hi there"),
            ],
            branches: vec![BranchItem::new("main", "main", true)],
            status_line: Some("other session ready".into()),
            ..AppBootstrap::default()
        };
        runtime
            .session_bootstraps
            .insert("other-session-id".into(), other_bootstrap);

        let result = runtime.open_session("other-session-id").unwrap();
        assert!(result.is_some());
        let session = result.unwrap();
        assert_eq!(session.session_id, "other-session-id");
        assert_eq!(session.bootstrap.transcript.len(), 2);
        assert_eq!(
            session.bootstrap.transcript[0].content,
            "hello from other session"
        );

        // Unknown session returns None.
        let result = runtime.open_session("unknown-id").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn create_session_registers_a_fresh_empty_bootstrap() {
        let mut runtime = MockRuntime::seeded();

        let session = runtime.create_session(None).unwrap();

        assert!(session.session_id.starts_with("00000000-0000-0000-0000-"));
        assert_eq!(session.bootstrap.transcript.len(), 0);
        assert_eq!(
            runtime
                .available_sessions
                .last()
                .map(|entry| entry.session_id.as_str()),
            Some(session.session_id.as_str())
        );
        assert!(runtime.session_bootstraps.contains_key(&session.session_id));
    }

    #[test]
    fn create_session_carries_requested_character_name() {
        let mut runtime = MockRuntime::seeded();

        let session = runtime.create_session(Some("Aster")).unwrap();

        assert_eq!(
            runtime
                .available_sessions
                .last()
                .and_then(|entry| entry.character_name.as_deref()),
            Some("Aster")
        );
        assert_eq!(
            session
                .bootstrap
                .session_metadata
                .as_ref()
                .and_then(|metadata| metadata.character_name.as_deref()),
            Some("Aster")
        );
    }
}
