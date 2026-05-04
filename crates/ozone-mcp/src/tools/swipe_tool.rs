     1|/// MCP tool: swipe tool.
     2|use crate::OzoneMcpServer;
     3|use crate::ToolReply;
     4|use anyhow::Result;
     5|use serde_json::Value;
     6|
     7|pub fn swipe_tool(server: &mut OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
     8|    let action = required_string(args, "action")?;
     9|    let sandbox_id = optional_string(args, "sandboxId");
    10|    match action.as_str() {
    11|        "add" => {
    12|            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
    13|            let parent_message_id =
    14|                parse_message_id(&required_string(args, "parentMessageId")?)?;
    15|            let content = required_string(args, "content")?;
    16|            let parent_context_message_id = optional_string(args, "contextMessageId")
    17|                .map(|value| parse_message_id(&value))
    18|                .transpose()?;
    19|            let swipe_group_id = optional_string(args, "swipeGroupId")
    20|                .map(|value| parse_swipe_group_id(&value))
    21|                .transpose()?;
    22|            let ordinal = optional_u64(args, "ordinal").map(|value| value as u16);
    23|            let author_kind =
    24|                optional_string(args, "author").unwrap_or_else(|| "assistant".to_owned());
    25|            let author_name = optional_string(args, "authorName");
    26|            let state = optional_string(args, "state")
    27|                .map(|value| value.parse::<SwipeCandidateState>())
    28|                .transpose()?
    29|                .unwrap_or_default();
    30|            server.with_repo(sandbox_id.as_deref(), |repo| {
    31|                let message_record = repo.insert_message(
    32|                    &session_id,
    33|                    CreateMessageRequest {
    34|                        parent_id: Some(parent_message_id.to_string()),
    35|                        author_kind,
    36|                        author_name,
    37|                        content,
    38|                    },
    39|                )?;
    40|                let message_id = parse_message_id(&message_record.message_id)?;
    41|                let existing_group = match swipe_group_id.as_ref() {
    42|                    Some(group_id) => repo.get_swipe_group(&session_id, group_id)?,
    43|                    None => repo
    44|                        .list_swipe_groups(&session_id)?
    45|                        .into_iter()
    46|                        .find(|group| group.parent_message_id == parent_message_id),
    47|                };
    48|                let mut group = existing_group.unwrap_or_else(|| {
    49|                    let mut group = SwipeGroup::new(
    50|                        parse_swipe_group_id(&Uuid::new_v4().to_string())
    51|                            .expect("generated uuid should parse"),
    52|                        parent_message_id.clone(),
    53|                    );
    54|                    group.parent_context_message_id = parent_context_message_id.clone();
    55|                    group
    56|                });
    57|                if group.parent_context_message_id.is_none() {
    58|                    group.parent_context_message_id = parent_context_message_id;
    59|                }
    60|                let next_ordinal = match ordinal {
    61|                    Some(value) => value,
    62|                    None => {
    63|                        match repo.list_swipe_candidates(&session_id, &group.swipe_group_id) {
    64|                            Ok(candidates) => candidates
    65|                                .iter()
    66|                                .map(|candidate| candidate.ordinal)
    67|                                .max()
    68|                                .unwrap_or(0)
    69|                                .saturating_add(1),
    70|                            Err(PersistError::SwipeGroupNotFound(_)) => 0,
    71|                            Err(error) => return Err(anyhow!(error.to_string())),
    72|                        }
    73|                    }
    74|                };
    75|                let candidate = repo.record_swipe_candidate(
    76|                    &session_id,
    77|                    ozone_persist::RecordSwipeCandidateCommand {
    78|                        group: group.clone(),
    79|                        candidate: SwipeCandidate {
    80|                            swipe_group_id: group.swipe_group_id.clone(),
    81|                            ordinal: next_ordinal,
    82|                            message_id,
    83|                            state,
    84|                            partial_content: None,
    85|                            tokens_generated: None,
    86|                        },
    87|                    },
    88|                )?;
    89|                Ok(ToolReply::success(
    90|                    "Added swipe candidate".to_owned(),
    91|                    json!({
    92|                        "group": swipe_group_json(&group),
    93|                        "candidate": swipe_candidate_json(&candidate)
    94|                    }),
    95|                ))
    96|            })
    97|        }
    98|        "list" => {
    99|            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
   100|            server.with_repo(sandbox_id.as_deref(), |repo| {
   101|                let groups = repo.list_swipe_groups(&session_id)?;
   102|                let mut results = Vec::new();
   103|                for group in groups {
   104|                    let candidates = repo.list_swipe_candidates(&session_id, &group.swipe_group_id)?;
   105|                    results.push(json!({
   106|                        "group": swipe_group_json(&group),
   107|                        "candidates": candidates.iter().map(swipe_candidate_json).collect::<Vec<_>>()
   108|                    }));
   109|                }
   110|                Ok(ToolReply::success(
   111|                    "Listed swipe groups".to_owned(),
   112|                    json!({ "swipes": results }),
   113|                ))
   114|            })
   115|        }
   116|        "activate" => {
   117|            let session_id = parse_session_id(&required_string(args, "sessionId")?)?;
   118|            let swipe_group_id = parse_swipe_group_id(&required_string(args, "swipeGroupId")?)?;
   119|            let ordinal = required_u64(args, "ordinal")? as u16;
   120|            server.with_repo(sandbox_id.as_deref(), |repo| {
   121|                let group = repo.activate_swipe_candidate(
   122|                    &session_id,
   123|                    ActivateSwipeCommand {
   124|                        swipe_group_id: swipe_group_id.clone(),
   125|                        ordinal,
   126|                    },
   127|                )?;
   128|                let selected_candidate = repo
   129|                    .list_swipe_candidates(&session_id, &group.swipe_group_id)?
   130|                    .into_iter()
   131|                    .find(|candidate| candidate.ordinal == group.active_ordinal)
   132|                    .ok_or_else(|| {
   133|                        anyhow!(
   134|                            "swipe group {} is missing active ordinal {}",
   135|                            group.swipe_group_id,
   136|                            group.active_ordinal
   137|                        )
   138|                    })?;
   139|                if let Some(active_branch) = repo.get_active_branch(&session_id)? {
   140|                    let candidate_message_ids = repo
   141|                        .list_swipe_candidates(&session_id, &group.swipe_group_id)?
   142|                        .into_iter()
   143|                        .map(|candidate| candidate.message_id)
   144|                        .collect::<Vec<_>>();
   145|                    if active_branch.branch.tip_message_id == group.parent_message_id
   146|                        || candidate_message_ids.contains(&active_branch.branch.tip_message_id)
   147|                    {
   148|                        let _ = repo.set_branch_tip(
   149|                            &session_id,
   150|                            &active_branch.branch.branch_id,
   151|                            &selected_candidate.message_id,
   152|                        )?;
   153|                    }
   154|                }
   155|                Ok(ToolReply::success(
   156|                    "Activated swipe candidate".to_owned(),
   157|                    json!({
   158|                        "group": swipe_group_json(&group),
   159|                        "selectedCandidate": swipe_candidate_json(&selected_candidate)
   160|                    }),
   161|                ))
   162|            })
   163|        }
   164|        other => Ok(ToolReply::error(
   165|            "Swipe action failed".to_owned(),
   166|            json!({ "error": format!("unsupported swipe action `{other}`") }),
   167|        )),
   168|    }
   169|}
   170|
   171|