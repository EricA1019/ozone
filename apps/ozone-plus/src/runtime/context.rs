use ozone_core::engine::ConversationMessage;
use ozone_engine::ConversationStore;
use ozone_tui::{
    RuntimeContextRefresh as TuiRuntimeContextRefresh, SessionContext as TuiSessionContext,
};

use crate::{
    context_bridge::{ContextBuildResult, ContextPlanPreview, DryRunContextBuild},
    hybrid_search::HybridSearchService,
};

use super::{
    tui_context_dry_run_from_build, tui_context_preview_from_plan, OzonePlusRuntime,
};

impl OzonePlusRuntime {
    pub(super) fn build_context_for_generation(
        &mut self,
        context: &TuiSessionContext,
    ) -> Result<ContextBuildResult, String> {
        let transcript = self
            .engine
            .store()
            .get_active_branch_transcript(&context.session_id)
            .map_err(|error| error.to_string())?;
        self.build_context_for_transcript(context, &transcript)
    }

    pub(super) fn build_context_for_transcript(
        &mut self,
        context: &TuiSessionContext,
        transcript: &[ConversationMessage],
    ) -> Result<ContextBuildResult, String> {
        let pinned_memories = self
            .repo
            .list_pinned_memories(&context.session_id)
            .map_err(|error| error.to_string())?;
        let retrieved_memories =
            HybridSearchService::new(&self.repo, &self.inference.config().memory)
                .context_retrieval(&context.session_id, transcript, &pinned_memories, 3)?;
        self.context_bridge.build_from_transcript(
            transcript,
            &pinned_memories,
            retrieved_memories.as_ref(),
            None,
            &self.inference,
        )
    }

    #[allow(dead_code)]
    pub fn latest_context_plan_preview(&self) -> Option<&ContextPlanPreview> {
        self.context_bridge.latest_plan_preview()
    }

    #[allow(dead_code)]
    pub fn latest_context_dry_run(&self) -> Option<&DryRunContextBuild> {
        self.context_bridge.latest_dry_run()
    }

    #[allow(dead_code)]
    pub fn status_line_context_preview_text(&self) -> String {
        self.context_bridge.status_line_preview_text()
    }

    #[allow(dead_code)]
    pub fn dry_run_context_build(
        &mut self,
        context: &TuiSessionContext,
    ) -> Result<DryRunContextBuild, String> {
        let transcript = self
            .engine
            .store()
            .get_active_branch_transcript(&context.session_id)
            .map_err(|error| error.to_string())?;
        self.dry_run_context_build_for_transcript(context, &transcript)
    }

    pub(super) fn dry_run_context_build_for_transcript(
        &mut self,
        context: &TuiSessionContext,
        transcript: &[ConversationMessage],
    ) -> Result<DryRunContextBuild, String> {
        let pinned_memories = self
            .repo
            .list_pinned_memories(&context.session_id)
            .map_err(|error| error.to_string())?;
        let retrieved_memories =
            HybridSearchService::new(&self.repo, &self.inference.config().memory)
                .context_retrieval(&context.session_id, transcript, &pinned_memories, 3)?;
        self.context_bridge.dry_run_from_transcript(
            transcript,
            &pinned_memories,
            retrieved_memories.as_ref(),
            None,
            &self.inference,
        )
    }

    pub(super) fn build_dry_run_context_refresh(
        &mut self,
        context: &TuiSessionContext,
    ) -> Result<TuiRuntimeContextRefresh, String> {
        let dry_run = self.dry_run_context_build(context)?;
        Ok(TuiRuntimeContextRefresh {
            status_line: Some(format!(
                "Context dry run captured · {}",
                dry_run.result.preview.summary
            )),
            context_preview: self
                .context_bridge
                .latest_plan_preview()
                .map(tui_context_preview_from_plan),
            context_dry_run: self
                .context_bridge
                .latest_dry_run()
                .map(tui_context_dry_run_from_build),
            ..TuiRuntimeContextRefresh::default()
        })
    }

    pub(super) fn build_session_refresh(
        &mut self,
        context: &TuiSessionContext,
        status_line: impl Into<String>,
    ) -> Result<TuiRuntimeContextRefresh, String> {
        let snapshot = self.load_session_snapshot(context)?;
        Ok(TuiRuntimeContextRefresh {
            status_line: Some(status_line.into()),
            session_title: Some(snapshot.session_title),
            transcript: Some(snapshot.transcript),
            session_metadata: Some(snapshot.metadata),
            session_stats: Some(snapshot.stats),
            context_preview: self
                .context_bridge
                .latest_plan_preview()
                .map(tui_context_preview_from_plan),
            context_dry_run: self
                .context_bridge
                .latest_dry_run()
                .map(tui_context_dry_run_from_build),
            ..TuiRuntimeContextRefresh::default()
        })
    }

    pub(super) fn refresh_context_cache(&mut self, context: &TuiSessionContext) {
        let _ = self.dry_run_context_build(context);
    }
}