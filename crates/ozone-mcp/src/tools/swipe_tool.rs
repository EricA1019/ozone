use crate::optional_string;
use crate::optional_u64;
use crate::parse_message_id;
use crate::parse_session_id;
use crate::parse_swipe_group_id;
use crate::required_string;
use crate::required_u64;
use crate::swipe_candidate_json;
use crate::swipe_group_json;
use crate::OzoneMcpServer;
use crate::ToolReply;
use anyhow::anyhow;
use ozone_core::engine::{ActivateSwipeCommand, SwipeCandidate, SwipeCandidateState, SwipeGroup};
use ozone_persist::{CreateMessageRequest, PersistError};
use serde_json::json;
use uuid::Uuid;

#[allow(clippy::expect_used)]
pub fn swipe_tool(
    server: &mut OzoneMcpServer,
    args: &serde_json::Value,
) -> anyhow::Result<ToolReply> {
    let action = required_string(args, "action")?;
    let sandbox_id = optional_string(args, "sandboxId");
    match action.as_str() {
        "add" => {
            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
            let parent_message_id = parse_message_id(&required_string(args, "parentMessageId")?)?;
            let content = required_string(args, "content")?;
            let parent_context_message_id = optional_string(args, "contextMessageId")
                .map(|value| parse_message_id(&value))
                .transpose()?;
            let swipe_group_id = optional_string(args, "swipeGroupId")
                .map(|value| parse_swipe_group_id(&value))
                .transpose()?;
            let ordinal = optional_u64(args, "ordinal").map(|value| value as u16);
            let author_kind =
                optional_string(args, "author").unwrap_or_else(|| "assistant".to_owned());
            let author_name = optional_string(args, "authorName");
            let state = optional_string(args, "state")
                .map(|value| value.parse::<SwipeCandidateState>())
                .transpose()?
                .unwrap_or_default();
            server.with_repo(sandbox_id.as_deref(), |repo| {
                let message_record = repo.insert_message(
                    &session_id,
                    CreateMessageRequest {
                        parent_id: Some(parent_message_id.to_string()),
                        author_kind,
                        author_name,
                        content,
                    },
                )?;
                let message_id = parse_message_id(&message_record.message_id)?;
                let existing_group = match swipe_group_id.as_ref() {
                    Some(group_id) => repo.get_swipe_group(&session_id, group_id)?,
                    None => repo
                        .list_swipe_groups(&session_id)?
                        .into_iter()
                        .find(|group| group.parent_message_id == parent_message_id),
                };
                let mut group = existing_group.unwrap_or_else(|| {
                    let mut group = SwipeGroup::new(
                        parse_swipe_group_id(&Uuid::new_v4().to_string())
                            .expect("generated uuid should parse"),
                        parent_message_id.clone(),
                    );
                    group.parent_context_message_id = parent_context_message_id.clone();
                    group
                });
                if group.parent_context_message_id.is_none() {
                    group.parent_context_message_id = parent_context_message_id;
                }
                let next_ordinal = match ordinal {
                    Some(value) => value,
                    None => match repo.list_swipe_candidates(&session_id, &group.swipe_group_id) {
                        Ok(candidates) => candidates
                            .iter()
                            .map(|candidate| candidate.ordinal)
                            .max()
                            .unwrap_or(0)
                            .saturating_add(1),
                        Err(PersistError::SwipeGroupNotFound(_)) => 0,
                        Err(error) => return Err(anyhow!(error.to_string())),
                    },
                };
                let candidate = repo.record_swipe_candidate(
                    &session_id,
                    ozone_persist::RecordSwipeCandidateCommand {
                        group: group.clone(),
                        candidate: SwipeCandidate {
                            swipe_group_id: group.swipe_group_id.clone(),
                            ordinal: next_ordinal,
                            message_id,
                            state,
                            partial_content: None,
                            tokens_generated: None,
                        },
                    },
                )?;
                Ok(ToolReply::success(
                    "Added swipe candidate".to_owned(),
                    json!({
                        "group": swipe_group_json(&group),
                        "candidate": swipe_candidate_json(&candidate)
                    }),
                ))
            })
        }
        "list" => {
            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
            server.with_repo(sandbox_id.as_deref(), |repo| {
                let groups = repo.list_swipe_groups(&session_id)?;
                let mut results = Vec::new();
                for group in groups {
                    let candidates = repo.list_swipe_candidates(&session_id, &group.swipe_group_id)?;
                    results.push(json!({
                        "group": swipe_group_json(&group),
                        "candidates": candidates.iter().map(swipe_candidate_json).collect::<Vec<_>>()
                    }));
                }
                Ok(ToolReply::success(
                    "Listed swipe groups".to_owned(),
                    json!({ "swipes": results }),
                ))
            })
        }
        "activate" => {
            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
            let swipe_group_id = parse_swipe_group_id(&required_string(args, "swipeGroupId")?)?;
            let ordinal = required_u64(args, "ordinal")? as u16;
            server.with_repo(sandbox_id.as_deref(), |repo| {
                let group = repo.activate_swipe_candidate(
                    &session_id,
                    ActivateSwipeCommand {
                        swipe_group_id: swipe_group_id.clone(),
                        ordinal,
                    },
                )?;
                let selected_candidate = repo
                    .list_swipe_candidates(&session_id, &group.swipe_group_id)?
                    .into_iter()
                    .find(|candidate| candidate.ordinal == group.active_ordinal)
                    .ok_or_else(|| {
                        anyhow!(
                            "swipe group {} is missing active ordinal {}",
                            group.swipe_group_id,
                            group.active_ordinal
                        )
                    })?;
                if let Some(active_branch) = repo.get_active_branch(&session_id)? {
                    let candidate_message_ids = repo
                        .list_swipe_candidates(&session_id, &group.swipe_group_id)?
                        .into_iter()
                        .map(|candidate| candidate.message_id)
                        .collect::<Vec<_>>();
                    if active_branch.branch.tip_message_id == group.parent_message_id
                        || candidate_message_ids.contains(&active_branch.branch.tip_message_id)
                    {
                        let _ = repo.set_branch_tip(
                            &session_id,
                            &active_branch.branch.branch_id,
                            &selected_candidate.message_id,
                        )?;
                    }
                }
                Ok(ToolReply::success(
                    "Activated swipe candidate".to_owned(),
                    json!({
                        "group": swipe_group_json(&group),
                        "selectedCandidate": swipe_candidate_json(&selected_candidate)
                    }),
                ))
            })
        }
        other => Ok(ToolReply::error(
            "Swipe action failed".to_owned(),
            json!({ "error": format!("unsupported swipe action `{other}`") }),
        )),
    }
}
