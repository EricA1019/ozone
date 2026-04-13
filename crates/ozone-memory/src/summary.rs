//! Deterministic summary generation.
//!
//! Phase 2C summaries use pure text extraction — no LLM calls.
//! Phase 3 will add utility-model-generated summaries.

/// A message turn ready for summarization.
#[derive(Debug, Clone)]
pub struct SummaryInputTurn {
    pub role: String,
    pub content: String,
}

/// Configuration for summary generation.
#[derive(Debug, Clone)]
pub struct SummaryConfig {
    /// Maximum sentences to include in a chunk summary.
    pub chunk_max_sentences: usize,
    /// Maximum characters for a session synopsis.
    pub synopsis_max_chars: usize,
    /// Minimum messages before a synopsis can be generated.
    pub synopsis_min_messages: usize,
}

impl Default for SummaryConfig {
    fn default() -> Self {
        Self {
            chunk_max_sentences: 5,
            synopsis_max_chars: 500,
            synopsis_min_messages: 10,
        }
    }
}

/// Generate a chunk summary from a sequence of message turns.
///
/// Extracts the first `config.chunk_max_sentences` sentences from the combined
/// message content, prioritizing assistant messages (which tend to contain more
/// narrative content).
pub fn generate_chunk_summary(
    turns: &[SummaryInputTurn],
    config: &SummaryConfig,
) -> Option<String> {
    if turns.is_empty() {
        return None;
    }

    let mut sentences = Vec::new();

    // Prioritize assistant turns, then user turns.
    let mut sorted_turns: Vec<&SummaryInputTurn> = turns.iter().collect();
    sorted_turns.sort_by_key(|t| if t.role == "assistant" { 0 } else { 1 });

    for turn in &sorted_turns {
        for sentence in split_sentences(&turn.content) {
            let trimmed = sentence.trim();
            if !trimmed.is_empty() && trimmed.len() >= 10 {
                sentences.push(trimmed.to_string());
                if sentences.len() >= config.chunk_max_sentences {
                    break;
                }
            }
        }
        if sentences.len() >= config.chunk_max_sentences {
            break;
        }
    }

    if sentences.is_empty() {
        return None;
    }

    Some(sentences.join(" "))
}

/// Generate a session synopsis from all message turns.
///
/// Produces a one-paragraph overview of the session by extracting the first
/// sentence from each assistant turn, up to `config.synopsis_max_chars`.
pub fn generate_session_synopsis(
    turns: &[SummaryInputTurn],
    config: &SummaryConfig,
) -> Option<String> {
    if turns.len() < config.synopsis_min_messages {
        return None;
    }

    let mut parts = Vec::new();
    let mut total_chars = 0;

    for turn in turns {
        if turn.role != "assistant" {
            continue;
        }
        if let Some(first_sentence) = split_sentences(&turn.content).into_iter().next() {
            let trimmed = first_sentence.trim();
            if trimmed.is_empty() || trimmed.len() < 5 {
                continue;
            }
            if total_chars + trimmed.len() > config.synopsis_max_chars {
                break;
            }
            total_chars += trimmed.len() + 1; // +1 for space separator
            parts.push(trimmed.to_string());
        }
    }

    if parts.is_empty() {
        return None;
    }

    Some(parts.join(" "))
}

/// Split text into sentences using basic punctuation heuristics.
///
/// Handles `.`, `!`, `?` as sentence terminators.
/// Avoids splitting on common abbreviations like "Mr.", "Dr.", "etc.".
fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        current.push(ch);
        if matches!(ch, '.' | '!' | '?') {
            let trimmed = current.trim().to_string();
            // Avoid splitting on abbreviations (word < 4 chars before period).
            if ch == '.' {
                let words: Vec<&str> = trimmed.split_whitespace().collect();
                if let Some(last_word) = words.last() {
                    if last_word.len() <= 3 && !trimmed.ends_with("..") {
                        continue;
                    }
                }
            }
            if !trimmed.is_empty() {
                sentences.push(trimmed);
            }
            current = String::new();
        }
    }

    // Remaining text that doesn't end with punctuation.
    let remaining = current.trim().to_string();
    if !remaining.is_empty() && remaining.len() >= 10 {
        sentences.push(remaining);
    }

    sentences
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> SummaryConfig {
        SummaryConfig {
            chunk_max_sentences: 3,
            synopsis_max_chars: 200,
            synopsis_min_messages: 2,
        }
    }

    fn turn(role: &str, content: &str) -> SummaryInputTurn {
        SummaryInputTurn {
            role: role.into(),
            content: content.into(),
        }
    }

    #[test]
    fn chunk_summary_extracts_sentences() {
        let turns = vec![
            turn("user", "Tell me about forests."),
            turn("assistant", "Forests are important ecosystems. They cover about 30% of the Earth's land area. Many species depend on them."),
        ];
        let result = generate_chunk_summary(&turns, &cfg()).unwrap();
        assert!(result.contains("Forests are important ecosystems."));
        assert!(result.contains("They cover about 30%"));
    }

    #[test]
    fn chunk_summary_empty_input() {
        assert!(generate_chunk_summary(&[], &cfg()).is_none());
    }

    #[test]
    fn chunk_summary_short_turns() {
        let turns = vec![turn("user", "Hi"), turn("assistant", "Hey")];
        // Very short content should return None (below 10 char threshold).
        assert!(generate_chunk_summary(&turns, &cfg()).is_none());
    }

    #[test]
    fn session_synopsis_below_threshold() {
        let turns = vec![turn("user", "Hello")];
        let config = SummaryConfig {
            synopsis_min_messages: 5,
            ..cfg()
        };
        assert!(generate_session_synopsis(&turns, &config).is_none());
    }

    #[test]
    fn session_synopsis_extracts_first_sentences() {
        let turns = vec![
            turn("user", "Tell me a story."),
            turn(
                "assistant",
                "Once upon a time there was a brave knight. He traveled far and wide.",
            ),
            turn("user", "What happened next?"),
            turn(
                "assistant",
                "The knight found a hidden castle. It was surrounded by a dark forest.",
            ),
        ];
        let config = SummaryConfig {
            synopsis_min_messages: 2,
            ..cfg()
        };
        let result = generate_session_synopsis(&turns, &config).unwrap();
        assert!(result.contains("Once upon a time"));
        assert!(result.contains("The knight found a hidden castle."));
    }

    #[test]
    fn session_synopsis_respects_max_chars() {
        let turns = vec![
            turn("user", "Start."),
            turn("assistant", "A very long opening sentence that goes on and on and is really quite detailed about the topic at hand."),
            turn("user", "Continue."),
            turn("assistant", "Another extremely long sentence that would push us way over the character limit if we included it fully."),
        ];
        let config = SummaryConfig {
            synopsis_max_chars: 110,
            synopsis_min_messages: 2,
            ..cfg()
        };
        let result = generate_session_synopsis(&turns, &config).unwrap();
        assert!(result.len() <= 115); // small tolerance for join space
    }

    #[test]
    fn split_sentences_basic() {
        let sentences = split_sentences("Hello world. How are you? I am fine!");
        assert_eq!(sentences.len(), 3);
    }
}
