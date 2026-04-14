//! Streaming parser for thinking/reasoning blocks in LLM output.
//!
//! Handles `<think>` ... `</think>` blocks that may arrive as partial UTF-8 chunks.

use bytes::{Buf, BytesMut};
use tokio_util::codec::Decoder;

/// Display mode for thinking blocks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThinkingDisplayMode {
    /// Hide thinking blocks entirely (production default)
    #[default]
    Hidden,
    /// Show collapsed indicator with expandable content
    Assisted,
    /// Show full thinking content inline (debug)
    Debug,
}

/// State machine for parsing thinking blocks
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ThinkingState {
    #[default]
    Normal,
    InThinkTag { depth: usize },
    InThinking,
    InCloseTag { depth: usize },
}

/// Output from the thinking decoder
#[derive(Debug, Clone, PartialEq)]
pub enum ThinkingOutput {
    /// Normal content (outside thinking blocks)
    Content(String),
    /// Thinking block content
    Thinking(String),
    /// Thinking block started
    ThinkingStart,
    /// Thinking block ended
    ThinkingEnd,
}

/// Decoder for streaming thinking blocks
pub struct ThinkingBlockDecoder {
    state: ThinkingState,
    display_mode: ThinkingDisplayMode,
    buffer: String,
    pending_start: bool,
    pending_end: bool,
}

impl ThinkingBlockDecoder {
    pub fn new(display_mode: ThinkingDisplayMode) -> Self {
        Self {
            state: ThinkingState::Normal,
            display_mode,
            buffer: String::new(),
            pending_start: false,
            pending_end: false,
        }
    }

    pub fn display_mode(&self) -> ThinkingDisplayMode {
        self.display_mode
    }

    /// Feed a string token and return all decoded outputs.
    ///
    /// This is the ergonomic streaming API: call once per incoming token
    /// and dispatch the resulting `ThinkingOutput` items.
    pub fn feed(&mut self, input: &str) -> Vec<ThinkingOutput> {
        let mut src = BytesMut::from(input.as_bytes());
        let mut outputs = Vec::new();
        while let Ok(Some(output)) = self.decode(&mut src) {
            outputs.push(output);
        }
        outputs
    }
}

impl Decoder for ThinkingBlockDecoder {
    type Item = ThinkingOutput;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Emit pending events first (in order: start, then end)
        if self.pending_start {
            self.pending_start = false;
            return Ok(Some(ThinkingOutput::ThinkingStart));
        }
        if self.pending_end {
            self.pending_end = false;
            return Ok(Some(ThinkingOutput::ThinkingEnd));
        }

        if src.is_empty() {
            return Ok(None);
        }

        // Convert to string, handling partial UTF-8
        let text = match std::str::from_utf8(src) {
            Ok(s) => s.to_string(),
            Err(e) => {
                let valid_up_to = e.valid_up_to();
                if valid_up_to == 0 {
                    return Ok(None); // Need more bytes
                }
                std::str::from_utf8(&src[..valid_up_to])
                    .map(|s| s.to_string())
                    .unwrap_or_default()
            }
        };

        // State machine implementation
        match &self.state {
            ThinkingState::Normal => {
                if let Some(pos) = text.find("<think>") {
                    let before = &text[..pos];
                    src.advance(pos + 7); // consume up to and including <think>
                    self.state = ThinkingState::InThinking;
                    if !before.is_empty() {
                        self.pending_start = true;
                        Ok(Some(ThinkingOutput::Content(before.to_string())))
                    } else {
                        Ok(Some(ThinkingOutput::ThinkingStart))
                    }
                } else if text.ends_with('<')
                    || text.ends_with("<t")
                    || text.ends_with("<th")
                    || text.ends_with("<thi")
                    || text.ends_with("<thin")
                    || text.ends_with("<think")
                {
                    // Potential partial tag at end, find where it starts
                    let partial_start = text.rfind('<').unwrap_or(text.len());
                    src.advance(partial_start);
                    if partial_start > 0 {
                        Ok(Some(ThinkingOutput::Content(
                            text[..partial_start].to_string(),
                        )))
                    } else {
                        Ok(None)
                    }
                } else {
                    let len = text.len();
                    src.advance(len);
                    Ok(Some(ThinkingOutput::Content(text)))
                }
            }
            ThinkingState::InThinking => {
                if let Some(pos) = text.find("</think>") {
                    let thinking_content = &text[..pos];
                    src.advance(pos + 8); // consume up to and including </think>
                    self.state = ThinkingState::Normal;
                    if !thinking_content.is_empty() {
                        self.buffer = thinking_content.to_string();
                        self.pending_end = true;
                        Ok(Some(ThinkingOutput::Thinking(thinking_content.to_string())))
                    } else {
                        Ok(Some(ThinkingOutput::ThinkingEnd))
                    }
                } else if text.ends_with('<')
                    || text.ends_with("</")
                    || text.ends_with("</t")
                    || text.ends_with("</th")
                    || text.ends_with("</thi")
                    || text.ends_with("</thin")
                    || text.ends_with("</think")
                {
                    // Potential partial close tag at end
                    let partial_start = text.rfind('<').unwrap_or(text.len());
                    src.advance(partial_start);
                    if partial_start > 0 {
                        Ok(Some(ThinkingOutput::Thinking(
                            text[..partial_start].to_string(),
                        )))
                    } else {
                        Ok(None)
                    }
                } else {
                    let len = text.len();
                    src.advance(len);
                    Ok(Some(ThinkingOutput::Thinking(text)))
                }
            }
            ThinkingState::InThinkTag { .. } | ThinkingState::InCloseTag { .. } => {
                // These states are reserved for future nested tag support
                let len = src.len();
                src.advance(len);
                Ok(Some(ThinkingOutput::Content(text)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_all(decoder: &mut ThinkingBlockDecoder, input: &str) -> Vec<ThinkingOutput> {
        let mut src = BytesMut::from(input);
        let mut results = Vec::new();
        while let Ok(Some(output)) = decoder.decode(&mut src) {
            results.push(output);
        }
        results
    }

    #[test]
    fn test_normal_content_passthrough() {
        let mut decoder = ThinkingBlockDecoder::new(ThinkingDisplayMode::Debug);
        let results = decode_all(&mut decoder, "Hello, world!");

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0],
            ThinkingOutput::Content("Hello, world!".to_string())
        );
    }

    #[test]
    fn test_complete_thinking_block() {
        let mut decoder = ThinkingBlockDecoder::new(ThinkingDisplayMode::Debug);
        let results = decode_all(&mut decoder, "<think>reasoning here</think>done");

        // Events: ThinkingStart, Thinking("reasoning here"), ThinkingEnd, Content("done")
        assert_eq!(results.len(), 4);
        assert_eq!(results[0], ThinkingOutput::ThinkingStart);
        assert_eq!(
            results[1],
            ThinkingOutput::Thinking("reasoning here".to_string())
        );
        assert_eq!(results[2], ThinkingOutput::ThinkingEnd);
        assert_eq!(results[3], ThinkingOutput::Content("done".to_string()));
    }

    #[test]
    fn test_content_before_thinking() {
        let mut decoder = ThinkingBlockDecoder::new(ThinkingDisplayMode::Debug);
        let results = decode_all(&mut decoder, "prefix<think>inner</think>");

        // Events: Content("prefix"), ThinkingStart, Thinking("inner"), ThinkingEnd
        assert_eq!(results.len(), 4);
        assert_eq!(results[0], ThinkingOutput::Content("prefix".to_string()));
        assert_eq!(results[1], ThinkingOutput::ThinkingStart);
        assert_eq!(results[2], ThinkingOutput::Thinking("inner".to_string()));
        assert_eq!(results[3], ThinkingOutput::ThinkingEnd);
    }

    #[test]
    fn test_partial_open_tag_at_boundary() {
        let mut decoder = ThinkingBlockDecoder::new(ThinkingDisplayMode::Debug);

        // First chunk ends with partial tag
        let mut src = BytesMut::from("Hello<thi");
        let result1 = decoder.decode(&mut src).unwrap();
        assert_eq!(result1, Some(ThinkingOutput::Content("Hello".to_string())));

        // Remaining partial tag should still be in buffer
        assert_eq!(src.as_ref(), b"<thi");

        // Add more data to complete the tag
        src.extend_from_slice(b"nk>inside</think>");
        let results: Vec<_> = std::iter::from_fn(|| decoder.decode(&mut src).ok().flatten())
            .collect();

        // Events: ThinkingStart, Thinking("inside"), ThinkingEnd
        assert_eq!(results.len(), 3);
        assert_eq!(results[0], ThinkingOutput::ThinkingStart);
        assert_eq!(
            results[1],
            ThinkingOutput::Thinking("inside".to_string())
        );
        assert_eq!(results[2], ThinkingOutput::ThinkingEnd);
    }

    #[test]
    fn test_partial_close_tag_at_boundary() {
        let mut decoder = ThinkingBlockDecoder::new(ThinkingDisplayMode::Debug);

        // Start with open tag
        let mut src = BytesMut::from("<think>content</thi");
        let result1 = decoder.decode(&mut src).unwrap();
        assert_eq!(result1, Some(ThinkingOutput::ThinkingStart));

        let result2 = decoder.decode(&mut src).unwrap();
        assert_eq!(
            result2,
            Some(ThinkingOutput::Thinking("content".to_string()))
        );

        // Partial close tag remains
        assert_eq!(src.as_ref(), b"</thi");

        // Complete the close tag
        src.extend_from_slice(b"nk>after");
        let results: Vec<_> = std::iter::from_fn(|| decoder.decode(&mut src).ok().flatten())
            .collect();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0], ThinkingOutput::ThinkingEnd);
        assert_eq!(results[1], ThinkingOutput::Content("after".to_string()));
    }

    #[test]
    fn test_empty_thinking_block() {
        let mut decoder = ThinkingBlockDecoder::new(ThinkingDisplayMode::Debug);
        let results = decode_all(&mut decoder, "<think></think>");

        assert_eq!(results.len(), 2);
        assert_eq!(results[0], ThinkingOutput::ThinkingStart);
        assert_eq!(results[1], ThinkingOutput::ThinkingEnd);
    }

    #[test]
    fn test_nested_content_no_crash() {
        // Nested tags aren't supported but shouldn't crash
        let mut decoder = ThinkingBlockDecoder::new(ThinkingDisplayMode::Debug);
        let results = decode_all(&mut decoder, "<think>outer<think>inner</think>more</think>");

        // Should parse as single thinking block (inner <think> treated as content)
        assert!(!results.is_empty());
        // Just verify no panic occurred
    }

    #[test]
    fn test_multiple_thinking_blocks() {
        let mut decoder = ThinkingBlockDecoder::new(ThinkingDisplayMode::Debug);
        let results = decode_all(&mut decoder, "<think>first</think>middle<think>second</think>");

        // Events: ThinkingStart, Thinking("first"), ThinkingEnd, Content("middle"),
        //         ThinkingStart, Thinking("second"), ThinkingEnd
        assert_eq!(results.len(), 7);
        assert_eq!(results[0], ThinkingOutput::ThinkingStart);
        assert_eq!(results[1], ThinkingOutput::Thinking("first".to_string()));
        assert_eq!(results[2], ThinkingOutput::ThinkingEnd);
        assert_eq!(results[3], ThinkingOutput::Content("middle".to_string()));
        assert_eq!(results[4], ThinkingOutput::ThinkingStart);
        assert_eq!(results[5], ThinkingOutput::Thinking("second".to_string()));
        assert_eq!(results[6], ThinkingOutput::ThinkingEnd);
    }

    #[test]
    fn test_display_mode_accessor() {
        let decoder = ThinkingBlockDecoder::new(ThinkingDisplayMode::Assisted);
        assert_eq!(decoder.display_mode(), ThinkingDisplayMode::Assisted);

        let decoder2 = ThinkingBlockDecoder::new(ThinkingDisplayMode::Hidden);
        assert_eq!(decoder2.display_mode(), ThinkingDisplayMode::Hidden);
    }

    #[test]
    fn test_angle_bracket_in_normal_content() {
        let mut decoder = ThinkingBlockDecoder::new(ThinkingDisplayMode::Debug);
        let results = decode_all(&mut decoder, "a < b and c > d");

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0],
            ThinkingOutput::Content("a < b and c > d".to_string())
        );
    }

    #[test]
    fn test_angle_bracket_in_thinking() {
        let mut decoder = ThinkingBlockDecoder::new(ThinkingDisplayMode::Debug);
        let results = decode_all(&mut decoder, "<think>if a < b then</think>");

        // Events: ThinkingStart, Thinking("if a < b then"), ThinkingEnd
        assert_eq!(results.len(), 3);
        assert_eq!(results[0], ThinkingOutput::ThinkingStart);
        assert_eq!(
            results[1],
            ThinkingOutput::Thinking("if a < b then".to_string())
        );
        assert_eq!(results[2], ThinkingOutput::ThinkingEnd);
    }
}
