//! Loads, renders, and caches slide images for the active protocol.
//!
//! Rendered results are cached by everything that changes their pixels, so a
//! frame rebuild never re-decodes or re-encodes an image it already has. Kitty
//! image data is transmitted once per cache entry and freed on eviction.
//!
//! Decoding, protocol encoding (a Sixel image takes over a second) and Mermaid
//! run off the UI thread. Until a result arrives the image keeps its cells
//! blank, so the slide does not move when it lands; [`ImageStore::poll`]
//! reports arrivals so the frame is rebuilt.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
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
    /// Theme colors: ASCII art and captions are drawn in them.
    text: (u8, u8, u8),
    bg: (u8, u8, u8),
}

/// Background work coming back to the UI thread.
enum Done {
    Decoded {
        path: PathBuf,
        still: Option<RgbaImage>,
        frames: Option<Vec<GifFrame>>,
    },
    /// `None` when the encoder skipped a job queued before [`ImageStore::clear`].
    Encoded {
        key: Key,
        epoch: u64,
        image: Option<RenderedImage>,
    },
    Mermaid {
        hash: u64,
        result: Result<(PathBuf, RgbaImage), String>,
    },
}

struct EncodeJob {
    key: Key,
    epoch: u64,
    pixels: Arc<RgbaImage>,
    img: SlideImage,
    colors: (Color, Color),
    window: WindowSize,
}

pub(crate) struct ImageStore {
    protocol: ImageProtocol,
    loaded: HashMap<PathBuf, Arc<RgbaImage>>,
    /// Sizes read from file headers, so an image still decoding keeps its cells.
    dims: HashMap<PathBuf, (u32, u32)>,
    decoding: HashSet<PathBuf>,
    gifs: HashMap<PathBuf, Arc<Vec<GifFrame>>>,
    gif_frame: usize,
    gif_advanced: Instant,
    cache: HashMap<Key, Rendered>,
    encoding: HashSet<Key>,
    /// Bumped by `clear`, so encodes queued for the old size are skipped.
    epoch: Arc<AtomicU64>,
    encoder: mpsc::Sender<EncodeJob>,
    done_tx: mpsc::Sender<Done>,
    done: mpsc::Receiver<Done>,
    /// Kitty transmissions waiting to be written before the next frame.
    outbox: Vec<u8>,
    kitty_ids: HashSet<u32>,
    mermaid: Option<MermaidRenderer>,
    /// Rendered diagram (PNG path) or the reason there is none, by source hash.
    diagrams: HashMap<u64, Result<PathBuf, String>>,
    diagrams_rendering: HashSet<u64>,
}

impl ImageStore {
    pub fn new(protocol: ImageProtocol, slides: &[Slide]) -> Self {
        let (done_tx, done) = mpsc::channel();
        let (encoder, jobs) = mpsc::channel::<EncodeJob>();
        let epoch = Arc::new(AtomicU64::new(0));
        {
            let (done_tx, epoch) = (done_tx.clone(), Arc::clone(&epoch));
            // One encoder, so a resize cannot start a pile of parallel encodes.
            std::thread::spawn(move || {
                for job in jobs {
                    let current = job.epoch == epoch.load(Ordering::Relaxed);
                    let image = current.then(|| {
                        let (text, bg) = job.colors;
                        let (k, w) = (&job.key, &job.window);
                        render_slide_image(
                            &job.pixels,
                            &job.img,
                            k.cols,
                            k.rows,
                            k.protocol,
                            text,
                            bg,
                            w,
                        )
                    });
                    let done = Done::Encoded {
                        key: job.key,
                        epoch: job.epoch,
                        image,
                    };
                    if done_tx.send(done).is_err() {
                        break;
                    }
                }
            });
        }
        let mut store = Self {
            protocol,
            loaded: HashMap::new(),
            dims: HashMap::new(),
            decoding: HashSet::new(),
            gifs: HashMap::new(),
            gif_frame: 0,
            gif_advanced: Instant::now(),
            cache: HashMap::new(),
            encoding: HashSet::new(),
            epoch,
            encoder,
            done_tx,
            done,
            outbox: Vec::new(),
            kitty_ids: HashSet::new(),
            mermaid: None,
            diagrams: HashMap::new(),
            diagrams_rendering: HashSet::new(),
        };
        store.preload(slides);
        store
    }

    pub fn protocol(&self) -> ImageProtocol {
        self.protocol
    }

    /// Decodes every image the deck references, in the background.
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
        let mut todo = Vec::new();
        for path in paths {
            if self.loaded.contains_key(&path) || !self.decoding.insert(path.clone()) {
                continue;
            }
            if let Ok(dims) = image::image_dimensions(&path) {
                self.dims.insert(path.clone(), dims);
            }
            todo.push(path);
        }
        if todo.is_empty() {
            return;
        }
        let done = self.done_tx.clone();
        std::thread::spawn(move || {
            for path in todo {
                let still = crate::image_util::load_image(&path).ok();
                let frames = still
                    .as_ref()
                    .and_then(|_| crate::image_util::load_gif_frames(&path));
                if done
                    .send(Done::Decoded {
                        path,
                        still,
                        frames,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
    }

    /// Takes in finished background work; returns true if anything arrived,
    /// so the slide must be laid out again.
    pub fn poll(&mut self) -> bool {
        let mut arrived = false;
        while let Ok(done) = self.done.try_recv() {
            arrived |= self.accept(done);
        }
        arrived
    }

    /// Whether background work is still due, so the event loop should check
    /// back soon rather than idle.
    pub fn busy(&self) -> bool {
        !(self.decoding.is_empty()
            && self.encoding.is_empty()
            && self.diagrams_rendering.is_empty())
    }

    /// Blocks until all background work has arrived; for headless layout,
    /// which has no next frame to show it in. Returns true if anything did.
    pub fn wait(&mut self) -> bool {
        let mut arrived = false;
        while self.busy() {
            let Ok(done) = self.done.recv() else { break };
            arrived |= self.accept(done);
        }
        arrived
    }

    fn accept(&mut self, done: Done) -> bool {
        match done {
            Done::Decoded {
                path,
                still,
                frames,
            } => {
                self.decoding.remove(&path);
                if let Some(frames) = frames.filter(|f| f.len() > 1) {
                    self.gifs.insert(path.clone(), Arc::new(frames));
                }
                if let Some(still) = still {
                    self.loaded.insert(path, Arc::new(still));
                }
            }
            Done::Encoded { key, epoch, image } => {
                if epoch != self.epoch.load(Ordering::Relaxed) {
                    return false;
                }
                self.encoding.remove(&key);
                let Some(image) = image else { return false };
                self.store(key, image);
            }
            Done::Mermaid { hash, result } => {
                self.diagrams_rendering.remove(&hash);
                let result = result.map(|(path, img)| {
                    self.loaded.insert(path.clone(), Arc::new(img));
                    path
                });
                self.diagrams.insert(hash, result);
            }
        }
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
                &Arc::clone(&frames[frame].image),
                img,
                frame,
                cols,
                rows,
                protocol,
                colors,
                window,
            ));
        }
        let Some(pixels) = self.loaded.get(&img.path).cloned() else {
            // Still decoding: hold the cells it will take; otherwise missing.
            self.decoding.contains(&img.path).then_some(())?;
            return Some(match self.dims.get(&img.path) {
                Some(&dims)
                    if !matches!(protocol, ImageProtocol::Ascii | ImageProtocol::Blocks) =>
                {
                    blank(dims, cols, rows, window)
                }
                _ => Rendered::Lines(Vec::new()),
            });
        };
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

    /// A Mermaid diagram as an image, or the text to show instead: why it
    /// could not be rendered, or that it is still rendering.
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
        let path = match self.diagrams.get(&hash) {
            Some(Ok(path)) => path.clone(),
            Some(Err(reason)) => return Err(reason.clone()),
            None => {
                if self.diagrams_rendering.insert(hash) {
                    self.start_mermaid(source, hash);
                }
                return Err("rendering…".to_string());
            }
        };
        let Some(pixels) = self.loaded.get(&path).cloned() else {
            return Err("rendering…".to_string());
        };
        let img = SlideImage {
            path,
            alt_text: String::new(),
            position: ImagePosition::Below,
            render_mode: ImageRenderMode::Auto,
            scale: 100,
            color_override: String::new(),
        };
        Ok(self.render(&pixels, &img, 0, cols, rows, self.protocol, colors, window))
    }

    fn start_mermaid(&mut self, source: &str, hash: u64) {
        let renderer = match &self.mermaid {
            Some(renderer) => renderer,
            None => match MermaidRenderer::new() {
                Ok(renderer) => self.mermaid.insert(renderer),
                Err(e) => {
                    self.diagrams_rendering.remove(&hash);
                    self.diagrams.insert(hash, Err(format!("{e:#}")));
                    return;
                }
            },
        };
        let job = renderer.job(source, hash);
        let done = self.done_tx.clone();
        std::thread::spawn(move || {
            let result = job()
                .and_then(|path| Ok((path.clone(), crate::image_util::load_image(&path)?)))
                .map_err(|e| format!("{e:#}"));
            let _ = done.send(Done::Mermaid { hash, result });
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn render(
        &mut self,
        pixels: &Arc<RgbaImage>,
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
            text: crate::theme::colors::color_to_rgb(text).unwrap_or_default(),
            bg: crate::theme::colors::color_to_rgb(bg).unwrap_or_default(),
        };
        if let Some(hit) = self.cache.get(&key) {
            return hit.clone();
        }
        if matches!(protocol, ImageProtocol::Ascii | ImageProtocol::Blocks) {
            let image = render_slide_image(pixels, img, cols, rows, protocol, text, bg, window);
            return self.store(key, image);
        }
        let animated = self.is_animated(&img.path);
        // An animation encodes one frame at a time and shows the newest it has.
        let busy = animated && self.encoding.iter().any(|k| k.path == key.path);
        if !busy && self.encoding.insert(key.clone()) {
            let job = EncodeJob {
                key: key.clone(),
                epoch: self.epoch.load(Ordering::Relaxed),
                pixels: Arc::clone(pixels),
                img: img.clone(),
                colors: (text, bg),
                window: *window,
            };
            let _ = self.encoder.send(job);
        }
        let shown =
            |k: &Key| k.path == key.path && (k.cols, k.rows, k.protocol) == (cols, rows, protocol);
        if let Some((_, frame)) = self.cache.iter().find(|(k, _)| animated && shown(k)) {
            return frame.clone();
        }
        blank(pixels.dimensions(), cols, rows, window)
    }

    /// Caches a finished image, sending Kitty its data, and returns it.
    fn store(&mut self, key: Key, image: RenderedImage) -> Rendered {
        let rendered = match image {
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
        if key.protocol == ImageProtocol::Kitty && (key.frame > 0 || self.is_animated(&key.path)) {
            let stale: Vec<Key> = self
                .cache
                .keys()
                .filter(|k| k.path == key.path && k.frame != key.frame)
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
        self.encoding.clear();
        self.epoch.fetch_add(1, Ordering::Relaxed);
    }

    /// Escape data that must reach the terminal before the next frame.
    pub fn take_outbox(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.outbox)
    }
}

/// Blank cells the size an image will be once encoded.
fn blank(dims: (u32, u32), cols: usize, rows: usize, window: &WindowSize) -> Rendered {
    let (_, _, cols, rows) = crate::image_util::fit_cells(dims, window, cols, rows);
    Rendered::Cells {
        kind: ImageKind::Inline(Rc::from("")),
        cols,
        rows,
    }
}
