//! Kitty graphics protocol escapes.
//!
//! Images are transmitted once under a numeric ID (`a=t`) and then shown with
//! small placement commands (`a=p`), so redrawing a slide never re-sends pixel
//! data. Every command carries `q=2` so the terminal sends no replies that
//! would pollute the input stream.

use base64::Engine;
use std::io::Cursor;
use std::sync::atomic::{AtomicU32, Ordering};

/// IDs are never reused within a session so a stale placement can never show
/// a different image.
static NEXT_IMAGE_ID: AtomicU32 = AtomicU32::new(1);

/// The protocol caps each escape's base64 payload at 4096 bytes.
const CHUNK_SIZE: usize = 4096;

/// Remove every placement and free all transmitted image data.
pub const DELETE_ALL_IMAGES: &str = "\x1b_Ga=d,d=A,q=2;AAAA\x1b\\";

pub fn next_image_id() -> u32 {
    NEXT_IMAGE_ID.fetch_add(1, Ordering::Relaxed)
}

/// PNG-encode `img` and transmit it under `id` without displaying it.
/// Returns `None` for an empty image or if encoding fails.
pub fn transmit_escape(id: u32, img: &image::RgbaImage) -> Option<String> {
    let (sw, sh) = img.dimensions();
    if sw == 0 || sh == 0 {
        return None;
    }

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
    let chunks: Vec<&str> = encoded
        .as_bytes()
        .chunks(CHUNK_SIZE)
        .map(|chunk| std::str::from_utf8(chunk).unwrap_or(""))
        .collect();
    let mut escape = String::with_capacity(encoded.len() + chunks.len() * 40);
    for (i, chunk) in chunks.iter().enumerate() {
        // m=1 announces that more chunks follow; only the first carries keys.
        let more = u8::from(i + 1 < chunks.len());
        if i == 0 {
            escape.push_str(&format!(
                "\x1b_Ga=t,i={id},f=100,t=d,q=2,m={more};{chunk}\x1b\\"
            ));
        } else {
            escape.push_str(&format!("\x1b_Gm={more};{chunk}\x1b\\"));
        }
    }
    Some(escape)
}

/// Show image `id` at the cursor, scaled to `cols` x `rows` cells. Reusing
/// `placement_id` moves the existing placement instead of stacking another;
/// `C=1` keeps the cursor where it was.
pub fn placement_escape(id: u32, placement_id: u32, cols: usize, rows: usize) -> String {
    format!("\x1b_Ga=p,i={id},p={placement_id},c={cols},r={rows},C=1,q=2;AAAA\x1b\\")
}

/// Remove the placements of image `id` but keep its data for re-placement.
pub fn delete_placements(id: u32) -> String {
    format!("\x1b_Ga=d,d=i,i={id},q=2;AAAA\x1b\\")
}

/// Remove the placements of image `id` and free its data.
pub fn delete_image(id: u32) -> String {
    format!("\x1b_Ga=d,d=I,i={id},q=2;AAAA\x1b\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_placement_and_delete_escapes() {
        let cases = [
            (
                placement_escape(42, 7, 80, 20),
                "\x1b_Ga=p,i=42,p=7,c=80,r=20,C=1,q=2;AAAA\x1b\\",
            ),
            (delete_placements(42), "\x1b_Ga=d,d=i,i=42,q=2;AAAA\x1b\\"),
            (delete_image(42), "\x1b_Ga=d,d=I,i=42,q=2;AAAA\x1b\\"),
        ];
        for (actual, expected) in cases {
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn test_transmit_escape_chunks_payload() {
        // Pseudo-random pixels so the PNG cannot compress below one chunk.
        let mut seed = 1u32;
        let noise = image::RgbaImage::from_fn(64, 64, |_, _| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            image::Rgba(seed.to_le_bytes())
        });
        let pixel = image::RgbaImage::new(1, 1);

        for (img, expect_multi) in [(pixel, false), (noise, true)] {
            let esc = transmit_escape(99, &img).unwrap();
            let chunks: Vec<&str> = esc
                .strip_suffix("\x1b\\")
                .unwrap()
                .split("\x1b\\")
                .map(|c| c.strip_prefix("\x1b_G").unwrap())
                .collect();
            assert_eq!(chunks.len() > 1, expect_multi);

            let mut payload = String::new();
            for (i, chunk) in chunks.iter().enumerate() {
                let (keys, data) = chunk.split_once(';').unwrap();
                let more = if i + 1 < chunks.len() { "m=1" } else { "m=0" };
                let expected_keys = if i == 0 {
                    format!("a=t,i=99,f=100,t=d,q=2,{more}")
                } else {
                    more.to_string()
                };
                assert_eq!(keys, expected_keys);
                assert!(data.len() <= CHUNK_SIZE);
                payload.push_str(data);
            }
            let png = base64::engine::general_purpose::STANDARD
                .decode(payload)
                .unwrap();
            let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
            assert_eq!(decoded, img);
        }
    }

    #[test]
    fn test_transmit_escape_zero_size_returns_none() {
        let img = image::RgbaImage::new(0, 0);
        assert!(transmit_escape(1, &img).is_none());
    }
}
