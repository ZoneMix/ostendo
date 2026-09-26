//! Loads, renders, and caches slide images for the active protocol.
//!
//! Rendered results are cached by everything that changes their pixels, so a
//! frame rebuild never re-decodes or re-encodes an image it already has. Kitty
//! image data is transmitted once per cache entry and freed on eviction.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

use crossterm::style::Color;
use image::RgbaImage;

use crate::image_util::kitty;
use crate::image_util::mermaid::MermaidRenderer;
use crate::image_util::render::{render_slide_image, RenderedImage};
use crate::image_util::GifFrame;
use crate::presentation::{ImagePosition, ImageRenderMode, Slide, SlideImage};
use crate::render::layout::WindowSize;
use crate::render::text::StyledLine;
use crate::terminal::protocols::ImageProtocol;

use super::display::ImageKind;

/// A rendered image: text rows (ASCII) or a protocol image of a known cell size.
#[derive(Clone)]
pub(crate) enum Rendered {
    Lines(Vec<StyledLine>),
    Cells {
        kind: ImageKind,
        cols: usize,
        rows: usize,
    },
}

#[derive(Hash, PartialEq, Eq, Clone)]
struct Key {
    path: PathBuf,
    frame: usize,
    cols: usize,
    rows: usize,
    protocol: ImageProtocol,
    color: String,
    bg: (u8, u8, u8),
}

type GifLoad = JoinHandle<HashMap<PathBuf, Vec<GifFrame>>>;

pub(crate) struct ImageStore {
    protocol: ImageProtocol,
    loaded: HashMap<PathBuf, Arc<RgbaImage>>,
    gifs: HashMap<PathBuf, Arc<Vec<GifFrame>>>,
    gif_loading: Option<GifLoad>,
    gif_frame: usize,
    gif_advanced: Instant,
    cache: HashMap<Key, Rendered>,
    /// Kitty transmissions waiting to be written before the next frame.
    outbox: Vec<u8>,
    kitty_ids: HashSet<u32>,
    mermaid: Option<MermaidRenderer>,
    mermaid_errors: HashMap<u64, String>,
}

impl ImageStore {
    pub fn new(protocol: ImageProtocol, slides: &[Slide]) -> Self {
        let mut store = Self {
            protocol,
            loaded: HashMap::new(),
            gifs: HashMap::new(),
            gif_loading: None,
            gif_frame: 0,
            gif_advanced: Instant::now(),
            cache: HashMap::new(),
            outbox: Vec::new(),
            kitty_ids: HashSet::new(),
            mermaid: None,
            mermaid_errors: HashMap::new(),
        };
        store.preload(slides);
        store
    }

    pub fn protocol(&self) -> ImageProtocol {
        self.protocol
    }

    /// Loads every image the deck references; GIF frames decode in the background.
    pub fn preload(&mut self, slides: &[Slide]) {
        let column_images = slides
            .iter()
            .flat_map(|s| s.columns.iter().flat_map(|c| &c.contents))
            .filter_map(|c| c.image.as_ref().map(|i| PathBuf::from(&i.path)));
        let paths: Vec<PathBuf> = slides
            .iter()
            .filter_map(|s| s.image.as_ref().map(|i| i.path.clone()))
            .chain(column_images)
            .collect();
        let mut gifs = Vec::new();
        for path in paths {
            if self.loaded.contains_key(&path) {
                continue;
            }
            if let Ok(img) = crate::image_util::load_image(&path) {
                self.loaded.insert(path.clone(), Arc::new(img));
                let is_gif = path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("gif"));
                if is_gif {
                    gifs.push(path);
                }
            }
        }
        if !gifs.is_empty() && self.gif_loading.is_none() {
            self.gif_loading = Some(std::thread::spawn(move || {
                gifs.into_iter()
                    .filter_map(|p| Some((p.clone(), crate::image_util::load_gif_frames(&p)?)))
                    .collect()
            }));
        }
        let wants_mermaid = slides.iter().any(|s| !s.mermaid_blocks.is_empty());
        if wants_mermaid && self.mermaid.is_none() && MermaidRenderer::is_available() {
            self.mermaid = MermaidRenderer::new().ok();
        }
    }

    /// Collects finished background GIF decoding; returns true if frames arrived.
    pub fn poll_gif_loading(&mut self) -> bool {
        if !self
            .gif_loading
            .as_ref()
            .is_some_and(JoinHandle::is_finished)
        {
            return false;
        }
        let Some(Ok(loaded)) = self.gif_loading.take().map(JoinHandle::join) else {
            return false;
        };
        self.gifs.extend(
            loaded
                .into_iter()
                .filter(|(_, f)| f.len() > 1)
                .map(|(p, f)| (p, Arc::new(f))),
        );
        true
    }

    pub fn is_animated(&self, path: &Path) -> bool {
        self.gifs.contains_key(path)
    }

    pub fn gif_frame(&self) -> usize {
        self.gif_frame
    }

    pub fn reset_gif(&mut self) {
        self.gif_frame = 0;
        self.gif_advanced = Instant::now();
    }

    /// Advances `path`'s animation when the current frame's delay has passed.
    pub fn advance_gif(&mut self, path: &Path) -> bool {
        let Some(frames) = self.gifs.get(path) else {
            return false;
        };
        let current = frames.get(self.gif_frame % frames.len());
        let delay = current.map_or(100, |f| u128::from(f.delay_ms.max(20)));
        if self.gif_advanced.elapsed().as_millis() < delay {
            return false;
        }
        self.gif_frame = (self.gif_frame + 1) % frames.len();
        self.gif_advanced = Instant::now();
        true
    }

    /// Renders the slide image into at most `cols` × `rows` cells.
    pub fn slide_image(
        &mut self,
        img: &SlideImage,
        cols: usize,
        rows: usize,
        default_protocol: ImageProtocol,
        colors: (Color, Color),
        window: &WindowSize,
    ) -> Option<Rendered> {
        let protocol = match img.render_mode {
            ImageRenderMode::Auto => default_protocol,
            ImageRenderMode::Kitty => ImageProtocol::Kitty,
            ImageRenderMode::Iterm => ImageProtocol::Iterm2,
            ImageRenderMode::Sixel => ImageProtocol::Sixel,
            ImageRenderMode::Ascii => ImageProtocol::Ascii,
        };
        if let Some(frames) = self.gifs.get(&img.path).cloned() {
            let frame = self.gif_frame % frames.len();
            return Some(self.render(
                &frames[frame].image,
                img,
                frame,
                cols,
                rows,
                protocol,
                colors,
                window,
            ));
        }
        let pixels = Arc::clone(self.loaded.get(&img.path)?);
        Some(self.render(&pixels, img, 0, cols, rows, protocol, colors, window))
    }

    /// A column image as text rows (half blocks, or ramp art when forced).
    pub fn column_image(
        &mut self,
        path: &str,
        color: Option<&str>,
        cols: usize,
        colors: (Color, Color),
    ) -> Vec<StyledLine> {
        let img = SlideImage {
            path: PathBuf::from(path),
            alt_text: String::new(),
            position: ImagePosition::Below,
            render_mode: ImageRenderMode::Auto,
            scale: 100,
            color_override: color.unwrap_or_default().to_string(),
        };
        let protocol = if self.protocol == ImageProtocol::Ascii {
            ImageProtocol::Ascii
        } else {
            ImageProtocol::Blocks
        };
        let Some(pixels) = self.loaded.get(&img.path).cloned() else {
            return Vec::new();
        };
        let window = WindowSize {
            columns: 0,
            rows: 0,
            pixel_width: 0,
            pixel_height: 0,
        };
        match self.render(&pixels, &img, 0, cols, cols, protocol, colors, &window) {
            Rendered::Lines(lines) => lines,
            Rendered::Cells { .. } => Vec::new(),
        }
    }

    /// A Mermaid diagram as an image, or the reason it could not be rendered.
    pub fn mermaid(
        &mut self,
        source: &str,
        cols: usize,
        rows: usize,
        colors: (Color, Color),
        window: &WindowSize,
    ) -> Result<Rendered, String> {
        let hash = {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            source.hash(&mut h);
            h.finish()
        };
        if let Some(err) = self.mermaid_errors.get(&hash) {
            return Err(err.clone());
        }
        let renderer = self
            .mermaid
            .as_mut()
            .ok_or_else(|| "install mermaid-cli (mmdc) to render".to_string())?;
        let pixel_width = (window.pixels_per_column() * cols as f64) as usize;
        let path = match renderer.render(source, pixel_width.max(400)) {
            Ok(path) => path,
            Err(e) => {
                let msg = format!("render failed: {e}");
                self.mermaid_errors.insert(hash, msg.clone());
                return Err(msg);
            }
        };
        if !self.loaded.contains_key(&path) {
            let img = crate::image_util::load_image(&path).map_err(|e| e.to_string())?;
            self.loaded.insert(path.clone(), Arc::new(img));
        }
        let img = SlideImage {
            path: path.clone(),
            alt_text: String::new(),
            position: ImagePosition::Below,
            render_mode: ImageRenderMode::Auto,
            scale: 100,
            color_override: String::new(),
        };
        let pixels = Arc::clone(&self.loaded[&path]);
        Ok(self.render(&pixels, &img, 0, cols, rows, self.protocol, colors, window))
    }

    #[allow(clippy::too_many_arguments)]
    fn render(
        &mut self,
        pixels: &RgbaImage,
        img: &SlideImage,
        frame: usize,
        cols: usize,
        rows: usize,
        protocol: ImageProtocol,
        (text, bg): (Color, Color),
        window: &WindowSize,
    ) -> Rendered {
        let key = Key {
            path: img.path.clone(),
            frame,
            cols,
            rows,
            protocol,
            color: img.color_override.clone(),
            bg: crate::theme::colors::color_to_rgb(bg).unwrap_or_default(),
        };
        if let Some(hit) = self.cache.get(&key) {
            return hit.clone();
        }
        let rendered = match render_slide_image(pixels, img, cols, rows, protocol, text, bg, window)
        {
            RenderedImage::Lines(lines) => Rendered::Lines(lines),
            RenderedImage::Protocol {
                escape_data,
                cols,
                rows,
            } => Rendered::Cells {
                kind: ImageKind::Inline(Rc::from(escape_data)),
                cols,
                rows,
            },
            RenderedImage::KittyPlacement {
                image_id,
                cols,
                rows,
                transmit_escape,
            } => {
                self.outbox.extend_from_slice(transmit_escape.as_bytes());
                self.kitty_ids.insert(image_id);
                Rendered::Cells {
                    kind: ImageKind::Kitty {
                        id: image_id,
                        cols: cols as u16,
                    },
                    cols,
                    rows,
                }
            }
        };
        // GIF frames on Kitty would otherwise pile up in terminal memory: keep
        // only the current frame of each animation.
        if frame > 0 || self.is_animated(&img.path) {
            let stale: Vec<Key> = self
                .cache
                .keys()
                .filter(|k| k.path == key.path && k.frame != frame)
                .cloned()
                .collect();
            for k in stale {
                self.evict(&k);
            }
        }
        self.cache.insert(key, rendered.clone());
        rendered
    }

    fn evict(&mut self, key: &Key) {
        if let Some(Rendered::Cells {
            kind: ImageKind::Kitty { id, .. },
            ..
        }) = self.cache.remove(key)
        {
            if self.kitty_ids.remove(&id) {
                self.outbox
                    .extend_from_slice(kitty::delete_image(id).as_bytes());
            }
        }
    }

    /// Drops every rendered image (colors or cell size changed).
    pub fn clear(&mut self) {
        for id in self.kitty_ids.drain() {
            self.outbox
                .extend_from_slice(kitty::delete_image(id).as_bytes());
        }
        self.cache.clear();
    }

    /// Escape data that must reach the terminal before the next frame.
    pub fn take_outbox(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.outbox)
    }
}
