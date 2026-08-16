use anyhow::{anyhow, Result};
use serde_json::Value;

/// Encodes a JSON-RPC payload with the standard LSP `Content-Length:` header.
pub fn encode_message(payload: &Value) -> Result<Vec<u8>> {
    let json_bytes = serde_json::to_vec(payload)?;
    let header = format!("Content-Length: {}\r\n\r\n", json_bytes.len());
    let mut out = Vec::with_capacity(header.len() + json_bytes.len());
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(&json_bytes);
    Ok(out)
}

/// Attempts to decode a single framed JSON-RPC message from a byte buffer.
/// Returns `Ok(Some((Value, bytes_consumed)))` if a complete frame exists.
pub fn decode_message(buffer: &[u8]) -> Result<Option<(Value, usize)>> {
    let text = match std::str::from_utf8(buffer) {
        Ok(t) => t,
        Err(_) => return Ok(None),
    };

    let header_end = match text.find("\r\n\r\n") {
        Some(pos) => pos,
        None => return Ok(None),
    };

    let header_part = &text[..header_end];
    let mut content_length: Option<usize> = None;

    for line in header_part.lines() {
        if let Some(stripped) = line.strip_prefix("Content-Length:") {
            if let Ok(len) = stripped.trim().parse::<usize>() {
                content_length = Some(len);
                break;
            }
        }
    }

    let content_len = match content_length {
        Some(len) => len,
        None => {
            return Err(anyhow!(
                "Missing or invalid Content-Length header in LSP frame"
            ))
        }
    };

    let body_start = header_end + 4;
    let total_required = body_start + content_len;

    if buffer.len() < total_required {
        return Ok(None); // Need more bytes
    }

    let body_bytes = &buffer[body_start..total_required];
    let val: Value = serde_json::from_slice(body_bytes)?;

    Ok(Some((val, total_required)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_encode_and_decode_roundtrip() {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": 1234
            }
        });

        let encoded = encode_message(&payload).unwrap();
        assert!(encoded.starts_with(b"Content-Length: "));

        let decoded = decode_message(&encoded).unwrap();
        assert!(decoded.is_some());
        let (val, consumed) = decoded.unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(val["id"], 1);
        assert_eq!(val["method"], "initialize");
    }

    #[test]
    fn test_partial_buffer_returns_none() {
        let payload = json!({ "key": "value" });
        let encoded = encode_message(&payload).unwrap();

        // Truncate buffer
        let partial = &encoded[..encoded.len() - 5];
        let decoded = decode_message(partial).unwrap();
        assert!(decoded.is_none());
    }
}
