use crate::OzoneMcpServer;
use crate::ToolReply;
use serde_json::json;
use anyhow::anyhow;
use uuid::Uuid;
use crate::required_string;
use crate::optional_string;
use crate::optional_bool;
use crate::parse_session_id;
use crate::parse_message_id;
use crate::parse_branch_id;
use crate::now_timestamp_ms;
use crate::branch_record_json;
use ozone_core::engine::{ConversationBranch, BranchState, CreateBranchCommand};

pub fn branch_tool(server: &mut OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
    let action = required_string(args, "action")?;
    let sandbox_id = optional_string(args, "sandboxId");
    match action.as_str() {
        "create" => {
            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
            let name = required_string(args, "name")?;
            let activate = optional_bool(args, "activate").unwrap_or(false);
            let from_message_id = optional_string(args, "fromMessageId")
                .map(|value| parse_message_id(&value))
                .transpose()?;
            server.with_repo(sandbox_id.as_deref(), |repo| {
                let tip_message_id = match from_message_id {
                    Some(value) => value,
                    None => {
                        repo.get_active_branch(&session_id)?
                            .ok_or_else(|| {
                                anyhow!("session {session_id} has no active branch")
                            })?
                            .branch
                            .tip_message_id
                    }
                };
                let mut branch = ConversationBranch::new(
                    parse_branch_id(&Uuid::new_v4().to_string())?,
                    session_id.clone(),
                    name,
                    tip_message_id.clone(),
                    now_timestamp_ms(),
                );
                branch.state = if activate {
                    BranchState::Active
                } else {
                    BranchState::Inactive
                };
                let record = repo.create_branch(CreateBranchCommand {
                    branch,
                    forked_from: tip_message_id,
                })?;
                Ok(ToolReply::success(
                    "Created branch".to_owned(),
                    json!({ "branch": branch_record_json(&record) }),
                ))
            })
        }
        "list" => {
            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
            server.with_repo(sandbox_id.as_deref(), |repo| {
                let branches = repo.list_branches(&session_id)?;
                Ok(ToolReply::success(
                    "Listed branches".to_owned(),
                    json!({
                        "branches": branches.iter().map(branch_record_json).collect::<Vec<_>>(),
                        "found": branches.len()
                    }),
                ))
            })
        }
        "activate" => {
            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
            let branch_id = parse_branch_id(&required_string(args, "branchId")?)?;
            server.with_repo(sandbox_id.as_deref(), |repo| {
                let branch = repo.activate_branch(&session_id, &branch_id)?;
                Ok(ToolReply::success(
                    "Activated branch".to_owned(),
                    json!({ "branch": branch_record_json(&branch) }),
                ))
            })
        }
        "delete" => {
            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
            let branch_id = parse_branch_id(&required_string(args, "branchId")?)?;
            server.with_repo(sandbox_id.as_deref(), |repo| {
                repo.delete_branch(&session_id, &branch_id)?;
                Ok(ToolReply::success(
                    "Deleted branch".to_owned(),
                    json!({ "branchId": branch_id.to_string() }),
                ))
            })
        }
        other => Ok(ToolReply::error(
            "Branch action failed".to_owned(),
            json!({ "error": format!("unsupported branch action `{other}`") }),
        )),
    }
}
