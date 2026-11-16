//! Pure-Rust Perceptual `dHash` (`--if-changed`), Set-of-Mark (`[e1]`) Pixel Annotation,
//! and Pixel-by-Pixel Visual Regression Diffing (`diff_screenshot` / `diff_url`).
//!
//! Operates directly on Cairo `ARGB32` (`[B, G, R, A]` on little-endian) or synthesized frame
//! buffers in `< 2 ms` with zero external image or font crate dependencies.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotationBox {
    pub label: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Debug, Clone)]
pub struct RawFrame {
    pub width: u32,
    pub height: u32,
    /// Tightly packed `width * height * 4` bytes in `[B, G, R, A]` order.
    pub bgra: Vec<u8>,
    pub dhash: u64,
}

#[derive(Debug, Clone)]
pub struct VisualDiffOutcome {
    pub width: u32,
    pub height: u32,
    pub mismatch_pixels: u64,
    pub total_pixels: u64,
    pub mismatch_pct: f64,
    pub dhash_distance: u32,
    pub changed_regions: Vec<Value>,
    pub diff_png: Vec<u8>,
}

/// Computes a 64-bit horizontal-gradient perceptual hash (`dHash`) over a `9x8` box-sampled
/// luminance grid from a `[B, G, R, A]` buffer with arbitrary row `stride`.
#[must_use]
pub fn compute_bgra_dhash64(bgra: &[u8], width: u32, height: u32, stride: usize) -> u64 {
    if width == 0 || height == 0 || bgra.len() < (height as usize).saturating_mul(stride) {
        return 0;
    }
    let mut grid = [[0u32; 9]; 8];
    for (gy, row_cells) in grid.iter_mut().enumerate() {
        let y0 = (gy as u32 * height) / 8;
        let y1 = (((gy as u32 + 1) * height) / 8).max(y0 + 1).min(height);
        let y_step = ((y1 - y0) / 4).max(1);
        for (gx, cell) in row_cells.iter_mut().enumerate() {
            let x0 = (gx as u32 * width) / 9;
            let x1 = (((gx as u32 + 1) * width) / 9).max(x0 + 1).min(width);
            let x_step = ((x1 - x0) / 4).max(1);
            let mut sum = 0u64;
            let mut count = 0u64;
            let mut y = y0;
            while y < y1 {
                let row_off = y as usize * stride;
                let mut x = x0;
                while x < x1 {
                    let idx = row_off + (x as usize * 4);
                    if idx + 2 < bgra.len() {
                        let b = u64::from(bgra[idx]);
                        let g = u64::from(bgra[idx + 1]);
                        let r = u64::from(bgra[idx + 2]);
                        sum += (77 * r + 150 * g + 29 * b) >> 8;
                        count += 1;
                    }
                    x += x_step;
                }
                y += y_step;
            }
            *cell = sum.checked_div(count).unwrap_or(0) as u32;
        }
    }

    let mut hash = 0u64;
    let mut bit = 0u32;
    for row in &grid {
        for col in 0..8 {
            if row[col] > row[col + 1] {
                hash |= 1u64 << bit;
            }
            bit += 1;
        }
    }
    hash
}

#[must_use]
pub const fn hamming_distance64(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// Copies a strided Cairo `ARGB32` buffer into a tightly packed `RawFrame`.
#[must_use]
pub fn pack_bgra_frame(src: &[u8], width: u32, height: u32, stride: usize) -> RawFrame {
    let row_bytes = (width as usize) * 4;
    let mut packed = vec![0u8; row_bytes * (height as usize)];
    for y in 0..(height as usize) {
        let s_off = y * stride;
        let d_off = y * row_bytes;
        if s_off + row_bytes <= src.len() {
            packed[d_off..d_off + row_bytes].copy_from_slice(&src[s_off..s_off + row_bytes]);
        }
    }
    let dhash = compute_bgra_dhash64(&packed, width, height, row_bytes);
    RawFrame {
        width,
        height,
        bgra: packed,
        dhash,
    }
}

///Synthesizes a deterministic Gear-0 `RawFrame` representing a DOM layout when WebKit is asleep.
#[must_use]
pub fn synthesize_gear0_frame(
    width: u32,
    height: u32,
    boxes: &[AnnotationBox],
    seed_text: &str,
) -> RawFrame {
    let w = width.clamp(64, 1920);
    let h = height.clamp(64, 1200);
    let stride = (w as usize) * 4;
    let mut bgra = vec![0u8; stride * (h as usize)];
    // Fill dark slate canvas + deterministic 8-tile content-signature header hashed from `seed_text`
    let mut fnv: u64 = 0xcbf2_9ce4_8422_2325;
    for b in seed_text.bytes() {
        fnv ^= u64::from(b);
        fnv = fnv.wrapping_mul(0x0100_0000_01b3);
    }
    for y in 0..(h as usize) {
        for x in 0..(w as usize) {
            let (r, g, b) = if y < 28 {
                let sector = ((x * 8) / (w as usize)).min(7);
                let byte_val = ((fnv >> (sector * 8)) & 0xFF) as u8;
                (
                    byte_val.wrapping_mul(3) | 0x40,
                    byte_val.rotate_left(3) | 0x30,
                    byte_val.rotate_right(2) | 0x50,
                )
            } else {
                (0x16, 0x15, 0x1E)
            };
            let off = y * stride + x * 4;
            bgra[off] = b;
            bgra[off + 1] = g;
            bgra[off + 2] = r;
            bgra[off + 3] = 0xFF;
        }
    }
    draw_annotations_bgra(&mut bgra, w, h, stride, boxes);
    let dhash = compute_bgra_dhash64(&bgra, w, h, stride);
    RawFrame {
        width: w,
        height: h,
        bgra,
        dhash,
    }
}

/// Draws Set-of-Mark (`[e1]`, `[e2]`) bounding boxes and crisp `5x7` bitmap callout badges
/// directly onto a `[B, G, R, A]` pixel buffer without mutating the live DOM.
pub fn draw_annotations_bgra(
    bgra: &mut [u8],
    width: u32,
    height: u32,
    stride: usize,
    boxes: &[AnnotationBox],
) {
    const PALETTE: &[(u8, u8, u8)] = &[
        (0xFF, 0x2E, 0x88), // Neon Rose
        (0x00, 0xD2, 0xFF), // Electric Cyan
        (0xFF, 0xB8, 0x00), // Amber Gold
        (0x34, 0xD3, 0x99), // Emerald
    ];
    let w_i32 = width as i32;
    let h_i32 = height as i32;
    if w_i32 <= 4 || h_i32 <= 4 {
        return;
    }

    for (idx, item) in boxes.iter().take(60).enumerate() {
        if item.w <= 2 || item.h <= 2 {
            continue;
        }
        let x0 = item.x.clamp(0, w_i32 - 2);
        let y0 = item.y.clamp(0, h_i32 - 2);
        let x1 = (item.x + item.w).clamp(x0 + 2, w_i32 - 1);
        let y1 = (item.y + item.h).clamp(y0 + 2, h_i32 - 1);
        let (r, g, b) = PALETTE[idx % PALETTE.len()];

        // 2px border outline
        for t in 0..2 {
            draw_hline(bgra, stride, w_i32, h_i32, x0, x1, y0 + t, (r, g, b));
            draw_hline(bgra, stride, w_i32, h_i32, x0, x1, y1 - t, (r, g, b));
            draw_vline(bgra, stride, w_i32, h_i32, x0 + t, y0, y1, (r, g, b));
            draw_vline(bgra, stride, w_i32, h_i32, x1 - t, y0, y1, (r, g, b));
        }

        // Tag pill `[eN]` placed just above or inside top-left corner
        let tag_txt = if item.label.is_empty() {
            format!("e{}", idx + 1)
        } else {
            item.label.trim_start_matches('@').to_string()
        };
        let char_count = tag_txt.chars().take(8).count() as i32;
        let pill_w = (char_count * 6 + 5).min(w_i32 - 2);
        let pill_h = 11;
        let pill_x = x0.clamp(0, (w_i32 - pill_w).max(0));
        let pill_y = if y0 >= pill_h { y0 - pill_h } else { y0 };

        fill_rect(
            bgra,
            stride,
            w_i32,
            h_i32,
            pill_x,
            pill_y,
            pill_w,
            pill_h,
            (r, g, b),
        );
        let mut cursor_x = pill_x + 3;
        for ch in tag_txt.chars().take(8) {
            draw_glyph_5x7(
                bgra,
                stride,
                w_i32,
                h_i32,
                cursor_x,
                pill_y + 2,
                ch,
                (0x0B, 0x0C, 0x10),
            );
            cursor_x += 6;
        }
    }
}

/// Compares two `RawFrame`s pixel-by-pixel, identifying changed `8x8` layout sectors and
/// encoding a red-highlighted visual diff PNG.
#[must_use]
pub fn compare_frames(base: &RawFrame, curr: &RawFrame) -> VisualDiffOutcome {
    let out_w = base.width.max(curr.width).max(1);
    let out_h = base.height.max(curr.height).max(1);
    let total_pixels = u64::from(out_w) * u64::from(out_h);
    let mut mismatch_pixels = 0u64;
    let scale = out_w.div_ceil(640).max(1);
    let prev_w = out_w.div_ceil(scale).max(1);
    let prev_h = out_h.div_ceil(scale).max(1);
    let mut rgb_diff = vec![0u8; (prev_w as usize) * (prev_h as usize) * 3];

    let mut tile_hits = [[0u32; 8]; 8];
    for y in 0..out_h {
        let py = (y / scale).min(prev_h - 1) as usize;
        for x in 0..out_w {
            let b_px = sample_bgra(base, x, y);
            let c_px = sample_bgra(curr, x, y);
            let delta = u32::from(b_px.0.abs_diff(c_px.0))
                + u32::from(b_px.1.abs_diff(c_px.1))
                + u32::from(b_px.2.abs_diff(c_px.2));
            let px = (x / scale).min(prev_w - 1) as usize;
            let dst = (py * (prev_w as usize) + px) * 3;
            if delta > 24 {
                mismatch_pixels += 1;
                let tx = ((x * 8) / out_w).min(7) as usize;
                let ty = ((y * 8) / out_h).min(7) as usize;
                tile_hits[ty][tx] += 1;
                // High-visibility crimson highlight always wins when downsampling
                rgb_diff[dst] = 0xFF;
                rgb_diff[dst + 1] = c_px.1 / 4;
                rgb_diff[dst + 2] = c_px.0 / 4;
            } else if rgb_diff[dst] != 0xFF {
                // Dimmed grayscale context so red diff regions pop immediately
                let luma =
                    ((77 * u32::from(c_px.2) + 150 * u32::from(c_px.1) + 29 * u32::from(c_px.0))
                        >> 8) as u8;
                let dimmed = (u16::from(luma) * 2 / 3) as u8;
                rgb_diff[dst] = dimmed;
                rgb_diff[dst + 1] = dimmed;
                rgb_diff[dst + 2] = dimmed;
            }
        }
    }

    let tile_area = ((out_w / 8).max(1) * (out_h / 8).max(1)).max(1);
    let min_tile_noise = (tile_area / 50).max(4);
    let mut changed_regions = Vec::new();
    for (ty, row) in tile_hits.iter().enumerate() {
        for (tx, &hits) in row.iter().enumerate() {
            if hits >= min_tile_noise {
                let rx = (tx as u32 * out_w) / 8;
                let ry = (ty as u32 * out_h) / 8;
                let rw = (((tx as u32 + 1) * out_w) / 8).saturating_sub(rx);
                let rh = (((ty as u32 + 1) * out_h) / 8).saturating_sub(ry);
                changed_regions.push(json!({
                    "grid": format!("r{ty}c{tx}"),
                    "x": rx,
                    "y": ry,
                    "width": rw,
                    "height": rh,
                    "changed_pixels": hits,
                }));
            }
        }
    }

    let mismatch_pct = if total_pixels > 0 {
        ((mismatch_pixels as f64 / total_pixels as f64) * 10_000.0).round() / 100.0
    } else {
        0.0
    };
    let dhash_distance = hamming_distance64(base.dhash, curr.dhash);
    let diff_png = encode_rgb_png(prev_w, prev_h, &rgb_diff);

    VisualDiffOutcome {
        width: out_w,
        height: out_h,
        mismatch_pixels,
        total_pixels,
        mismatch_pct,
        dhash_distance,
        changed_regions,
        diff_png,
    }
}

/// Encodes an arbitrary-resolution tightly packed `RGB8` (`width * height * 3`) frame into a valid PNG
/// using multi-block RFC-1950/1951 stored deflate chunks (zero external compression crates).
#[must_use]
pub fn encode_rgb_png(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
    let w = width.max(1) as usize;
    let h = height.max(1) as usize;
    let row_rgb = w * 3;
    let mut raw_scanlines = Vec::with_capacity(h * (1 + row_rgb));
    for y in 0..h {
        raw_scanlines.push(0u8); // Filter 0 (None)
        let src_off = y * row_rgb;
        if src_off + row_rgb <= rgb.len() {
            raw_scanlines.extend_from_slice(&rgb[src_off..src_off + row_rgb]);
        } else {
            raw_scanlines.resize(raw_scanlines.len() + row_rgb, 0);
        }
    }

    let adler = adler32(&raw_scanlines);
    let block_count = raw_scanlines.len().div_ceil(65_535).max(1);
    let mut zlib = Vec::with_capacity(raw_scanlines.len() + 6 + block_count * 5);
    zlib.extend_from_slice(&[0x78, 0x01]);
    if raw_scanlines.is_empty() {
        zlib.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
    } else {
        let chunks: Vec<&[u8]> = raw_scanlines.chunks(65_535).collect();
        for (i, chunk) in chunks.iter().enumerate() {
            let is_last = i + 1 == chunks.len();
            zlib.push(u8::from(is_last));
            let len = chunk.len() as u16;
            let nlen = !len;
            zlib.extend_from_slice(&len.to_le_bytes());
            zlib.extend_from_slice(&nlen.to_le_bytes());
            zlib.extend_from_slice(chunk);
        }
    }
    zlib.extend_from_slice(&adler.to_be_bytes());

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.max(1).to_be_bytes());
    ihdr.extend_from_slice(&height.max(1).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);

    let mut png = Vec::with_capacity(zlib.len() + 64);
    png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    write_chunk(&mut png, b"IHDR", &ihdr);
    write_chunk(&mut png, b"IDAT", &zlib);
    write_chunk(&mut png, b"IEND", &[]);
    png
}

/// Encodes a `RawFrame` (`BGRA`) into a valid `RGB8` PNG.
#[must_use]
pub fn encode_bgra_frame_png(frame: &RawFrame) -> Vec<u8> {
    let px_count = (frame.width as usize) * (frame.height as usize);
    let mut rgb = Vec::with_capacity(px_count * 3);
    for &[b, g, r, _] in frame.bgra.as_chunks::<4>().0.iter().take(px_count) {
        rgb.push(r);
        rgb.push(g);
        rgb.push(b);
    }
    encode_rgb_png(frame.width, frame.height, &rgb)
}

fn sample_bgra(frame: &RawFrame, x: u32, y: u32) -> (u8, u8, u8) {
    if x >= frame.width || y >= frame.height {
        return (0, 0, 0);
    }
    let off = ((y as usize) * (frame.width as usize) + (x as usize)) * 4;
    if off + 2 < frame.bgra.len() {
        (frame.bgra[off], frame.bgra[off + 1], frame.bgra[off + 2])
    } else {
        (0, 0, 0)
    }
}

#[inline]
fn put_px(bgra: &mut [u8], stride: usize, w: i32, h: i32, x: i32, y: i32, rgb: (u8, u8, u8)) {
    if x < 0 || y < 0 || x >= w || y >= h {
        return;
    }
    let off = (y as usize) * stride + (x as usize) * 4;
    if off + 3 < bgra.len() {
        bgra[off] = rgb.2;
        bgra[off + 1] = rgb.1;
        bgra[off + 2] = rgb.0;
        bgra[off + 3] = 0xFF;
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_hline(
    bgra: &mut [u8],
    stride: usize,
    w: i32,
    h: i32,
    x0: i32,
    x1: i32,
    y: i32,
    rgb: (u8, u8, u8),
) {
    for x in x0..=x1 {
        put_px(bgra, stride, w, h, x, y, rgb);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_vline(
    bgra: &mut [u8],
    stride: usize,
    w: i32,
    h: i32,
    x: i32,
    y0: i32,
    y1: i32,
    rgb: (u8, u8, u8),
) {
    for y in y0..=y1 {
        put_px(bgra, stride, w, h, x, y, rgb);
    }
}

#[allow(clippy::too_many_arguments)]
fn fill_rect(
    bgra: &mut [u8],
    stride: usize,
    w: i32,
    h: i32,
    rx: i32,
    ry: i32,
    rw: i32,
    rh: i32,
    rgb: (u8, u8, u8),
) {
    for y in ry..(ry + rh) {
        for x in rx..(rx + rw) {
            put_px(bgra, stride, w, h, x, y, rgb);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_glyph_5x7(
    bgra: &mut [u8],
    stride: usize,
    w: i32,
    h: i32,
    gx: i32,
    gy: i32,
    ch: char,
    rgb: (u8, u8, u8),
) {
    let rows: [u8; 7] = match ch.to_ascii_lowercase() {
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b10000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
        ],
        '6' => [
            0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
        ],
        'e' => [
            0b00000, 0b01110, 0b10001, 0b11111, 0b10000, 0b10001, 0b01110,
        ],
        'v' => [
            0b00000, 0b00000, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        ':' => [
            0b00000, 0b00100, 0b00100, 0b00000, 0b00100, 0b00100, 0b00000,
        ],
        '-' => [
            0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000,
        ],
        _ => [
            0b01110, 0b01010, 0b01110, 0b00000, 0b00000, 0b00000, 0b00000,
        ],
    };
    for (ry, bits) in rows.iter().enumerate() {
        for rx in 0..5 {
            if (bits >> (4 - rx)) & 1 == 1 {
                put_px(bgra, stride, w, h, gx + rx, gy + ry as i32, rgb);
            }
        }
    }
}

fn write_chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    let crc = crc32_ieee(tag, data);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn crc32_ieee(tag: &[u8; 4], data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in tag.iter().chain(data.iter()) {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    const MOD_ADLER: u32 = 65_521;
    let mut s1 = 1u32;
    let mut s2 = 0u32;
    for &b in data {
        s1 = (s1 + u32::from(b)) % MOD_ADLER;
        s2 = (s2 + s1) % MOD_ADLER;
    }
    (s2 << 16) | s1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dhash_annotations_and_visual_diff() {
        let base = synthesize_gear0_frame(320, 200, &[], "https://example.com/a");
        let identical = synthesize_gear0_frame(320, 200, &[], "https://example.com/a");
        assert_eq!(hamming_distance64(base.dhash, identical.dhash), 0);

        let same_diff = compare_frames(&base, &identical);
        assert_eq!(same_diff.mismatch_pixels, 0);
        assert_eq!(same_diff.mismatch_pct, 0.0);
        assert!(same_diff.diff_png.starts_with(b"\x89PNG\r\n\x1a\n"));

        let annotated = synthesize_gear0_frame(
            320,
            200,
            &[AnnotationBox {
                label: "v1:e1".to_string(),
                x: 40,
                y: 60,
                w: 120,
                h: 36,
            }],
            "https://example.com/a",
        );
        let diff = compare_frames(&base, &annotated);
        assert!(diff.mismatch_pixels > 100);
        assert!(diff.mismatch_pct > 0.1);
        assert!(!diff.changed_regions.is_empty());
        assert!(diff.diff_png.starts_with(b"\x89PNG\r\n\x1a\n"));
    }
}
