use std::io::{BufRead, Write};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::JSONRPC_VERSION;

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub(super) jsonrpc: String,
    #[serde(default)]
    pub(super) id: Option<Value>,
    pub(super) method: String,
    #[serde(default)]
    pub(super) params: Option<Value>,
}

pub(super) fn read_message(reader: &mut impl BufRead) -> Result<Option<JsonRpcRequest>> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            return Ok(None);
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .context("invalid Content-Length header")?,
            );
        }
    }

    let content_length = content_length.ok_or_else(|| anyhow!("missing Content-Length header"))?;
    let mut payload = vec![0_u8; content_length];
    reader.read_exact(&mut payload)?;
    Ok(Some(
        serde_json::from_slice::<JsonRpcRequest>(&payload)
            .context("failed to parse JSON-RPC request body")?,
    ))
}

pub(super) fn write_message(writer: &mut impl Write, value: &Value) -> Result<()> {
    let payload = serde_json::to_vec(value)?;
    write!(writer, "Content-Length: {}\r\n\r\n", payload.len())?;
    writer.write_all(&payload)?;
    Ok(())
}

pub(super) fn success_response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": JSONRPC_VERSION,
        "id": id,
        "result": result
    })
}

pub(super) fn error_response(id: Value, code: i64, message: String) -> Value {
    json!({
        "jsonrpc": JSONRPC_VERSION,
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

#[cfg(test)]
mod tests {
    use std::io::{BufReader, Cursor};

    use serde_json::{json, Value};

    use super::{read_message, write_message};

    #[test]
    fn jsonrpc_read_message_parses_content_length_framing() {
        let payload = br#"{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}"#;
        let mut framed = format!("Content-Length: {}\r\n\r\n", payload.len()).into_bytes();
        framed.extend_from_slice(payload);
        let mut reader = BufReader::new(Cursor::new(framed));

        let request = read_message(&mut reader)
            .expect("parse frame")
            .expect("request");

        assert_eq!(request.jsonrpc, "2.0");
        assert_eq!(request.method, "ping");
    }

    #[test]
    fn jsonrpc_write_message_prefixes_content_length_header() {
        let value = json!({
            "jsonrpc": "2.0",
            "id": 7,
            "result": {"ok": true}
        });
        let expected_payload = serde_json::to_vec(&value).expect("payload");
        let mut output = Vec::new();

        write_message(&mut output, &value).expect("write frame");

        let prefix = format!("Content-Length: {}\r\n\r\n", expected_payload.len());
        assert!(output.starts_with(prefix.as_bytes()));
        let payload = &output[prefix.len()..];
        let decoded: Value = serde_json::from_slice(payload).expect("decode payload");
        assert_eq!(decoded, value);
    }
}
