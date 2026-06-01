use crate::hybrid_search::HybridSearchService;

use ozone_persist::{AuthorId, CreateNoteMemoryRequest, Provenance, UpdateSessionRequest};

use super::{
    recall_helpers::{
        recent_search_section, tui_context_dry_run_from_build, tui_context_preview_from_plan,
    },
    shell_commands::{format_tags, parse_shell_command, require_non_empty},
    HooksCommand, MemoryCommand, OzonePlusRuntime, SafeModeCommand, SearchCommand,
    SessionCommand, ShellCommand, SummarizeShellCommand, ThinkingCommand, TierBCommand,
    TuiRuntimeContextRefresh, TuiSessionContext,
};

impl OzonePlusRuntime {
    pub(super) fn run_command_impl(
        &mut self,
        context: &TuiSessionContext,
        input: &str,
    ) -> Result<Option<TuiRuntimeContextRefresh>, String> {
        let command = match parse_shell_command(input) {
            Ok(command) => command,
            Err(error) => return Ok(Some(Self::status_only_refresh(error))),
        };

        match command {
            ShellCommand::Session(SessionCommand::Show) => {
                let snapshot = self.load_session_snapshot(context)?;
                let status = format!(
                    "Session {} · character {} · tags {}",
                    snapshot.session_title,
                    snapshot
                        .metadata
                        .character_name
                        .as_deref()
                        .filter(|value| !value.is_empty())
                        .unwrap_or("—"),
                    format_tags(&snapshot.metadata.tags),
                );
                Ok(Some(TuiRuntimeContextRefresh {
                    status_line: Some(status),
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
                }))
            }
            ShellCommand::Session(SessionCommand::Rename(name)) => {
                let name = require_non_empty("session name", name)?;
                self.repo
                    .update_session_metadata(
                        &context.session_id,
                        UpdateSessionRequest {
                            name: Some(name.clone()),
                            ..UpdateSessionRequest::default()
                        },
                    )
                    .map_err(|error| error.to_string())?;
                self.build_session_refresh(context, format!("Session renamed to {name}"))
                    .map(Some)
            }
            ShellCommand::Session(SessionCommand::Retitle) => {
                let current = self
                    .repo
                    .get_session(&context.session_id)
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| format!("session {} was not found", context.session_id))?;
                let Some(title) = self.generate_session_title_for_current_context(context)? else {
                    return Ok(Some(Self::status_only_refresh(
                        "Not enough transcript context to generate a title",
                    )));
                };
                if title == current.name {
                    return Ok(Some(Self::status_only_refresh(format!(
                        "Session title already set to {title}"
                    ))));
                }
                self.repo
                    .update_session_metadata(
                        &context.session_id,
                        UpdateSessionRequest {
                            name: Some(title.clone()),
                            ..UpdateSessionRequest::default()
                        },
                    )
                    .map_err(|error| error.to_string())?;
                self.build_session_refresh(context, format!("Session retitled to {title}"))
                    .map(Some)
            }
            ShellCommand::Session(SessionCommand::Reroll) => Ok(Some(Self::status_only_refresh(
                "`/session reroll` runs from the conversation screen using the selected assistant message"
                    .to_owned(),
            ))),
            ShellCommand::Session(SessionCommand::Character(character_name)) => {
                self.repo
                    .update_session_metadata(
                        &context.session_id,
                        UpdateSessionRequest {
                            character_name: Some(character_name.clone()),
                            ..UpdateSessionRequest::default()
                        },
                    )
                    .map_err(|error| error.to_string())?;
                let status = match character_name {
                    Some(character_name) => format!("Character set to {character_name}"),
                    None => "Character cleared".to_owned(),
                };
                self.build_session_refresh(context, status).map(Some)
            }
            ShellCommand::Session(SessionCommand::Tags(tags)) => {
                self.repo
                    .update_session_metadata(
                        &context.session_id,
                        UpdateSessionRequest {
                            tags: Some(tags.clone()),
                            ..UpdateSessionRequest::default()
                        },
                    )
                    .map_err(|error| error.to_string())?;
                let status = if tags.is_empty() {
                    "Session tags cleared".to_owned()
                } else {
                    format!("Session tags set to {}", format_tags(&tags))
                };
                self.build_session_refresh(context, status).map(Some)
            }
            ShellCommand::Memory(MemoryCommand::List) => self
                .build_recall_browser_refresh(context, "Loaded saved memories")
                .map(Some),
            ShellCommand::Memory(MemoryCommand::Note(text)) => {
                let text = require_non_empty("memory note", text)?;
                self.repo
                    .create_note_memory(
                        &context.session_id,
                        CreateNoteMemoryRequest::new(
                            text,
                            AuthorId::User,
                            Provenance::UserAuthored,
                        ),
                    )
                    .map_err(|error| error.to_string())?;
                self.refresh_context_cache(context);
                self.build_recall_browser_refresh(context, "Created note memory")
                    .map(Some)
            }
            ShellCommand::Memory(MemoryCommand::Unpin(artifact_id)) => {
                let removed = self
                    .repo
                    .remove_saved_memory(&context.session_id, &artifact_id)
                    .map_err(|error| error.to_string())?;
                if !removed {
                    return Ok(Some(Self::status_only_refresh(format!(
                        "Saved memory {} was not found",
                        artifact_id
                    ))));
                }
                self.refresh_context_cache(context);
                self.build_recall_browser_refresh(context, format!("Removed memory {artifact_id}"))
                    .map(Some)
            }
            ShellCommand::Search(SearchCommand::Session(query)) => {
                let query = require_non_empty("search query", query)?;
                let result = HybridSearchService::new(&self.repo, &self.inference.config().memory)
                    .search_session(&context.session_id, &query)?;
                self.recent_search = Some(recent_search_section("session", &result, false));
                self.build_recall_browser_refresh(
                    context,
                    format!(
                        "Session search `{query}` · {} · {} hit{}",
                        result.status.summary_line(),
                        result.hits.len(),
                        if result.hits.len() == 1 { "" } else { "s" }
                    ),
                )
                .map(Some)
            }
            ShellCommand::Search(SearchCommand::Global(query)) => {
                let query = require_non_empty("search query", query)?;
                let result = HybridSearchService::new(&self.repo, &self.inference.config().memory)
                    .search_global(&query)?;
                self.recent_search = Some(recent_search_section("global", &result, true));
                self.build_recall_browser_refresh(
                    context,
                    format!(
                        "Global search `{query}` · {} · {} hit{}",
                        result.status.summary_line(),
                        result.hits.len(),
                        if result.hits.len() == 1 { "" } else { "s" }
                    ),
                )
                .map(Some)
            }
            ShellCommand::Summarize(SummarizeShellCommand::Session) => {
                let transcript = self
                    .repo
                    .get_active_branch_transcript(&context.session_id)
                    .map_err(|error| error.to_string())?;

                if transcript.len() < 2 {
                    return Ok(Some(Self::status_only_refresh(
                        "Need at least 2 messages to generate a synopsis".to_string(),
                    )));
                }

                let turns: Vec<ozone_memory::summary::SummaryInputTurn> = transcript
                    .iter()
                    .map(|msg| ozone_memory::summary::SummaryInputTurn {
                        role: msg.author_kind.clone(),
                        content: msg.content.clone(),
                    })
                    .collect();

                let config = ozone_memory::summary::SummaryConfig::default();
                let status = match ozone_memory::summary::generate_session_synopsis(&turns, &config)
                {
                    Some(synopsis) => {
                        let _ = self.repo.store_session_synopsis(
                            &context.session_id,
                            &synopsis,
                            transcript.len(),
                            0,
                        );
                        format!("Synopsis: {synopsis}")
                    }
                    None => format!(
                        "Not enough assistant content to generate a synopsis ({} messages)",
                        transcript.len()
                    ),
                };

                Ok(Some(Self::status_only_refresh(status)))
            }
            ShellCommand::Thinking(ThinkingCommand::Status) => {
                let mode = match self.thinking_display_mode {
                    ozone_engine::ThinkingDisplayMode::Hidden => "hidden",
                    ozone_engine::ThinkingDisplayMode::Assisted => "assisted",
                    ozone_engine::ThinkingDisplayMode::Debug => "debug",
                };
                Ok(Some(Self::status_only_refresh(format!(
                    "Thinking display: {mode}"
                ))))
            }
            ShellCommand::Thinking(ThinkingCommand::SetMode(mode)) => {
                self.thinking_display_mode = mode;
                let label = match mode {
                    ozone_engine::ThinkingDisplayMode::Hidden => {
                        "hidden (thinking blocks stripped)"
                    }
                    ozone_engine::ThinkingDisplayMode::Assisted => {
                        "assisted (thinking accumulated, not inline)"
                    }
                    ozone_engine::ThinkingDisplayMode::Debug => "debug (thinking shown inline)",
                };
                Ok(Some(Self::status_only_refresh(format!(
                    "Thinking display set to {label}"
                ))))
            }
            ShellCommand::TierB(TierBCommand::Status) => {
                let _ = self.is_tier_b_active();
                let tier_b = &self.inference.config().memory.tier_b;
                let status = if self.safe_mode {
                    "Tier B: OFF (safe mode)".to_owned()
                } else if !tier_b.enabled {
                    "Tier B: OFF (disabled in config)".to_owned()
                } else {
                    format!(
                        "Tier B: ON · importance_proposals={} retrieval_keys={} thinking_summaries={}",
                        tier_b.importance_proposals,
                        tier_b.retrieval_keys,
                        tier_b.thinking_summaries,
                    )
                };
                Ok(Some(Self::status_only_refresh(status)))
            }
            ShellCommand::TierB(TierBCommand::Toggle) => {
                self.safe_mode = !self.safe_mode;
                let status = if self.safe_mode {
                    "Safe mode ON — Tier B features disabled"
                } else {
                    "Safe mode OFF — Tier B features enabled"
                };
                Ok(Some(Self::status_only_refresh(status)))
            }
            ShellCommand::Hooks(HooksCommand::Status) => {
                let has_pre = self.hooks_config.pre_generation.is_some();
                let has_post = self.hooks_config.post_generation.is_some();
                Ok(Some(Self::status_only_refresh(format!(
                    "Hooks: pre_generation={} post_generation={}",
                    if has_pre { "configured" } else { "—" },
                    if has_post { "configured" } else { "—" },
                ))))
            }
            ShellCommand::Hooks(HooksCommand::List) => {
                if self.custom_commands.is_empty() {
                    Ok(Some(Self::status_only_refresh(
                        "No custom commands found in $XDG_CONFIG_HOME/ozone/commands/".to_owned(),
                    )))
                } else {
                    let list = self
                        .custom_commands
                        .iter()
                        .map(|cmd| {
                            if let Some(desc) = &cmd.description {
                                format!("  {}  — {desc}", cmd.name)
                            } else {
                                format!("  {}", cmd.name)
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    Ok(Some(Self::status_only_refresh(format!(
                        "Custom commands:\n{list}"
                    ))))
                }
            }
            ShellCommand::SafeMode(SafeModeCommand::Status) => Ok(Some(Self::status_only_refresh(
                format!("Safe mode: {}", if self.safe_mode { "ON" } else { "OFF" }),
            ))),
            ShellCommand::SafeMode(SafeModeCommand::On) => {
                self.safe_mode = true;
                Ok(Some(Self::status_only_refresh(
                    "Safe mode ON — Tier B features disabled".to_owned(),
                )))
            }
            ShellCommand::SafeMode(SafeModeCommand::Off) => {
                self.safe_mode = false;
                Ok(Some(Self::status_only_refresh(
                    "Safe mode OFF — Tier B features enabled".to_owned(),
                )))
            }
            ShellCommand::SafeMode(SafeModeCommand::Toggle) => {
                self.safe_mode = !self.safe_mode;
                let status = if self.safe_mode {
                    "Safe mode ON — Tier B features disabled"
                } else {
                    "Safe mode OFF — Tier B features enabled"
                };
                Ok(Some(Self::status_only_refresh(status)))
            }
        }
    }
}