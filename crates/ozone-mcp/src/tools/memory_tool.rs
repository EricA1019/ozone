use crate::OzoneMcpServer;
use crate::ToolReply;
use serde_json::json;
use crate::required_string;
use crate::optional_string;
use crate::optional_u64;
use crate::parse_session_id;
use crate::parse_message_id;
use crate::pinned_memory_record_json;
use crate::pinned_memory_view_json;
use ozone_persist::{CreateNoteMemoryRequest, AuthorId, Provenance, PinMessageMemoryRequest};

pub fn memory_tool(server: &mut OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
    let action = required_string(args, "action")?;
    let sandbox_id = optional_string(args, "sandboxId");
    match action.as_str() {
        "note" => {
            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
            let content = required_string(args, "content")?;
            server.with_repo(sandbox_id.as_deref(), |repo| {
                let record = repo.create_note_memory(
                    &session_id,
                    CreateNoteMemoryRequest::new(
                        content,
                        AuthorId::User,
                        Provenance::UserAuthored,
                    ),
                )?;
                Ok(ToolReply::success(
                    "Created note memory".to_owned(),
                    json!({ "record": pinned_memory_record_json(&record) }),
                ))
            })
        }
        "pin" => {
            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
            let message_id = parse_message_id(&required_string(args, "messageId")?)?;
            let expires_after_turns =
                optional_u64(args, "expiresAfterTurns").map(|value| value as u32);
            server.with_repo(sandbox_id.as_deref(), |repo| {
                let record = repo.pin_message_memory(
                    &session_id,
                    &message_id,
                    PinMessageMemoryRequest {
                        pinned_by: AuthorId::User,
                        expires_after_turns,
                        provenance: Provenance::UserAuthored,
                    },
                )?;
                Ok(ToolReply::success(
                    "Pinned memory".to_owned(),
                    json!({ "record": pinned_memory_record_json(&record) }),
                ))
            })
        }
        "list" => {
            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
            let kind_opt = optional_string(args, "kind");
            server.with_repo(sandbox_id.as_deref(), move |repo| {
                let memories = match kind_opt.as_deref() {
                    Some("note") => repo.list_note_memories(&session_id)?,
                    _ => repo.list_pinned_memories(&session_id)?,
                };
                Ok(ToolReply::success(
                    "Listed memories".to_owned(),
                    json!({
                        "memories": memories.iter().map(pinned_memory_view_json).collect::<Vec<_>>(),
                        "found": memories.len(),
                        "kind": kind_opt.as_deref().unwrap_or("pinned")
                    }),
                ))
            })
        }
        other => Ok(ToolReply::error(
            "Memory action failed".to_owned(),
            json!({ "error": format!("unsupported memory action `{other}`") }),
        )),
    }
}
