use std::io::Write;
#[cfg(not(test))]
use std::process::{Command, Stdio};

/// Copies text to the system clipboard using native desktop utilities with OSC 52 fallback.
/// On Linux: tries `wl-copy` (Wayland) or `xclip -selection clipboard` (X11).
/// On macOS: tries `pbcopy`.
/// On Windows: tries `powershell -command Set-Clipboard`.
/// Always also sends the terminal OSC 52 sequence so remote SSH / tmux sessions work.
pub fn copy_to_clipboard(text: &str) -> bool {
    #[allow(unused_mut)]
    let mut native_copied = false;

    #[cfg(all(target_os = "linux", not(test)))]
    {
        // 1. Try wl-copy first if on Wayland
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            if let Ok(mut child) = Command::new("wl-copy")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(text.as_bytes());
                    drop(stdin);
                }
                if child.wait().map(|s| s.success()).unwrap_or(false) {
                    native_copied = true;
                }
            }
        }

        // 2. Also copy to xclip if DISPLAY is present or as fallback if Wayland was unavailable
        if std::env::var_os("DISPLAY").is_some()
            || (!native_copied && std::env::var_os("WAYLAND_DISPLAY").is_none())
        {
            if let Ok(mut child) = Command::new("xclip")
                .args(["-selection", "clipboard"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(text.as_bytes());
                    drop(stdin);
                }
                if child.wait().map(|s| s.success()).unwrap_or(false) {
                    native_copied = true;
                }
            }

            // Also copy to primary selection for X11 middle-click paste
            if let Ok(mut child) = Command::new("xclip")
                .args(["-selection", "primary"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(text.as_bytes());
                    drop(stdin);
                }
                let _ = child.wait();
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(mut child) = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
                drop(stdin);
            }
            if child.wait().map(|s| s.success()).unwrap_or(false) {
                native_copied = true;
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(mut child) = Command::new("powershell")
            .args(["-NoProfile", "-Command", "Set-Clipboard", "-Value", text])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            if child.wait().map(|s| s.success()).unwrap_or(false) {
                native_copied = true;
            }
        }
    }

    // Always emit OSC 52 escape sequences for SSH, tmux, and terminals that support it
    let b64 = base64_encode(text.as_bytes());
    let osc52 = format!("\x1b]52;c;{}\x07", b64);
    let mut stdout = std::io::stdout();
    let osc52_sent = if stdout.write_all(osc52.as_bytes()).is_ok() {
        let _ = stdout.flush();
        true
    } else {
        false
    };

    native_copied || osc52_sent
}

/// Result of reading the system clipboard on `Ctrl+V` / `Alt+V`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardPasteResult {
    /// Raw image binary read from the system clipboard (or copied image file)
    Image {
        bytes: Vec<u8>,
        media_type: String,
        ext: String,
        dimensions: Option<(u32, u32)>,
        source_path: Option<std::path::PathBuf>,
    },
    /// Plain text read from the system clipboard
    Text(String),
}

/// Inspects raw bytes for PNG, JPEG, GIF, or WebP magic headers and extracts `(media_type, ext, dimensions)`.
pub fn inspect_image_bytes(
    bytes: &[u8],
) -> Option<(&'static str, &'static str, Option<(u32, u32)>)> {
    if bytes.len() >= 24 && &bytes[0..8] == b"\x89PNG\r\n\x1a\n" {
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        let dims = if w > 0 && h > 0 { Some((w, h)) } else { None };
        return Some(("image/png", "png", dims));
    }

    if bytes.len() >= 10 && (&bytes[0..6] == b"GIF87a" || &bytes[0..6] == b"GIF89a") {
        let w = u16::from_le_bytes([bytes[6], bytes[7]]) as u32;
        let h = u16::from_le_bytes([bytes[8], bytes[9]]) as u32;
        let dims = if w > 0 && h > 0 { Some((w, h)) } else { None };
        return Some(("image/gif", "gif", dims));
    }

    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        let dims = if bytes.len() >= 30 && &bytes[12..16] == b"VP8X" {
            let w = 1 + u32::from_le_bytes([bytes[24], bytes[25], bytes[26], 0]);
            let h = 1 + u32::from_le_bytes([bytes[27], bytes[28], bytes[29], 0]);
            Some((w, h))
        } else if bytes.len() >= 30 && &bytes[12..16] == b"VP8 " {
            let w = (u16::from_le_bytes([bytes[26], bytes[27]]) & 0x3FFF) as u32;
            let h = (u16::from_le_bytes([bytes[28], bytes[29]]) & 0x3FFF) as u32;
            if w > 0 && h > 0 {
                Some((w, h))
            } else {
                None
            }
        } else {
            None
        };
        return Some(("image/webp", "webp", dims));
    }

    if bytes.len() >= 4 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        let mut idx = 2;
        let mut dims = None;
        while idx + 8 < bytes.len() {
            if bytes[idx] != 0xFF {
                idx += 1;
                continue;
            }
            let marker = bytes[idx + 1];
            if marker == 0xC0 || marker == 0xC2 {
                let h = u16::from_be_bytes([bytes[idx + 5], bytes[idx + 6]]) as u32;
                let w = u16::from_be_bytes([bytes[idx + 7], bytes[idx + 8]]) as u32;
                if w > 0 && h > 0 {
                    dims = Some((w, h));
                }
                break;
            }
            if marker == 0xD8 || marker == 0xD9 || (0xD0..=0xD7).contains(&marker) {
                idx += 2;
            } else {
                let seg_len = u16::from_be_bytes([bytes[idx + 2], bytes[idx + 3]]) as usize;
                if seg_len < 2 {
                    break;
                }
                idx += 2 + seg_len;
            }
        }
        return Some(("image/jpeg", "jpg", dims));
    }

    None
}

/// Decodes basic `%XX` URL percent-encoding in `file://` URIs from drag-and-drop or file manager copy.
fn percent_decode_path(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push(((h << 4) | l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Checks if a pasted string (e.g. from drag-and-drop, `file://` URI, or copied path) points to a valid image file on disk.
pub fn resolve_pasted_image_path(
    raw: &str,
    workspace_root: &std::path::Path,
) -> Option<std::path::PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.lines().count() > 2 {
        return None;
    }
    let first_line = trimmed.lines().next()?.trim();
    let unquoted = first_line.trim_matches(|c| c == '\'' || c == '"').trim();
    let stripped = if let Some(rest) = unquoted.strip_prefix("file://localhost") {
        percent_decode_path(rest)
    } else if let Some(rest) = unquoted.strip_prefix("file://") {
        percent_decode_path(rest)
    } else {
        // Also handle shell-escaped spaces from terminal drag-and-drop (e.g., `/path/my\ image.png`)
        unquoted.replace("\\ ", " ")
    };

    let lower = stripped.to_lowercase();
    let has_img_ext = lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".gif")
        || lower.ends_with(".webp");
    if !has_img_ext {
        return None;
    }

    let p = std::path::Path::new(&stripped);
    let resolved = if p.is_absolute() {
        p.to_path_buf()
    } else {
        workspace_root.join(p)
    };
    if resolved.is_file() {
        Some(resolved)
    } else {
        None
    }
}

/// Reads an image or text from the OS clipboard (`Ctrl+V` / `Alt+V`).
/// Checks for binary image targets (`image/png`, `image/jpeg`, `image/webp`, `image/gif`) first,
/// then checks if a copied file URI points to an image, and finally falls back to plain text.
pub fn read_from_clipboard(workspace_root: &std::path::Path) -> Option<ClipboardPasteResult> {
    #[cfg(all(target_os = "linux", not(test)))]
    {
        // 1. Wayland (`wl-paste`)
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            if let Ok(out) = Command::new("wl-paste")
                .arg("--list-types")
                .stderr(Stdio::null())
                .output()
            {
                if out.status.success() {
                    let types = String::from_utf8_lossy(&out.stdout);
                    for mime in ["image/png", "image/webp", "image/jpeg", "image/gif"] {
                        if types.lines().any(|l| l.trim() == mime) {
                            if let Ok(img_out) = Command::new("wl-paste")
                                .args(["--type", mime])
                                .stderr(Stdio::null())
                                .output()
                            {
                                if img_out.status.success() && !img_out.stdout.is_empty() {
                                    if let Some((m, ext, dims)) =
                                        inspect_image_bytes(&img_out.stdout)
                                    {
                                        return Some(ClipboardPasteResult::Image {
                                            bytes: img_out.stdout,
                                            media_type: m.to_string(),
                                            ext: ext.to_string(),
                                            dimensions: dims,
                                            source_path: None,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    // Check text/uri-list for copied image file in file manager
                    if types.lines().any(|l| l.trim() == "text/uri-list") {
                        if let Ok(uri_out) = Command::new("wl-paste")
                            .args(["--type", "text/uri-list"])
                            .stderr(Stdio::null())
                            .output()
                        {
                            if uri_out.status.success() {
                                let uri_str = String::from_utf8_lossy(&uri_out.stdout);
                                if let Some(path) =
                                    resolve_pasted_image_path(&uri_str, workspace_root)
                                {
                                    if let Ok(bytes) = std::fs::read(&path) {
                                        if let Some((m, ext, dims)) = inspect_image_bytes(&bytes) {
                                            return Some(ClipboardPasteResult::Image {
                                                bytes,
                                                media_type: m.to_string(),
                                                ext: ext.to_string(),
                                                dimensions: dims,
                                                source_path: Some(path),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 2. X11 (`xclip`)
        if std::env::var_os("DISPLAY").is_some() {
            if let Ok(out) = Command::new("xclip")
                .args(["-selection", "clipboard", "-t", "TARGETS", "-o"])
                .stderr(Stdio::null())
                .output()
            {
                if out.status.success() {
                    let targets = String::from_utf8_lossy(&out.stdout);
                    for mime in ["image/png", "image/webp", "image/jpeg", "image/gif"] {
                        if targets.lines().any(|l| l.trim() == mime) {
                            if let Ok(img_out) = Command::new("xclip")
                                .args(["-selection", "clipboard", "-t", mime, "-o"])
                                .stderr(Stdio::null())
                                .output()
                            {
                                if img_out.status.success() && !img_out.stdout.is_empty() {
                                    if let Some((m, ext, dims)) =
                                        inspect_image_bytes(&img_out.stdout)
                                    {
                                        return Some(ClipboardPasteResult::Image {
                                            bytes: img_out.stdout,
                                            media_type: m.to_string(),
                                            ext: ext.to_string(),
                                            dimensions: dims,
                                            source_path: None,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    if targets.lines().any(|l| l.trim() == "text/uri-list") {
                        if let Ok(uri_out) = Command::new("xclip")
                            .args(["-selection", "clipboard", "-t", "text/uri-list", "-o"])
                            .stderr(Stdio::null())
                            .output()
                        {
                            if uri_out.status.success() {
                                let uri_str = String::from_utf8_lossy(&uri_out.stdout);
                                if let Some(path) =
                                    resolve_pasted_image_path(&uri_str, workspace_root)
                                {
                                    if let Ok(bytes) = std::fs::read(&path) {
                                        if let Some((m, ext, dims)) = inspect_image_bytes(&bytes) {
                                            return Some(ClipboardPasteResult::Image {
                                                bytes,
                                                media_type: m.to_string(),
                                                ext: ext.to_string(),
                                                dimensions: dims,
                                                source_path: Some(path),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 3. Fallback to reading text from Wayland or X11
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            if let Ok(out) = Command::new("wl-paste")
                .arg("--no-newline")
                .stderr(Stdio::null())
                .output()
            {
                if out.status.success() && !out.stdout.is_empty() {
                    let text = String::from_utf8_lossy(&out.stdout).into_owned();
                    if let Some(path) = resolve_pasted_image_path(&text, workspace_root) {
                        if let Ok(bytes) = std::fs::read(&path) {
                            if let Some((m, ext, dims)) = inspect_image_bytes(&bytes) {
                                return Some(ClipboardPasteResult::Image {
                                    bytes,
                                    media_type: m.to_string(),
                                    ext: ext.to_string(),
                                    dimensions: dims,
                                    source_path: Some(path),
                                });
                            }
                        }
                    }
                    return Some(ClipboardPasteResult::Text(text));
                }
            }
        }
        if std::env::var_os("DISPLAY").is_some() {
            if let Ok(out) = Command::new("xclip")
                .args(["-selection", "clipboard", "-o"])
                .stderr(Stdio::null())
                .output()
            {
                if out.status.success() && !out.stdout.is_empty() {
                    let text = String::from_utf8_lossy(&out.stdout).into_owned();
                    if let Some(path) = resolve_pasted_image_path(&text, workspace_root) {
                        if let Ok(bytes) = std::fs::read(&path) {
                            if let Some((m, ext, dims)) = inspect_image_bytes(&bytes) {
                                return Some(ClipboardPasteResult::Image {
                                    bytes,
                                    media_type: m.to_string(),
                                    ext: ext.to_string(),
                                    dimensions: dims,
                                    source_path: Some(path),
                                });
                            }
                        }
                    }
                    return Some(ClipboardPasteResult::Text(text));
                }
            }
        }
    }

    #[cfg(all(target_os = "macos", not(test)))]
    {
        if let Ok(out) = Command::new("pngpaste")
            .arg("-")
            .stderr(Stdio::null())
            .output()
        {
            if out.status.success() && !out.stdout.is_empty() {
                if let Some((m, ext, dims)) = inspect_image_bytes(&out.stdout) {
                    return Some(ClipboardPasteResult::Image {
                        bytes: out.stdout,
                        media_type: m.to_string(),
                        ext: ext.to_string(),
                        dimensions: dims,
                        source_path: None,
                    });
                }
            }
        }
        if let Ok(out) = Command::new("pbpaste").stderr(Stdio::null()).output() {
            if out.status.success() && !out.stdout.is_empty() {
                let text = String::from_utf8_lossy(&out.stdout).into_owned();
                if let Some(path) = resolve_pasted_image_path(&text, workspace_root) {
                    if let Ok(bytes) = std::fs::read(&path) {
                        if let Some((m, ext, dims)) = inspect_image_bytes(&bytes) {
                            return Some(ClipboardPasteResult::Image {
                                bytes,
                                media_type: m.to_string(),
                                ext: ext.to_string(),
                                dimensions: dims,
                                source_path: Some(path),
                            });
                        }
                    }
                }
                return Some(ClipboardPasteResult::Text(text));
            }
        }
    }

    #[cfg(test)]
    let _ = workspace_root;

    None
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

    #[test]
    fn test_inspect_image_bytes_and_resolve_path() {
        let png_bytes: [u8; 32] = [
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H',
            b'D', b'R', 0x00, 0x00, 0x07, 0x80, // 1920
            0x00, 0x00, 0x04, 0x38, // 1080
            0x08, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let (mime, ext, dims) = inspect_image_bytes(&png_bytes).unwrap();
        assert_eq!(mime, "image/png");
        assert_eq!(ext, "png");
        assert_eq!(dims, Some((1920, 1080)));

        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("my screenshot.png");
        std::fs::write(&file_path, png_bytes).unwrap();

        let uri = format!("file://{}", file_path.to_string_lossy().replace(' ', "%20"));
        let resolved = resolve_pasted_image_path(&uri, dir.path());
        assert_eq!(resolved, Some(file_path));
    }
}
