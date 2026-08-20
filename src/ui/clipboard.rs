use std::io::Write;

/// Copies text to the system clipboard using terminal OSC 52 escape sequences.
/// Works natively across local terminals, SSH sessions, tmux, and all modern OS terminal emulators.
pub fn copy_to_clipboard(text: &str) -> bool {
    let b64 = base64_encode(text.as_bytes());
    // OSC 52 sequence: ESC ] 52 ; c ; <base64> BEL
    let osc52 = format!("\x1b]52;c;{}\x07", b64);
    let mut stdout = std::io::stdout();
    if stdout.write_all(osc52.as_bytes()).is_ok() {
        let _ = stdout.flush();
        true
    } else {
        false
    }
}

/// Pure-Rust Base64 encoder (RFC 4648 standard)
pub fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);

        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(
            base64_encode(b"Hello from minicode"),
            "SGVsbG8gZnJvbSBtaW5pY29kZQ=="
        );
    }

    #[test]
    fn test_copy_to_clipboard() {
        let ok = copy_to_clipboard("test copy");
        assert!(ok);
    }
}
