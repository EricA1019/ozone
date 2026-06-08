use crate::OzoneMcpServer;
use crate::ToolReply;
use serde_json::Value;
use serde_json::json;
use anyhow::anyhow;
use crate::required_string;
use crate::optional_string;
use crate::optional_string_array;
use crate::parse_session_id;
use crate::parse_branch_id;
use crate::session_summary_json;
use crate::branch_record_json;
use crate::message_json;
use crate::probe_session_lock;
use ozone_core::session::CreateSessionRequest;

pub fn session_tool(server: &mut OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
    let action = required_string(args, "action")?;
    let sandbox_id = optional_string(args, "sandboxId");
    match action.as_str() {
        "create" => {
            let name = required_string(args, "name")?;
            let character_name = optional_string(args, "characterName");
            let tags = optional_string_array(args, "tags")?;
            server.with_repo(sandbox_id.as_deref(), |repo| {
                let mut request = CreateSessionRequest::new(name);
                request.character_name = character_name;
                request.tags = tags;
                let session = repo.create_session(request)?;
                Ok(ToolReply::success(
                    "Created ozone+ session".to_owned(),
                    json!({ "session": session_summary_json(&session) }),
                ))
            })
        }
        "list" => server.with_repo(sandbox_id.as_deref(), |repo| {
            let sessions = repo.list_sessions()?;
            let session_jsons: Vec<Value> = sessions
                .iter()
                .map(|session| {
                    let mut json = session_summary_json(session);
                    // Attach last message preview if available
                    if let Ok(messages) = repo.list_session_messages(&session.session_id) {
                        if let Some(last) = messages.last() {
                            if let Some(obj) = json.as_object_mut() {
                                obj.insert(
                                    "lastMessagePreview".to_owned(),
                                    json!(last.content.as_str()),
                                );
                            }
                        }
                    }
                    json
                })
                .collect();
            Ok(ToolReply::success(
                "Listed ozone+ sessions".to_owned(),
                json!({
                    "sessions": session_jsons,
                    "found": sessions.len()
                }),
            ))
        }),
        "metadata" => {
            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
            server.with_repo(sandbox_id.as_deref(), |repo| {
                let session = repo
                    .get_session(&session_id)?
                    .ok_or_else(|| anyhow!("session {session_id} was not found"))?;
                let active_branch = repo.get_active_branch(&session_id)?;
                let transcript_message_count = match active_branch.as_ref() {
                    Some(record) => repo
                        .list_branch_messages(&session_id, &record.branch.branch_id)?
                        .len(),
                    None => 0,
                };
                let lock_probe = probe_session_lock(&repo, &session_id)?;
                let active_branch_name = active_branch.as_ref().map(|r| r.branch.name.clone());
                Ok(ToolReply::success(
                    "Loaded session metadata".to_owned(),
                    json!({
                        "session": session_summary_json(&session),
                        "activeBranch": active_branch.as_ref().map(branch_record_json),
                        "activeBranchName": active_branch_name,
                        "transcriptMessageCount": transcript_message_count,
                        "lock": lock_probe
                    }),
                ))
            })
        }
        "load" => {
            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
            server.with_repo(sandbox_id.as_deref(), |repo| {
                // Seed greeting if character has one and transcript is empty
                let greeting_seeded = repo.maybe_seed_character_greeting(&session_id)?;
                // Build standard metadata response
                let session = repo
                    .get_session(&session_id)?
                    .ok_or_else(|| anyhow!("session {session_id} was not found"))?;
                let active_branch = repo.get_active_branch(&session_id)?;
                let transcript_message_count = match active_branch.as_ref() {
                    Some(record) => repo
                        .list_branch_messages(&session_id, &record.branch.branch_id)?
                        .len(),
                    None => 0,
                };
                let lock_probe = probe_session_lock(&repo, &session_id)?;
                let active_branch_name = active_branch.as_ref().map(|r| r.branch.name.clone());
                Ok(ToolReply::success(
                    "Loaded session".to_owned(),
                    json!({
                        "session": session_summary_json(&session),
                        "activeBranch": active_branch.as_ref().map(branch_record_json),
                        "activeBranchName": active_branch_name,
                        "transcriptMessageCount": transcript_message_count,
                        "lock": lock_probe,
                        "greetingSeeded": greeting_seeded,
                    }),
                ))
            })
        }
        "transcript" => {
            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
            let branch_id = optional_string(args, "branchId")
                .map(|value| parse_branch_id(&value))
                .transpose()?;
            server.with_repo(sandbox_id.as_deref(), |repo| {
                let branch = match branch_id.as_ref() {
                    Some(branch_id) => repo
                        .get_branch(&session_id, branch_id)?
                        .ok_or_else(|| anyhow!("branch {branch_id} was not found"))?,
                    None => repo
                        .get_active_branch(&session_id)?
                        .ok_or_else(|| anyhow!("session {session_id} has no active branch"))?,
                };
                let messages =
                    repo.list_branch_messages(&session_id, &branch.branch.branch_id)?;
                Ok(ToolReply::success(
                    "Loaded transcript".to_owned(),
                    json!({
                        "branch": branch_record_json(&branch),
                        "messages": messages.iter().map(message_json).collect::<Vec<_>>()
                    }),
                ))
            })
        }
        "rename" => {
            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
            let new_name = required_string(args, "newName")?;
            server.with_repo(sandbox_id.as_deref(), |repo| {
                let session = repo.rename_session(&session_id, &new_name)?;
                Ok(ToolReply::success(
                    "Renamed ozone+ session".to_owned(),
                    json!({ "session": session_summary_json(&session) }),
                ))
            })
        }
        "delete" => {
            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
            server.with_repo(sandbox_id.as_deref(), |repo| {
                repo.delete_session(&session_id)?;
                Ok(ToolReply::success(
                    "Deleted ozone+ session".to_owned(),
                    json!({ "sessionId": session_id.to_string() }),
                ))
            })
        }
        other => Ok(ToolReply::error(
            "Session action failed".to_owned(),
            json!({ "error": format!("unsupported session action `{other}`") }),
        )),
    }
}
