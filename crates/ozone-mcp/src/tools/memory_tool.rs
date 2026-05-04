     1|/// MCP tool: memory tool.
     2|use crate::OzoneMcpServer;
     3|use crate::ToolReply;
     4|use anyhow::Result;
     5|use serde_json::Value;
     6|
     7|pub fn memory_tool(server: &mut OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
     8|    let action = required_string(args, "action")?;
     9|    let sandbox_id = optional_string(args, "sandboxId");
    10|    match action.as_str() {
    11|        "note" => {
    12|            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
    13|            let content = required_string(args, "content")?;
    14|            server.with_repo(sandbox_id.as_deref(), |repo| {
    15|                let record = repo.create_note_memory(
    16|                    &session_id,
    17|                    CreateNoteMemoryRequest::new(
    18|                        content,
    19|                        AuthorId::User,
    20|                        Provenance::UserAuthored,
    21|                    ),
    22|                )?;
    23|                Ok(ToolReply::success(
    24|                    "Created note memory".to_owned(),
    25|                    json!({ "record": pinned_memory_record_json(&record) }),
    26|                ))
    27|            })
    28|        }
    29|        "pin" => {
    30|            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
    31|            let message_id = parse_message_id(&required_string(args, "messageId")?)?;
    32|            let expires_after_turns =
    33|                optional_u64(args, "expiresAfterTurns").map(|value| value as u32);
    34|            server.with_repo(sandbox_id.as_deref(), |repo| {
    35|                let record = repo.pin_message_memory(
    36|                    &session_id,
    37|                    &message_id,
    38|                    PinMessageMemoryRequest {
    39|                        pinned_by: AuthorId::User,
    40|                        expires_after_turns,
    41|                        provenance: Provenance::UserAuthored,
    42|                    },
    43|                )?;
    44|                Ok(ToolReply::success(
    45|                    "Pinned memory".to_owned(),
    46|                    json!({ "record": pinned_memory_record_json(&record) }),
    47|                ))
    48|            })
    49|        }
    50|        "list" => {
    51|            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
    52|            let kind_opt = optional_string(args, "kind");
    53|            server.with_repo(sandbox_id.as_deref(), move |repo| {
    54|                let memories = match kind_opt.as_deref() {
    55|                    Some("note") => repo.list_note_memories(&session_id)?,
    56|                    _ => repo.list_pinned_memories(&session_id)?,
    57|                };
    58|                Ok(ToolReply::success(
    59|                    "Listed memories".to_owned(),
    60|                    json!({
    61|                        "memories": memories.iter().map(pinned_memory_view_json).collect::<Vec<_>>(),
    62|                        "found": memories.len(),
    63|                        "kind": kind_opt.as_deref().unwrap_or("pinned")
    64|                    }),
    65|                ))
    66|            })
    67|        }
    68|        other => Ok(ToolReply::error(
    69|            "Memory action failed".to_owned(),
    70|            json!({ "error": format!("unsupported memory action `{other}`") }),
    71|        )),
    72|    }
    73|}
    74|
    75|