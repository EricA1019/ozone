use crate::state::*;
use crate::input::{InputMode, KeyAction};
use crate::layout::LayoutModel;
use crate::app::TextAreaSurface;
use crate::app::textareas::themed_textarea_from_text;
use crossterm::event::{MouseEvent, MouseEventKind};
use std::time::Instant;

impl ShellState {
    pub(crate) fn apply_send_receipt(&mut self, receipt: RuntimeSendReceipt) {
        self.session.runtime = RuntimePhase::Generating {
            request_id: receipt.request_id.clone(),
            prompt: receipt.user_message.content.clone(),
            partial_content: None,
        };
        self.push_transcript_item(receipt.user_message);
        self.status_line = Some(format!(
            "Generating response (request: {}…)",
            &receipt.request_id[..8.min(receipt.request_id.len())]
        ));
    }

    pub(crate) fn apply_context_refresh(&mut self, refresh: RuntimeContextRefresh) {
        self.session.runtime = RuntimePhase::Idle;

        if let Some(session_title) = refresh.session_title {
            self.session.context.title = session_title;
        }

        if let Some(transcript) = refresh.transcript {
            let previous_selected_index = self.session.selected_message;
            let previous_selected_message_id = previous_selected_index
                .and_then(|index| self.session.transcript.get(index))
                .and_then(|item| item.message_id.clone());

            self.session.transcript = transcript;
            self.session.selected_message = previous_selected_message_id
                .as_deref()
                .and_then(|message_id| {
                    self.session
                        .transcript
                        .iter()
                        .position(|item| item.message_id.as_deref() == Some(message_id))
                })
                .or_else(|| {
                    previous_selected_index
                        .filter(|index| *index < self.session.transcript.len())
                })
                .or_else(|| {
                    (!self.session.transcript.is_empty())
                        .then_some(self.session.transcript.len().saturating_sub(1))
                });
        }

        if let Some(session_metadata) = refresh.session_metadata {
            self.session_metadata = Some(session_metadata);
        }

        if let Some(session_stats) = refresh.session_stats {
            self.session_stats = Some(session_stats);
        }

        if let Some(context_preview) = refresh.context_preview {
            self.context_preview = Some(context_preview);
        }

        if let Some(context_dry_run) = refresh.context_dry_run {
            self.context_dry_run = Some(context_dry_run);
        }

        if let Some(recall_browser) = refresh.recall_browser {
            self.recall_browser = Some(recall_browser);
        }

        self.status_line = Some(refresh.status_line.unwrap_or_else(|| "Context refreshed".into()));
    }

    pub(crate) fn apply_runtime_cancellation(&mut self, cancellation: RuntimeCancellation) {
        let had_partial_assistant_message = cancellation.partial_assistant_message.is_some();
        if let Some(partial_assistant_message) = cancellation.partial_assistant_message {
            self.push_transcript_item(partial_assistant_message);
        }

        self.session.runtime = RuntimePhase::Cancelled {
            request_id: None,
            prompt: self.session.runtime.prompt().unwrap_or_default().to_string(),
            reason: cancellation.reason,
        };
        self.status_line = Some(if had_partial_assistant_message {
            "Generation cancelled; partial reply kept locally".into()
        } else {
            "Generation cancelled".into()
        });
    }

    pub(crate) fn apply_runtime_completion(&mut self, completion: RuntimeCompletion) {
        self.push_transcript_item(completion.message.clone());
        self.session.runtime = RuntimePhase::Idle;
        self.status_line = Some("Generation completed".into());
    }

    pub(crate) fn apply_runtime_failure(&mut self, failure: RuntimeFailure) {
        let prompt = failure.prompt.clone();
        self.session.runtime = RuntimePhase::Failed {
            request_id: Some(failure.request_id.clone()),
            prompt: prompt.clone(),
            message: failure.message.clone(),
            reason: failure.reason.clone(),
        };
        self.replace_draft(DraftState::with_text(prompt.clone()));
        self.textarea = themed_textarea_from_text(
            TextAreaSurface::Composer,
            &prompt,
            prompt.chars().count(),
        );
        self.focus = FocusTarget::Draft;
        self.input_mode = InputMode::Insert;
        self.status_line = Some(format!("Generation failed: {}", failure.message));
    }

    pub(crate) fn apply_runtime_progress(&mut self, progress: RuntimeProgress) {
        if let RuntimePhase::Generating { partial_content, .. } = &mut self.session.runtime {
            *partial_content = Some(progress.content.clone());
        }
    }

    /// Handle mouse events for the TUI.
    pub(crate) fn handle_mouse_event(&mut self, mouse: MouseEvent, layout: &LayoutModel) -> KeyAction {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.scroll_conversation(layout, 1);
                KeyAction::Noop
            }
            MouseEventKind::ScrollDown => {
                self.scroll_conversation(layout, -1);
                KeyAction::Noop
            }
            _ => KeyAction::Noop,
        }
    }

    /// Sync textarea from draft state.
    pub(crate) fn sync_textarea_from_draft(&mut self, text: &str, cursor: usize) {
        let surface = if self.message_edit.is_some() {
            TextAreaSurface::MessageEdit
        } else {
            TextAreaSurface::Composer
        };
        self.textarea = themed_textarea_from_text(surface, text, cursor);
    }

    /// Replace the current draft with a new one.
    pub(crate) fn replace_draft(&mut self, draft: DraftState) {
        self.draft = draft;
    }

    /// Get the currently active toast message.
    pub(crate) fn active_toast(&self) -> Option<&(String, Instant)> {
        self.toast.as_ref()
    }

    /// Show a toast notification.
    pub(crate) fn show_toast(&mut self, message: &str) {
        self.toast = Some((message.to_string(), Instant::now()));
    }
}
