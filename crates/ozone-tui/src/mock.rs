use std::{collections::BTreeMap, convert::Infallible};

use ozone_core::engine::CancelReason;

use crate::{
    app::{
        AppBootstrap, BranchItem, DraftCheckpoint, DraftState, GenerationPoll, RuntimeCancellation,
        RuntimeCompletion, RuntimeContextRefresh, RuntimeSendReceipt, SessionContext,
        SessionListEntry, TranscriptItem,
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
        _name: String,
        _system_prompt: String,
    ) -> Result<crate::app::CharacterEntry, Self::Error>;

    /// Import a character card from a JSON file path.
    fn import_character(
        &mut self,
        _path: String,
    ) -> Result<crate::app::CharacterEntry, Self::Error>;
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
    pub toggled_pinned_messages: Vec<String>,
    pub available_sessions: Vec<SessionListEntry>,
    pub available_characters: Vec<crate::app::CharacterEntry>,
    pub active_generation: Option<MockGeneration>,
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
            toggled_pinned_messages: Vec::new(),
            available_sessions: Vec::new(),
            available_characters: Vec::new(),
            active_generation: None,
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
        name: String,
        _system_prompt: String,
    ) -> Result<crate::app::CharacterEntry, Self::Error> {
        let entry = crate::app::CharacterEntry {
            card_id: format!("mock-char-{}", self.available_characters.len() + 1),
            name: name.clone(),
            description: String::new(),
            session_count: 0,
        };
        self.available_characters.push(entry.clone());
        Ok(entry)
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
}
