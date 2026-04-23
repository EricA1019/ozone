use std::collections::HashSet;

use ozone_core::engine::ConversationMessage;
use ozone_memory::{
    summary::{generate_session_synopsis, SummaryConfig, SummaryInputTurn},
    KeywordExtractor,
};

pub(crate) const DEFAULT_SESSION_TITLE: &str = "New Conversation";
const MAX_TITLE_CHARS: usize = 48;
const MAX_TOPIC_KEYWORDS_WITH_CHARACTER: usize = 2;
const MAX_TOPIC_KEYWORDS_WITHOUT_CHARACTER: usize = 3;
const GENERIC_TOPIC_KEYWORDS: &[&str] = &[
    "about",
    "anything",
    "begin",
    "chat",
    "continue",
    "conversation",
    "create",
    "give",
    "great",
    "help",
    "hello",
    "hi",
    "let",
    "lets",
    "make",
    "need",
    "okay",
    "ok",
    "please",
    "roleplay",
    "session",
    "show",
    "start",
    "tell",
    "want",
];

pub(crate) fn should_auto_title(current_title: &str) -> bool {
    current_title.trim() == DEFAULT_SESSION_TITLE
}

pub(crate) fn generate_session_title(
    character_name: Option<&str>,
    transcript: &[ConversationMessage],
) -> Option<String> {
    let character_name = normalize_character_name(character_name);
    let character_words = character_name_words(character_name.as_deref());
    let visible_messages: Vec<&ConversationMessage> = transcript
        .iter()
        .filter(|message| !message.is_hidden && !message.content.trim().is_empty())
        .collect();
    if visible_messages.is_empty() {
        return None;
    }

    let extractor = KeywordExtractor::new();
    let topic_limit = if character_name.is_some() {
        MAX_TOPIC_KEYWORDS_WITH_CHARACTER
    } else {
        MAX_TOPIC_KEYWORDS_WITHOUT_CHARACTER
    };
    let topic = title_topic(
        &extractor,
        topic_sources(&visible_messages),
        &character_words,
        topic_limit,
    );

    let title = match (character_name, topic) {
        (Some(character_name), Some(topic)) => limit_title(format!("{character_name} — {topic}")),
        (Some(character_name), None) => limit_title(character_name),
        (None, Some(topic)) => limit_title(topic),
        (None, None) => return None,
    };

    (!title.is_empty() && title != DEFAULT_SESSION_TITLE).then_some(title)
}

fn topic_sources(messages: &[&ConversationMessage]) -> Vec<String> {
    let mut sources = Vec::new();

    if let Some(first_user) = messages
        .iter()
        .find(|message| message.author_kind == "user")
    {
        sources.push(first_user.content.clone());
    }

    if let Some(synopsis) = generate_synopsis(messages) {
        sources.push(synopsis);
    }

    if let Some(first_assistant) = messages
        .iter()
        .find(|message| message.author_kind == "assistant")
        .map(|message| message.content.clone())
    {
        sources.push(first_assistant);
    }

    sources
}

fn generate_synopsis(messages: &[&ConversationMessage]) -> Option<String> {
    let turns: Vec<SummaryInputTurn> = messages
        .iter()
        .map(|message| SummaryInputTurn {
            role: message.author_kind.clone(),
            content: message.content.clone(),
        })
        .collect();
    generate_session_synopsis(
        &turns,
        &SummaryConfig {
            synopsis_min_messages: 2,
            synopsis_max_chars: 160,
            ..SummaryConfig::default()
        },
    )
}

fn title_topic(
    extractor: &KeywordExtractor,
    sources: Vec<String>,
    character_words: &HashSet<String>,
    limit: usize,
) -> Option<String> {
    let mut keywords = Vec::new();

    for source in sources {
        for keyword in extractor.extract(&source) {
            if character_words.contains(&keyword)
                || GENERIC_TOPIC_KEYWORDS.contains(&keyword.as_str())
            {
                continue;
            }
            if !keywords.contains(&keyword) {
                keywords.push(keyword);
            }
            if keywords.len() >= limit {
                break;
            }
        }
        if keywords.len() >= limit {
            break;
        }
    }

    if keywords.is_empty() {
        return None;
    }

    let words: Vec<String> = keywords
        .into_iter()
        .filter_map(|keyword| display_keyword(&keyword))
        .collect();
    (!words.is_empty()).then(|| words.join(" "))
}

fn normalize_character_name(character_name: Option<&str>) -> Option<String> {
    character_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn character_name_words(character_name: Option<&str>) -> HashSet<String> {
    character_name
        .unwrap_or_default()
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| word.to_lowercase())
        .collect()
}

fn display_keyword(keyword: &str) -> Option<String> {
    let lower = keyword.trim().to_lowercase();
    if lower.is_empty() {
        return None;
    }

    Some(match lower.as_str() {
        "ai" => "AI".to_owned(),
        "api" => "API".to_owned(),
        "cpu" => "CPU".to_owned(),
        "ctx" => "Ctx".to_owned(),
        "cuda" => "CUDA".to_owned(),
        "fts" => "FTS".to_owned(),
        "gpu" => "GPU".to_owned(),
        "koboldcpp" => "KoboldCpp".to_owned(),
        "llamacpp" => "llama.cpp".to_owned(),
        "llm" => "LLM".to_owned(),
        "mcp" => "MCP".to_owned(),
        "ollama" => "Ollama".to_owned(),
        "ozone" => "Ozone".to_owned(),
        "ram" => "RAM".to_owned(),
        "vram" => "VRAM".to_owned(),
        _ => title_case_ascii(&lower),
    })
}

fn title_case_ascii(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => {
            let mut titled = first.to_uppercase().collect::<String>();
            titled.push_str(chars.as_str());
            titled
        }
        None => String::new(),
    }
}

fn limit_title(title: impl Into<String>) -> String {
    let title = title.into();
    if title.chars().count() <= MAX_TITLE_CHARS {
        return title;
    }

    let mut words: Vec<&str> = title.split_whitespace().collect();
    while words.len() > 1 && words.join(" ").chars().count() > MAX_TITLE_CHARS {
        words.pop();
    }

    let shortened = words.join(" ");
    if shortened.chars().count() <= MAX_TITLE_CHARS && !shortened.is_empty() {
        shortened
    } else {
        title.chars().take(MAX_TITLE_CHARS).collect()
    }
}

#[cfg(test)]
mod tests {
    use ozone_core::{
        engine::{ConversationMessage, MessageId},
        session::SessionId,
    };

    use super::{generate_session_title, should_auto_title, DEFAULT_SESSION_TITLE};

    fn message(author_kind: &str, content: &str, ordinal: usize) -> ConversationMessage {
        ConversationMessage::new(
            SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap(),
            MessageId::parse(format!("223e4567-e89b-12d3-a456-4266141740{ordinal:02}")).unwrap(),
            author_kind,
            content,
            ordinal as i64,
        )
    }

    #[test]
    fn default_placeholder_is_auto_title_eligible() {
        assert!(should_auto_title(DEFAULT_SESSION_TITLE));
        assert!(!should_auto_title("Observatory Intake"));
    }

    #[test]
    fn uses_character_name_plus_topic_keywords() {
        let transcript = vec![
            message("user", "Let's do an observatory intake with Nyra and review the star charts.", 1),
            message("assistant", "Great — we can treat this as an observatory intake and review the charts together.", 2),
        ];

        let title = generate_session_title(Some("Nyra"), &transcript).unwrap();
        assert_eq!(title, "Nyra — Observatory Intake");
    }

    #[test]
    fn uses_topic_keywords_without_character_name() {
        let transcript = vec![
            message(
                "user",
                "Koboldcpp launch failure on ubuntu with cuda after the update.",
                1,
            ),
            message(
                "assistant",
                "Let's debug the KoboldCpp launch failure on Ubuntu and check CUDA setup first.",
                2,
            ),
        ];

        let title = generate_session_title(None, &transcript).unwrap();
        assert_eq!(title, "KoboldCpp Launch Failure");
    }

    #[test]
    fn falls_back_to_character_name_when_topic_is_too_thin() {
        let transcript = vec![message("user", "hi", 1), message("assistant", "hello", 2)];

        let title = generate_session_title(Some("Aster"), &transcript).unwrap();
        assert_eq!(title, "Aster");
    }

    #[test]
    fn returns_none_when_there_is_no_useful_title_signal() {
        let transcript = vec![message("user", "hi", 1), message("assistant", "ok", 2)];

        assert_eq!(generate_session_title(None, &transcript), None);
    }
}
