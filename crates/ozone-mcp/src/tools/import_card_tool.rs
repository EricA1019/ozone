/// MCP tool: import card tool.
use crate::OzoneMcpServer;
use crate::ToolReply;
use anyhow::Result;
use serde_json::Value;
use serde_json::json;
use anyhow::anyhow;
use anyhow::bail;
use super::optional_string;
use super::optional_string_array;

pub fn import_card_tool(server: &mut OzoneMcpServer, args: &serde_json::Value) -> anyhow::Result<ToolReply> {
    let sandbox_id = optional_string(args, "sandboxId");
    let session_name = optional_string(args, "sessionName");
    let tags = optional_string_array(args, "tags")?;
    let path = optional_string(args, "path");
    let card_json = optional_string(args, "cardJson");
    let provenance = optional_string(args, "provenance");
    let card = match (path.as_deref(), card_json.as_deref()) {
        (Some(path), _) => {
            let text = fs::read_to_string(path)
                .with_context(|| format!("failed to read character card {}", path))?;
            CharacterCard::from_json_str(&text).map_err(|error| anyhow!(error.to_string()))?
        }
        (None, Some(card_json)) => CharacterCard::from_json_str(card_json)
            .map_err(|error| anyhow!(error.to_string()))?,
        (None, None) => bail!("import_card requires either `path` or `cardJson`"),
    };
    let sillytavern_format = card.source_format.starts_with("chara_card_v2");
    server.with_repo(sandbox_id.as_deref(), |repo| {
        let imported = repo.import_character_card(ImportCharacterCardRequest {
            card,
            session_name,
            tags,
            provenance: provenance
                .unwrap_or_else(|| path.clone().unwrap_or_else(|| "ozone-mcp".to_owned())),
        })?;
        Ok(ToolReply::success(
            "Imported character card".to_owned(),
            json!({
                "session": session_summary_json(&imported.session),
                "seededBranchId": imported.seeded_branch_id.map(|value| value.to_string()),
                "seededMessageId": imported.seeded_message_id.map(|value| value.to_string()),
                "sillytavernFormat": sillytavern_format
            }),
        ))
    })
}