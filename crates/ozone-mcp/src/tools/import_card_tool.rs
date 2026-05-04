     1|/// MCP tool: import card tool.
     2|use crate::OzoneMcpServer;
     3|use crate::ToolReply;
     4|use anyhow::Result;
     5|use serde_json::Value;
     6|
     7|pub fn import_card_tool(server: &mut OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
     8|    let sandbox_id = optional_string(args, "sandboxId");
     9|    let session_name = optional_string(args, "sessionName");
    10|    let tags = optional_string_array(args, "tags")?;
    11|    let path = optional_string(args, "path");
    12|    let card_json = optional_string(args, "cardJson");
    13|    let provenance = optional_string(args, "provenance");
    14|    let card = match (path.as_deref(), card_json.as_deref()) {
    15|        (Some(path), _) => {
    16|            let text = fs::read_to_string(path)
    17|                .with_context(|| format!("failed to read character card {}", path))?;
    18|            CharacterCard::from_json_str(&text).map_err(|error| anyhow!(error.to_string()))?
    19|        }
    20|        (None, Some(card_json)) => CharacterCard::from_json_str(card_json)
    21|            .map_err(|error| anyhow!(error.to_string()))?,
    22|        (None, None) => bail!("import_card requires either `path` or `cardJson`"),
    23|    };
    24|    let sillytavern_format = card.source_format.starts_with("chara_card_v2");
    25|    server.with_repo(sandbox_id.as_deref(), |repo| {
    26|        let imported = repo.import_character_card(ImportCharacterCardRequest {
    27|            card,
    28|            session_name,
    29|            tags,
    30|            provenance: provenance
    31|                .unwrap_or_else(|| path.clone().unwrap_or_else(|| "ozone-mcp".to_owned())),
    32|        })?;
    33|        Ok(ToolReply::success(
    34|            "Imported character card".to_owned(),
    35|            json!({
    36|                "session": session_summary_json(&imported.session),
    37|                "seededBranchId": imported.seeded_branch_id.map(|value| value.to_string()),
    38|                "seededMessageId": imported.seeded_message_id.map(|value| value.to_string()),
    39|                "sillytavernFormat": sillytavern_format
    40|            }),
    41|        ))
    42|    })
    43|}
    44|
    45|