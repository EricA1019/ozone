/// Minimal runtime trait for TUI lifecycle management.
/// Chat-specific runtime traits were removed in v0.5.
/// This shim preserves compilation of non-chat TUI screens (launcher, monitor, profiling).

use ozone_core::engine::{ConversationMessage, SessionId};
use crate::state::GenerationPoll;

/// Trait for TUI runtime operations. Implemented by the backend layer.
pub trait SessionRuntime {
    type Error: std::error::Error;

    fn display_name(&self) -> &str;

    fn send_draft(
        &mut self,
        session: &SessionId,
        prompt: &str,
    ) -> Result<GenerationPoll, Self::Error>;

    fn poll_generation(
        &mut self,
        session: &SessionId,
    ) -> Result<Option<GenerationPoll>, Self::Error>;

    fn cancel_generation(&mut self);

    fn get_transcript(&self) -> &[ConversationMessage];

    fn reroll_last(&mut self, session: &SessionId) -> Result<(), Self::Error>;

    fn resend_last(&mut self, session: &SessionId) -> Result<(), Self::Error>;
}
