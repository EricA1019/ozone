//! Assistive intelligence: heuristic importance scoring and keyword extraction.
//! These are Tier B features — proposals only, never autonomous mutations.

use crate::MemoryContent;

/// Configuration for the importance scorer
#[derive(Debug, Clone)]
pub struct ScoringConfig {
    /// Minimum score to emit a proposal (0.0–1.0)
    pub min_threshold: f32,
    pub recency_weight: f32,
    pub reference_weight: f32,
    pub keyword_weight: f32,
}

impl Default for ScoringConfig {
    fn default() -> Self {
        Self {
            min_threshold: 0.3,
            recency_weight: 0.3,
            reference_weight: 0.4,
            keyword_weight: 0.3,
        }
    }
}

/// Heuristic importance scorer — no LLM call required
pub struct ImportanceScorer {
    config: ScoringConfig,
}

const IMPORTANT_KEYWORDS: &[&str] = &[
    "important",
    "remember",
    "never forget",
    "always",
    "promise",
    "secret",
    "love",
    "hate",
    "fear",
    "goal",
    "dream",
    "wish",
    "must",
    "critical",
    "birthday",
    "anniversary",
    "name",
    "favorite",
];

impl ImportanceScorer {
    pub fn new(config: ScoringConfig) -> Self {
        Self { config }
    }

    /// Score a message's importance (0.0–1.0)
    pub fn score(&self, text: &str, is_user: bool, turn_age: usize) -> f32 {
        let mut score = 0.0f32;

        // Recency: newer turns score higher
        let recency = 1.0 / (1.0 + turn_age as f32 * 0.1);
        score += recency * self.config.recency_weight;

        if is_user {
            score += 0.1;
        }

        let lower = text.to_lowercase();
        let kw_matches = IMPORTANT_KEYWORDS
            .iter()
            .filter(|k| lower.contains(*k))
            .count();
        score += (kw_matches as f32 * 0.15).min(0.5) * self.config.keyword_weight;

        let ref_count = ["i ", "my ", "you ", "your ", "me "]
            .iter()
            .filter(|r| lower.contains(*r))
            .count();
        score += (ref_count as f32 * 0.1).min(0.4) * self.config.reference_weight;

        score.min(1.0)
    }

    /// Return an ImportanceProposal if score meets threshold, else None
    pub fn propose(&self, text: &str, is_user: bool, turn_age: usize) -> Option<MemoryContent> {
        let score = self.score(text, is_user, turn_age);
        if score < self.config.min_threshold {
            return None;
        }
        let justification = if score > 0.7 {
            "High importance: key references and important keywords"
        } else if score > 0.5 {
            "Moderate importance: relevant personal context"
        } else {
            "Low-moderate importance: may be worth remembering"
        };
        Some(MemoryContent::importance_proposal(score, justification))
    }
}

impl Default for ImportanceScorer {
    fn default() -> Self {
        Self::new(ScoringConfig::default())
    }
}

/// Simple keyword extractor using frequency ranking
pub struct KeywordExtractor {
    stop_words: &'static [&'static str],
    min_word_len: usize,
    max_keywords: usize,
}

const STOP_WORDS: &[&str] = &[
    "the", "a", "an", "is", "are", "was", "were", "be", "been", "being", "have", "has", "had",
    "do", "does", "did", "will", "would", "could", "should", "may", "might", "must", "shall",
    "can", "to", "of", "in", "for", "on", "with", "at", "by", "from", "as", "into", "about",
    "like", "this", "that", "these", "those", "i", "you", "he", "she", "it", "we", "they", "what",
    "which", "who", "when", "where", "why", "how", "all", "and", "but", "or", "not", "so", "than",
    "very", "just", "also", "over", "up", "out", "if", "its", "our", "their", "my", "your",
];

impl Default for KeywordExtractor {
    fn default() -> Self {
        Self {
            stop_words: STOP_WORDS,
            min_word_len: 3,
            max_keywords: 5,
        }
    }
}

impl KeywordExtractor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn extract(&self, text: &str) -> Vec<String> {
        use std::collections::HashMap;
        let lower = text.to_lowercase();
        let mut freq: HashMap<&str, (usize, usize)> = HashMap::new();
        for (index, word) in lower.split(|c: char| !c.is_alphanumeric()).enumerate() {
            if word.len() >= self.min_word_len && !self.stop_words.contains(&word) {
                let entry = freq.entry(word).or_insert((0, index));
                entry.0 += 1;
            }
        }
        let mut pairs: Vec<_> = freq.into_iter().collect();
        pairs.sort_by(|left, right| {
            right
                .1
                 .0
                .cmp(&left.1 .0)
                .then_with(|| left.1 .1.cmp(&right.1 .1))
        });
        pairs
            .into_iter()
            .take(self.max_keywords)
            .map(|(w, _)| w.to_string())
            .collect()
    }

    pub fn to_retrieval_key(&self, text: &str) -> MemoryContent {
        MemoryContent::retrieval_key(self.extract(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scorer_important_beats_mundane() {
        let scorer = ImportanceScorer::default();
        let hi = scorer.score("Remember my birthday is March 15", true, 0);
        let lo = scorer.score("ok", false, 50);
        assert!(hi > lo, "hi={hi}, lo={lo}");
    }

    #[test]
    fn scorer_proposal_threshold() {
        let scorer = ImportanceScorer::default();
        assert!(scorer.propose("ok", false, 100).is_none());
        assert!(scorer
            .propose("I promise to always remember your name", true, 0)
            .is_some());
    }

    #[test]
    fn scorer_score_clamps_to_one() {
        let scorer = ImportanceScorer::default();
        let score = scorer.score(
            "important critical must always remember promise love hate fear goal dream",
            true,
            0,
        );
        assert!(score <= 1.0);
    }

    #[test]
    fn extractor_excludes_stop_words() {
        let ex = KeywordExtractor::new();
        let kws = ex.extract("The quick brown fox jumps over the lazy dog");
        assert!(!kws.contains(&"the".to_string()));
        assert!(!kws.contains(&"over".to_string()));
    }

    #[test]
    fn extractor_limits_output() {
        let ex = KeywordExtractor::new();
        let kws = ex.extract("apple banana cherry date elderberry fig grape honeydew");
        assert!(kws.len() <= 5);
    }

    #[test]
    fn extractor_produces_retrieval_key() {
        let ex = KeywordExtractor::new();
        let content = ex.to_retrieval_key("Alice loves chocolate cake");
        assert!(matches!(content, MemoryContent::RetrievalKey { .. }));
    }

    #[test]
    fn extractor_preserves_first_seen_order_for_equal_counts() {
        let ex = KeywordExtractor::new();
        let kws = ex.extract("koboldcpp launch failure ubuntu koboldcpp cuda launch");
        assert_eq!(kws[..4], ["koboldcpp", "launch", "failure", "ubuntu"]);
    }
}
