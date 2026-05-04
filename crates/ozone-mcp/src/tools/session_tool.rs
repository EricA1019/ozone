     1|/// MCP tool: session tool.
     2|use crate::OzoneMcpServer;
     3|use crate::ToolReply;
     4|use anyhow::Result;
     5|use serde_json::Value;
     6|
     7|pub fn session_tool(server: &mut OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
     8|    let action = required_string(args, "action")?;
     9|    let sandbox_id = optional_string(args, "sandboxId");
    10|    match action.as_str() {
    11|        "create" => {
    12|            let name = required_string(args, "name")?;
    13|            let character_name = optional_string(args, "characterName");
    14|            let tags = optional_string_array(args, "tags")?;
    15|            server.with_repo(sandbox_id.as_deref(), |repo| {
    16|                let mut request = CreateSessionRequest::new(name);
    17|                request.character_name = character_name;
    18|                request.tags = tags;
    19|                let session = repo.create_session(request)?;
    20|                Ok(ToolReply::success(
    21|                    "Created ozone+ session".to_owned(),
    22|                    json!({ "session": session_summary_json(&session) }),
    23|                ))
    24|            })
    25|        }
    26|        "list" => server.with_repo(sandbox_id.as_deref(), |repo| {
    27|            let sessions = repo.list_sessions()?;
    28|            let session_jsons: Vec<Value> = sessions
    29|                .iter()
    30|                .map(|session| {
    31|                    let mut json = session_summary_json(session);
    32|                    // Attach last message preview if available
    33|                    if let Ok(messages) = repo.list_session_messages(&session.session_id) {
    34|                        if let Some(last) = messages.last() {
    35|                            if let Some(obj) = json.as_object_mut() {
    36|                                obj.insert(
    37|                                    "lastMessagePreview".to_owned(),
    38|                                    json!(last.content.as_str()),
    39|                                );
    40|                            }
    41|                        }
    42|                    }
    43|                    json
    44|                })
    45|                .collect();
    46|            Ok(ToolReply::success(
    47|                "Listed ozone+ sessions".to_owned(),
    48|                json!({
    49|                    "sessions": session_jsons,
    50|                    "found": sessions.len()
    51|                }),
    52|            ))
    53|        }),
    54|        "metadata" => {
    55|            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
    56|            server.with_repo(sandbox_id.as_deref(), |repo| {
    57|                let session = repo
    58|                    .get_session(&session_id)?
    59|                    .ok_or_else(|| anyhow!("session {session_id} was not found"))?;
    60|                let active_branch = repo.get_active_branch(&session_id)?;
    61|                let transcript_message_count = match active_branch.as_ref() {
    62|                    Some(record) => repo
    63|                        .list_branch_messages(&session_id, &record.branch.branch_id)?
    64|                        .len(),
    65|                    None => 0,
    66|                };
    67|                let lock_probe = probe_session_lock(&repo, &session_id)?;
    68|                let active_branch_name = active_branch.as_ref().map(|r| r.branch.name.clone());
    69|                Ok(ToolReply::success(
    70|                    "Loaded session metadata".to_owned(),
    71|                    json!({
    72|                        "session": session_summary_json(&session),
    73|                        "activeBranch": active_branch.as_ref().map(branch_record_json),
    74|                        "activeBranchName": active_branch_name,
    75|                        "transcriptMessageCount": transcript_message_count,
    76|                        "lock": lock_probe
    77|                    }),
    78|                ))
    79|            })
    80|        }
    81|        "load" => {
    82|            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
    83|            server.with_repo(sandbox_id.as_deref(), |repo| {
    84|                // Seed greeting if character has one and transcript is empty
    85|                let greeting_seeded = repo.maybe_seed_character_greeting(&session_id)?;
    86|                // Build standard metadata response
    87|                let session = repo
    88|                    .get_session(&session_id)?
    89|                    .ok_or_else(|| anyhow!("session {session_id} was not found"))?;
    90|                let active_branch = repo.get_active_branch(&session_id)?;
    91|                let transcript_message_count = match active_branch.as_ref() {
    92|                    Some(record) => repo
    93|                        .list_branch_messages(&session_id, &record.branch.branch_id)?
    94|                        .len(),
    95|                    None => 0,
    96|                };
    97|                let lock_probe = probe_session_lock(&repo, &session_id)?;
    98|                let active_branch_name = active_branch.as_ref().map(|r| r.branch.name.clone());
    99|                Ok(ToolReply::success(
   100|                    "Loaded session".to_owned(),
   101|                    json!({
   102|                        "session": session_summary_json(&session),
   103|                        "activeBranch": active_branch.as_ref().map(branch_record_json),
   104|                        "activeBranchName": active_branch_name,
   105|                        "transcriptMessageCount": transcript_message_count,
   106|                        "lock": lock_probe,
   107|                        "greetingSeeded": greeting_seeded,
   108|                    }),
   109|                ))
   110|            })
   111|        }
   112|        "transcript" => {
   113|            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
   114|            let branch_id = optional_string(args, "branchId")
   115|                .map(|value| parse_branch_id(&value))
   116|                .transpose()?;
   117|            server.with_repo(sandbox_id.as_deref(), |repo| {
   118|                let branch = match branch_id.as_ref() {
   119|                    Some(branch_id) => repo
   120|                        .get_branch(&session_id, branch_id)?
   121|                        .ok_or_else(|| anyhow!("branch {branch_id} was not found"))?,
   122|                    None => repo
   123|                        .get_active_branch(&session_id)?
   124|                        .ok_or_else(|| anyhow!("session {session_id} has no active branch"))?,
   125|                };
   126|                let messages =
   127|                    repo.list_branch_messages(&session_id, &branch.branch.branch_id)?;
   128|                Ok(ToolReply::success(
   129|                    "Loaded transcript".to_owned(),
   130|                    json!({
   131|                        "branch": branch_record_json(&branch),
   132|                        "messages": messages.iter().map(message_json).collect::<Vec<_>>()
   133|                    }),
   134|                ))
   135|            })
   136|        }
   137|        "rename" => {
   138|            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
   139|            let new_name = required_string(args, "newName")?;
   140|            server.with_repo(sandbox_id.as_deref(), |repo| {
   141|                let session = repo.rename_session(&session_id, &new_name)?;
   142|                Ok(ToolReply::success(
   143|                    "Renamed ozone+ session".to_owned(),
   144|                    json!({ "session": session_summary_json(&session) }),
   145|                ))
   146|            })
   147|        }
   148|        "delete" => {
   149|            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
   150|            server.with_repo(sandbox_id.as_deref(), |repo| {
   151|                repo.delete_session(&session_id)?;
   152|                Ok(ToolReply::success(
   153|                    "Deleted ozone+ session".to_owned(),
   154|                    json!({ "sessionId": session_id.to_string() }),
   155|                ))
   156|            })
   157|        }
   158|        other => Ok(ToolReply::error(
   159|            "Session action failed".to_owned(),
   160|            json!({ "error": format!("unsupported session action `{other}`") }),
   161|        )),
   162|    }
   163|}
   164|
   165|