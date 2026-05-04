     1|/// MCP tool: branch tool.
     2|use crate::OzoneMcpServer;
     3|use crate::ToolReply;
     4|use anyhow::Result;
     5|use serde_json::Value;
     6|
     7|pub fn branch_tool(server: &mut OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
     8|    let action = required_string(args, "action")?;
     9|    let sandbox_id = optional_string(args, "sandboxId");
    10|    match action.as_str() {
    11|        "create" => {
    12|            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
    13|            let name = required_string(args, "name")?;
    14|            let activate = optional_bool(args, "activate").unwrap_or(false);
    15|            let from_message_id = optional_string(args, "fromMessageId")
    16|                .map(|value| parse_message_id(&value))
    17|                .transpose()?;
    18|            server.with_repo(sandbox_id.as_deref(), |repo| {
    19|                let tip_message_id = match from_message_id {
    20|                    Some(value) => value,
    21|                    None => {
    22|                        repo.get_active_branch(&session_id)?
    23|                            .ok_or_else(|| {
    24|                                anyhow!("session {session_id} has no active branch")
    25|                            })?
    26|                            .branch
    27|                            .tip_message_id
    28|                    }
    29|                };
    30|                let mut branch = ConversationBranch::new(
    31|                    parse_branch_id(&Uuid::new_v4().to_string())?,
    32|                    session_id.clone(),
    33|                    name,
    34|                    tip_message_id.clone(),
    35|                    now_timestamp_ms(),
    36|                );
    37|                branch.state = if activate {
    38|                    BranchState::Active
    39|                } else {
    40|                    BranchState::Inactive
    41|                };
    42|                let record = repo.create_branch(CreateBranchCommand {
    43|                    branch,
    44|                    forked_from: tip_message_id,
    45|                })?;
    46|                Ok(ToolReply::success(
    47|                    "Created branch".to_owned(),
    48|                    json!({ "branch": branch_record_json(&record) }),
    49|                ))
    50|            })
    51|        }
    52|        "list" => {
    53|            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
    54|            server.with_repo(sandbox_id.as_deref(), |repo| {
    55|                let branches = repo.list_branches(&session_id)?;
    56|                Ok(ToolReply::success(
    57|                    "Listed branches".to_owned(),
    58|                    json!({
    59|                        "branches": branches.iter().map(branch_record_json).collect::<Vec<_>>(),
    60|                        "found": branches.len()
    61|                    }),
    62|                ))
    63|            })
    64|        }
    65|        "activate" => {
    66|            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
    67|            let branch_id = parse_branch_id(&required_string(args, "branchId")?)?;
    68|            server.with_repo(sandbox_id.as_deref(), |repo| {
    69|                let branch = repo.activate_branch(&session_id, &branch_id)?;
    70|                Ok(ToolReply::success(
    71|                    "Activated branch".to_owned(),
    72|                    json!({ "branch": branch_record_json(&branch) }),
    73|                ))
    74|            })
    75|        }
    76|        "delete" => {
    77|            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
    78|            let branch_id = parse_branch_id(&required_string(args, "branchId")?)?;
    79|            server.with_repo(sandbox_id.as_deref(), |repo| {
    80|                repo.delete_branch(&session_id, &branch_id)?;
    81|                Ok(ToolReply::success(
    82|                    "Deleted branch".to_owned(),
    83|                    json!({ "branchId": branch_id.to_string() }),
    84|                ))
    85|            })
    86|        }
    87|        other => Ok(ToolReply::error(
    88|            "Branch action failed".to_owned(),
    89|            json!({ "error": format!("unsupported branch action `{other}`") }),
    90|        )),
    91|    }
    92|}
    93|
    94|