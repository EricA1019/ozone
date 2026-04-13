use crate::inference_adapter::{InferenceAdapter, TranscriptTurn};
use ozone_core::engine::ConversationMessage;
use ozone_core::session::UnixTimestamp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextPlanSource {
    #[allow(dead_code)]
    EnginePlan,
    TranscriptFallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextTokenBudgetPreview {
    pub used_tokens: u32,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextPlanPreview {
    pub source: ContextPlanSource,
    pub summary: String,
    pub lines: Vec<String>,
    pub selected_items: Option<usize>,
    pub omitted_items: Option<usize>,
    pub token_budget: Option<ContextTokenBudgetPreview>,
}

impl ContextPlanPreview {
    fn status_line_preview_text(&self) -> String {
        let origin = match self.source {
            ContextPlanSource::EnginePlan => "engine plan",
            ContextPlanSource::TranscriptFallback => "transcript fallback",
        };
        let mut text = format!("context {origin}: {}", self.summary);
        if let Some(token_budget) = self.token_budget.as_ref() {
            text.push_str(&format!(
                " · {} / {} tokens",
                token_budget.used_tokens, token_budget.max_tokens
            ));
        }
        text
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextBuildResult {
    pub prompt: String,
    pub preview: ContextPlanPreview,
    pub is_dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DryRunContextBuild {
    pub built_at: UnixTimestamp,
    pub result: ContextBuildResult,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineContextPlanOutput {
    pub prompt: String,
    pub summary: String,
    pub lines: Vec<String>,
    pub selected_items: Option<usize>,
    pub omitted_items: Option<usize>,
    pub token_budget: Option<ContextTokenBudgetPreview>,
    pub is_dry_run: bool,
}

#[derive(Debug, Default)]
pub struct AppContextBridge {
    latest_plan_preview: Option<ContextPlanPreview>,
    #[allow(dead_code)]
    latest_dry_run: Option<DryRunContextBuild>,
}

impl AppContextBridge {
    #[allow(dead_code)]
    pub fn latest_plan_preview(&self) -> Option<&ContextPlanPreview> {
        self.latest_plan_preview.as_ref()
    }

    #[allow(dead_code)]
    pub fn latest_dry_run(&self) -> Option<&DryRunContextBuild> {
        self.latest_dry_run.as_ref()
    }

    pub fn status_line_preview_text(&self) -> String {
        self.latest_plan_preview
            .as_ref()
            .map(ContextPlanPreview::status_line_preview_text)
            .unwrap_or_else(|| "context plan pending".to_string())
    }

    #[allow(dead_code)]
    pub fn apply_engine_plan_output(
        &mut self,
        output: EngineContextPlanOutput,
    ) -> ContextBuildResult {
        let preview = ContextPlanPreview {
            source: ContextPlanSource::EnginePlan,
            summary: output.summary,
            lines: output.lines,
            selected_items: output.selected_items,
            omitted_items: output.omitted_items,
            token_budget: output.token_budget,
        };
        let result = ContextBuildResult {
            prompt: output.prompt,
            preview: preview.clone(),
            is_dry_run: output.is_dry_run,
        };
        self.latest_plan_preview = Some(preview);
        if result.is_dry_run {
            self.latest_dry_run = Some(DryRunContextBuild {
                built_at: crate::now_timestamp_ms(),
                result: result.clone(),
            });
        }
        result
    }

    pub fn build_from_transcript(
        &mut self,
        transcript: &[ConversationMessage],
        inference: &InferenceAdapter,
    ) -> Result<ContextBuildResult, String> {
        self.build_from_transcript_internal(transcript, inference, false)
    }

    #[allow(dead_code)]
    pub fn dry_run_from_transcript(
        &mut self,
        transcript: &[ConversationMessage],
        inference: &InferenceAdapter,
    ) -> Result<DryRunContextBuild, String> {
        let result = self.build_from_transcript_internal(transcript, inference, true)?;
        let dry_run = DryRunContextBuild {
            built_at: crate::now_timestamp_ms(),
            result,
        };
        self.latest_dry_run = Some(dry_run.clone());
        Ok(dry_run)
    }

    fn build_from_transcript_internal(
        &mut self,
        transcript: &[ConversationMessage],
        inference: &InferenceAdapter,
        is_dry_run: bool,
    ) -> Result<ContextBuildResult, String> {
        let turns = transcript
            .iter()
            .map(|message| {
                TranscriptTurn::from_author_kind(&message.author_kind, message.content.clone())
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        let prompt = inference
            .render_prompt(&turns)
            .map_err(|error| error.to_string())?;

        let preview = ContextPlanPreview {
            source: ContextPlanSource::TranscriptFallback,
            summary: format!(
                "{} turn{} via template {}",
                turns.len(),
                if turns.len() == 1 { "" } else { "s" },
                inference.selected_template()
            ),
            lines: transcript_preview_lines(transcript),
            selected_items: Some(turns.len()),
            omitted_items: Some(0),
            token_budget: Some(ContextTokenBudgetPreview {
                used_tokens: 0,
                max_tokens: u32::try_from(inference.config().context.max_tokens)
                    .unwrap_or(u32::MAX),
            }),
        };

        self.latest_plan_preview = Some(preview.clone());

        Ok(ContextBuildResult {
            prompt,
            preview,
            is_dry_run,
        })
    }
}

fn transcript_preview_lines(transcript: &[ConversationMessage]) -> Vec<String> {
    const MAX_LINES: usize = 5;
    transcript
        .iter()
        .rev()
        .take(MAX_LINES)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|message| {
            let content = message.content.replace('\n', " ");
            let snippet: String = content.chars().take(80).collect();
            if content.chars().count() > 80 {
                format!("{}: {}…", message.author_kind, snippet)
            } else {
                format!("{}: {}", message.author_kind, snippet)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ozone_core::engine::MessageId;
    use ozone_core::session::SessionId;
    use std::path::PathBuf;

    fn inference_adapter() -> InferenceAdapter {
        InferenceAdapter::load(crate::inference_adapter::InferenceAdapterInit {
            global_config_path: Some(PathBuf::from("/nonexistent/ozone-plus-config.toml")),
            ..Default::default()
        })
        .expect("adapter should load defaults")
    }

    fn message(author_kind: &str, content: &str, ordinal: u8) -> ConversationMessage {
        let session_id =
            SessionId::parse("123e4567-e89b-12d3-a456-426614174000").expect("valid session id");
        let message_id =
            MessageId::parse(format!("123e4567-e89b-12d3-a456-4266141740{:02}", ordinal))
                .expect("valid message id");
        ConversationMessage::new(
            session_id,
            message_id,
            author_kind.to_string(),
            content.to_string(),
            1_700_000_000_000,
        )
    }

    #[test]
    fn transcript_fallback_build_updates_preview_and_status_text() {
        let mut bridge = AppContextBridge::default();
        let adapter = inference_adapter();
        let transcript = vec![
            message("system", "You are concise.", 1),
            message("user", "Hello", 2),
        ];
        let result = bridge
            .build_from_transcript(&transcript, &adapter)
            .expect("fallback build should succeed");

        assert!(!result.prompt.is_empty());
        assert!(!result.is_dry_run);
        assert_eq!(result.preview.source, ContextPlanSource::TranscriptFallback);
        assert!(bridge
            .status_line_preview_text()
            .contains("context transcript fallback:"));
        assert!(bridge.latest_plan_preview().is_some());
    }

    #[test]
    fn applies_engine_plan_and_tracks_dry_run() {
        let mut bridge = AppContextBridge::default();
        let result = bridge.apply_engine_plan_output(EngineContextPlanOutput {
            prompt: "<prompt>".to_string(),
            summary: "selected 9, omitted 2".to_string(),
            lines: vec!["layer: recency".to_string()],
            selected_items: Some(9),
            omitted_items: Some(2),
            token_budget: Some(ContextTokenBudgetPreview {
                used_tokens: 900,
                max_tokens: 2048,
            }),
            is_dry_run: true,
        });

        assert_eq!(result.preview.source, ContextPlanSource::EnginePlan);
        assert!(bridge
            .status_line_preview_text()
            .contains("selected 9, omitted 2"));
        assert!(bridge.latest_dry_run().is_some());
    }
}
