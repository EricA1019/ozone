//! Streaming response types and the SSE/JSONLines decoder.
//!
//! `StreamDecoder` implements `tokio_util::codec::Decoder` so callers can
//! wrap any `AsyncRead` in a `tokio_util::codec::FramedRead` and get typed
//! `StreamChunk` items.

use bytes::{Buf, BytesMut};
use serde::{Deserialize, Serialize};
use tokio_util::codec::Decoder;

use crate::error::InferenceError;

// ---------------------------------------------------------------------------
// Streaming format
// ---------------------------------------------------------------------------

/// Wire format used by a backend for streaming responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamingFormat {
    /// Server-Sent Events (KoboldCpp, OpenAI-compatible).
    ServerSentEvents,
    /// Newline-delimited JSON (Ollama).
    JsonLines,
}

// ---------------------------------------------------------------------------
// Stream chunks
// ---------------------------------------------------------------------------

/// A typed item emitted by the streaming decoder.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamChunk {
    /// A token (or partial token) from the model.
    Token(String),
    /// The stream finished successfully.
    Done,
    /// The backend signalled a stop reason (e.g. `"stop"`, `"length"`).
    FinishReason(String),
}

// ---------------------------------------------------------------------------
// Internal SSE line deserialization helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// StreamDecoder
// ---------------------------------------------------------------------------

/// Stateful streaming decoder.
///
/// Handles both Server-Sent Events and newline-delimited JSON.
pub struct StreamDecoder {
    pub format: StreamingFormat,
    done: bool,
}

impl StreamDecoder {
    pub fn new(format: StreamingFormat) -> Self {
        Self {
            format,
            done: false,
        }
    }

    fn decode_sse_line(&self, line: &str) -> Option<StreamChunk> {
        // `data: [DONE]` terminates the stream.
        if line == "data: [DONE]" || line == "data:[DONE]" {
            return Some(StreamChunk::Done);
        }
        // Strip the `data: ` prefix.
        let payload = line
            .strip_prefix("data: ")
            .or_else(|| line.strip_prefix("data:"))?;

        // Parse to a generic value to detect format before committing.
        let v: serde_json::Value = serde_json::from_str(payload).ok()?;

        // KoboldCpp format: has a "token" key at the top level.
        if v.get("token").is_some() {
            let text = v["token"]["text"].as_str().unwrap_or("");
            // finish_reason is only meaningful as a non-null string.
            let finish = v["finish_reason"].as_str();
            if let Some(reason) = finish {
                return Some(StreamChunk::FinishReason(reason.to_string()));
            }
            if !text.is_empty() {
                return Some(StreamChunk::Token(text.to_string()));
            }
            return None;
        }

        // OpenAI chat-completions format: has a "choices" array.
        if let Some(choice) = v
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
        {
            // finish_reason is non-null when the stream is done.
            let finish = choice.get("finish_reason").and_then(|r| r.as_str());
            if let Some(reason) = finish {
                return Some(StreamChunk::FinishReason(reason.to_string()));
            }
            let content = choice
                .get("delta")
                .and_then(|d| d.get("content"))
                .and_then(|c| c.as_str())
                .unwrap_or("");
            if !content.is_empty() {
                return Some(StreamChunk::Token(content.to_string()));
            }
        }

        None
    }

    fn decode_jsonlines_line(&self, line: &str) -> Option<StreamChunk> {
        #[derive(Deserialize)]
        struct OllamaChunk {
            response: Option<String>,
            done: Option<bool>,
        }
        let chunk: OllamaChunk = serde_json::from_str(line).ok()?;
        if chunk.done == Some(true) {
            return Some(StreamChunk::Done);
        }
        chunk
            .response
            .filter(|s| !s.is_empty())
            .map(StreamChunk::Token)
    }
}

impl Decoder for StreamDecoder {
    type Item = StreamChunk;
    type Error = InferenceError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<StreamChunk>, InferenceError> {
        if self.done {
            return Ok(None);
        }

        loop {
            // Find the next newline in `src`.
            let newline_pos = src.iter().position(|&b| b == b'\n');
            let Some(pos) = newline_pos else {
                // No complete line yet — wait for more data.
                return Ok(None);
            };

            // Extract the line (without the newline).
            let raw = src.split_to(pos + 1);
            let line = std::str::from_utf8(&raw[..pos])
                .unwrap_or("")
                .trim_end_matches('\r');

            // Skip empty lines and SSE comment/event lines.
            if line.is_empty() || line.starts_with(':') || line.starts_with("event:") {
                continue;
            }

            let chunk = match self.format {
                StreamingFormat::ServerSentEvents => self.decode_sse_line(line),
                StreamingFormat::JsonLines => self.decode_jsonlines_line(line),
            };

            if let Some(chunk) = chunk {
                if chunk == StreamChunk::Done {
                    self.done = true;
                }
                return Ok(Some(chunk));
            }
            // Line parsed but produced no chunk (e.g. empty token) — continue.
        }
    }

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<StreamChunk>, InferenceError> {
        // Drain any remaining bytes as a final partial line.
        if !src.is_empty() {
            let remaining = std::str::from_utf8(&src[..])
                .unwrap_or("")
                .trim()
                .to_string();
            src.advance(src.len());
            if !remaining.is_empty() {
                let chunk = match self.format {
                    StreamingFormat::ServerSentEvents => self.decode_sse_line(&remaining),
                    StreamingFormat::JsonLines => self.decode_jsonlines_line(&remaining),
                };
                if let Some(c) = chunk {
                    return Ok(Some(c));
                }
            }
        }
        Ok(Some(StreamChunk::Done))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(decoder: &mut StreamDecoder, input: &str) -> Vec<StreamChunk> {
        let mut buf = BytesMut::from(input.as_bytes());
        let mut chunks = Vec::new();
        while let Some(chunk) = decoder.decode(&mut buf).unwrap() {
            chunks.push(chunk);
        }
        chunks
    }

    #[test]
    fn sse_koboldcpp_token_stream() {
        let mut dec = StreamDecoder::new(StreamingFormat::ServerSentEvents);
        let input = concat!(
            "data: {\"token\":{\"text\":\" Hello\"},\"finish_reason\":null}\n",
            "data: {\"token\":{\"text\":\" world\"},\"finish_reason\":null}\n",
            "data: {\"token\":{\"text\":\"\"},\"finish_reason\":\"stop\"}\n",
            "data: [DONE]\n",
        );
        let chunks = feed(&mut dec, input);
        assert_eq!(chunks[0], StreamChunk::Token(" Hello".into()));
        assert_eq!(chunks[1], StreamChunk::Token(" world".into()));
        assert_eq!(chunks[2], StreamChunk::FinishReason("stop".into()));
        assert_eq!(chunks[3], StreamChunk::Done);
    }

    #[test]
    fn sse_done_terminates() {
        let mut dec = StreamDecoder::new(StreamingFormat::ServerSentEvents);
        let input = "data: [DONE]\n";
        let chunks = feed(&mut dec, input);
        assert_eq!(chunks, vec![StreamChunk::Done]);
    }

    #[test]
    fn sse_empty_lines_skipped() {
        let mut dec = StreamDecoder::new(StreamingFormat::ServerSentEvents);
        let input = concat!(
            "\n",
            "event: message\n",
            "data: {\"token\":{\"text\":\"hi\"},\"finish_reason\":null}\n",
            "\n",
            "data: [DONE]\n",
        );
        let chunks = feed(&mut dec, input);
        assert_eq!(chunks[0], StreamChunk::Token("hi".into()));
        assert_eq!(chunks[1], StreamChunk::Done);
    }

    #[test]
    fn sse_openai_format() {
        let mut dec = StreamDecoder::new(StreamingFormat::ServerSentEvents);
        let input = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\" world\"},\"finish_reason\":null}]}\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n",
            "data: [DONE]\n",
        );
        let chunks = feed(&mut dec, input);
        assert_eq!(chunks[0], StreamChunk::Token("Hello".into()));
        assert_eq!(chunks[1], StreamChunk::Token(" world".into()));
        assert_eq!(chunks[2], StreamChunk::FinishReason("stop".into()));
        assert_eq!(chunks[3], StreamChunk::Done);
    }

    #[test]
    fn jsonlines_ollama_format() {
        let mut dec = StreamDecoder::new(StreamingFormat::JsonLines);
        let input = concat!(
            "{\"response\":\"Hello\",\"done\":false}\n",
            "{\"response\":\" world\",\"done\":false}\n",
            "{\"response\":\"\",\"done\":true}\n",
        );
        let chunks = feed(&mut dec, input);
        assert_eq!(chunks[0], StreamChunk::Token("Hello".into()));
        assert_eq!(chunks[1], StreamChunk::Token(" world".into()));
        assert_eq!(chunks[2], StreamChunk::Done);
    }

    #[test]
    fn partial_line_waits_for_newline() {
        let mut dec = StreamDecoder::new(StreamingFormat::ServerSentEvents);
        // Feed incomplete line.
        let mut buf = BytesMut::from("data: {\"token\":{\"text\":\"he".as_bytes());
        assert_eq!(dec.decode(&mut buf).unwrap(), None);
        // Complete the line.
        buf.extend_from_slice("llo\"},\"finish_reason\":null}\n".as_bytes());
        let chunk = dec.decode(&mut buf).unwrap();
        assert_eq!(chunk, Some(StreamChunk::Token("hello".into())));
    }
}
