//! Kitty graphics protocol: transmit/display split architecture.
//!
//! Instead of `a=T` (transmit+display combined), this module implements:
//! - `a=t` (transmit once, assign stable image ID)
//! - `a=p` (display by ID at cursor position — ~50 bytes)
//! - `a=f` (add animation frames to an image ID)
//! - `a=a` (start/stop animation playback)
//! - `a=d` (delete images by ID or all)
//!
//! This architecture fixes two critical issues:
//! 1. **Image centering**: cursor position controls placement, not embedded padding
//! 2. **Timer jitter during GIF**: placement commands are instant (~50 bytes),
//!    while transmissions happen once during prerender
//!
//! # Protocol Reference
//!
//! All commands use APC format: `\x1b_G<keys>;<payload>\x1b\\`
//! - `q=2` suppresses error responses (prevents input stream pollution)
//! - `C=1` prevents cursor movement after placement
//! - `m=0/1` controls chunked transmission (4096-byte chunks)

use base64::Engine;
use std::io::{Cursor, Write};
use std::sync::atomic::{AtomicU32, Ordering};

/// Global image ID counter. Each transmitted image gets a unique ID.
/// IDs are never reused within a session to avoid stale placement references.
static NEXT_IMAGE_ID: AtomicU32 = AtomicU32::new(1);

/// Allocate a new unique Kitty image ID.
pub fn next_image_id() -> u32 {
    NEXT_IMAGE_ID.fetch_add(1, Ordering::Relaxed)
}

/// Reset the ID counter (call on session restart or alternate screen switch).
pub fn reset_image_ids() {
    NEXT_IMAGE_ID.store(1, Ordering::Relaxed);
}

/// Transmit image data to Kitty without displaying it.
///
/// Returns the escape sequence string that, when written to stdout,
/// sends the image data with `a=t` (transmit only, no display).
/// The image is assigned the given `id` and can later be placed with
/// [`placement_escape`].
///
/// The image is PNG-encoded before base64 encoding and chunked at 4096 bytes.
pub fn transmit_escape(id: u32, img: &image::RgbaImage) -> Option<String> {
    let (sw, sh) = img.dimensions();
    if sw == 0 || sh == 0 {
        return None;
    }

    // PNG encode
    let mut png_bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(Cursor::new(&mut png_bytes));
    image::ImageEncoder::write_image(
        encoder,
        img.as_raw(),
        sw,
        sh,
        image::ExtendedColorType::Rgba8,
    )
    .ok()?;

    let encoded = base64::engine::general_purpose::STANDARD.encode(&png_bytes);

    // Build chunked escape sequence
    let chunk_size = 4096;
    let chunks: Vec<&[u8]> = encoded.as_bytes().chunks(chunk_size).collect();
    let mut escape = String::with_capacity(encoded.len() + chunks.len() * 40);

    for (i, chunk) in chunks.iter().enumerate() {
        let m = if i == chunks.len() - 1 { 0 } else { 1 };
        let chunk_str = std::str::from_utf8(chunk).unwrap_or("");
        if i == 0 {
            // First chunk: full metadata
            // a=t: transmit only (no display)
            // i=<id>: image ID for later reference
            // f=100: PNG format
            // t=d: direct (inline) data
            // q=2: suppress error responses
            escape.push_str(&format!(
                "\x1b_Ga=t,i={},f=100,t=d,q=2,m={};{}\x1b\\",
                id, m, chunk_str
            ));
        } else {
            escape.push_str(&format!("\x1b_Gm={};{}\x1b\\", m, chunk_str));
        }
    }

    Some(escape)
}

/// Generate a placement escape to display a previously transmitted image.
///
/// This is the fast path (~50 bytes) used on every render_frame() instead
/// of re-transmitting the full image data.
///
/// - `id`: the image ID from [`transmit_escape`]
/// - `cols`: number of terminal columns to span
/// - `rows`: number of terminal rows to span
///
/// The image is placed at the current cursor position. Use
/// `cursor::MoveTo(col, row)` before writing this escape.
/// `C=1` prevents cursor movement after placement.
pub fn placement_escape(id: u32, cols: usize, rows: usize) -> String {
    format!(
        "\x1b_Ga=p,i={},c={},r={},C=1,q=2;AAAA\x1b\\",
        id, cols, rows
    )
}

/// Generate an escape to add an animation frame to an existing image.
///
/// - `id`: the base image ID
/// - `frame_img`: the frame's RGBA pixel data
/// - `gap_ms`: delay in milliseconds before the next frame displays
///
/// Returns the chunked escape sequence for the frame data.
pub fn animation_frame_escape(
    id: u32,
    frame_img: &image::RgbaImage,
    gap_ms: u32,
) -> Option<String> {
    let (sw, sh) = frame_img.dimensions();
    if sw == 0 || sh == 0 {
        return None;
    }

    // PNG encode
    let mut png_bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(Cursor::new(&mut png_bytes));
    image::ImageEncoder::write_image(
        encoder,
        frame_img.as_raw(),
        sw,
        sh,
        image::ExtendedColorType::Rgba8,
    )
    .ok()?;

    let encoded = base64::engine::general_purpose::STANDARD.encode(&png_bytes);

    let chunk_size = 4096;
    let chunks: Vec<&[u8]> = encoded.as_bytes().chunks(chunk_size).collect();
    let mut escape = String::with_capacity(encoded.len() + chunks.len() * 40);

    for (i, chunk) in chunks.iter().enumerate() {
        let m = if i == chunks.len() - 1 { 0 } else { 1 };
        let chunk_str = std::str::from_utf8(chunk).unwrap_or("");
        if i == 0 {
            escape.push_str(&format!(
                "\x1b_Ga=f,i={},z={},f=100,t=d,q=2,m={};{}\x1b\\",
                id, gap_ms, m, chunk_str
            ));
        } else {
            // Animation frame chunks must include a=f (unlike regular image chunks)
            escape.push_str(&format!("\x1b_Ga=f,m={};{}\x1b\\", m, chunk_str));
        }
    }

    Some(escape)
}

/// Start animation loop playback for a transmitted image with frames.
pub fn animation_start_escape(id: u32) -> String {
    // s=3: loop animation, v=1: loop infinitely
    format!("\x1b_Ga=a,i={},s=3,v=1,q=2;AAAA\x1b\\", id)
}

/// Stop animation playback.
pub fn animation_stop_escape(id: u32) -> String {
    // s=1: stop animation
    format!("\x1b_Ga=a,i={},s=1,q=2;AAAA\x1b\\", id)
}

/// Delete all placements and free data for a specific image ID.
pub fn delete_image_escape(id: u32) -> String {
    // d=I: delete placements AND free image data
    format!("\x1b_Ga=d,d=I,i={},q=2;AAAA\x1b\\", id)
}

/// Delete ALL images (placements + data). Use on slide change or exit.
pub fn delete_all_escape() -> String {
    format!("\x1b_Ga=d,d=A,q=2;AAAA\x1b\\")
}

/// Wrap a Kitty escape sequence for tmux passthrough if running inside tmux.
pub fn tmux_wrap(escape: &str) -> String {
    if std::env::var("TMUX").is_ok() {
        // DCS passthrough for tmux
        format!("\x1bPtmux;{}\x1b\\", escape.replace('\x1b', "\x1b\x1b"))
    } else {
        escape.to_string()
    }
}

/// A transmitted image handle. Tracks the Kitty image ID, dimensions in
/// terminal cells, and whether animation frames have been uploaded.
#[derive(Debug, Clone)]
pub struct TransmittedImage {
    /// Unique Kitty image ID
    pub id: u32,
    /// Display width in terminal columns
    pub cols: usize,
    /// Display height in terminal rows
    pub rows: usize,
    /// Number of animation frames (0 = static image)
    pub frame_count: usize,
    /// Whether animation is currently playing
    pub animating: bool,
}

impl TransmittedImage {
    /// Generate the placement escape for this image.
    pub fn place(&self) -> String {
        placement_escape(self.id, self.cols, self.rows)
    }

    /// Generate the delete escape for this image.
    pub fn delete(&self) -> String {
        delete_image_escape(self.id)
    }

    /// Generate animation start escape.
    pub fn start_animation(&mut self) -> String {
        self.animating = true;
        animation_start_escape(self.id)
    }

    /// Generate animation stop escape.
    pub fn stop_animation(&mut self) -> String {
        self.animating = false;
        animation_stop_escape(self.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_image_id_increments() {
        let a = next_image_id();
        let b = next_image_id();
        assert_eq!(b, a + 1);
    }

    #[test]
    fn test_placement_escape_format() {
        let esc = placement_escape(42, 80, 20);
        assert!(esc.contains("a=p"));
        assert!(esc.contains("i=42"));
        assert!(esc.contains("c=80"));
        assert!(esc.contains("r=20"));
        assert!(esc.contains("C=1"));
        assert!(esc.contains("q=2"));
    }

    #[test]
    fn test_delete_image_escape() {
        let esc = delete_image_escape(42);
        assert!(esc.contains("a=d"));
        assert!(esc.contains("d=I"));
        assert!(esc.contains("i=42"));
    }

    #[test]
    fn test_delete_all_escape() {
        let esc = delete_all_escape();
        assert!(esc.contains("a=d"));
        assert!(esc.contains("d=A"));
    }

    #[test]
    fn test_animation_start_escape() {
        let esc = animation_start_escape(42);
        assert!(esc.contains("a=a"));
        assert!(esc.contains("i=42"));
        assert!(esc.contains("s=3"));
        assert!(esc.contains("v=1"));
    }

    #[test]
    fn test_animation_stop_escape() {
        let esc = animation_stop_escape(42);
        assert!(esc.contains("a=a"));
        assert!(esc.contains("s=1"));
    }

    #[test]
    fn test_transmit_escape_small_image() {
        // Create a tiny 2x2 RGBA image
        let img = image::RgbaImage::from_raw(2, 2, vec![
            255, 0, 0, 255,  0, 255, 0, 255,
            0, 0, 255, 255,  255, 255, 255, 255,
        ]).unwrap();

        let esc = transmit_escape(99, &img).unwrap();
        assert!(esc.contains("a=t"));
        assert!(esc.contains("i=99"));
        assert!(esc.contains("f=100"));
        assert!(esc.contains("t=d"));
        assert!(esc.contains("m=0")); // Small image = single chunk
    }

    #[test]
    fn test_transmit_escape_zero_size_returns_none() {
        let img = image::RgbaImage::new(0, 0);
        assert!(transmit_escape(1, &img).is_none());
    }

    #[test]
    fn test_animation_frame_escape_format() {
        let img = image::RgbaImage::from_raw(2, 2, vec![
            255, 0, 0, 255,  0, 255, 0, 255,
            0, 0, 255, 255,  255, 255, 255, 255,
        ]).unwrap();

        let esc = animation_frame_escape(42, &img, 100).unwrap();
        assert!(esc.contains("a=f"));
        assert!(esc.contains("i=42"));
        assert!(esc.contains("z=100"));
    }

    #[test]
    fn test_transmitted_image_place() {
        let ti = TransmittedImage {
            id: 7,
            cols: 40,
            rows: 10,
            frame_count: 0,
            animating: false,
        };
        let esc = ti.place();
        assert!(esc.contains("i=7"));
        assert!(esc.contains("c=40"));
        assert!(esc.contains("r=10"));
    }

    #[test]
    fn test_tmux_wrap_no_tmux() {
        // When TMUX is not set, passthrough
        std::env::remove_var("TMUX");
        let input = "\x1b_Ga=p,i=1;AAAA\x1b\\";
        assert_eq!(tmux_wrap(input), input);
    }
}
