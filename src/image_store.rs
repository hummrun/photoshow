//! Image runtime: bounded decode workers, display cache and egui texture adapter.
//!
//! Full-resolution pixels are never retained in the viewer cache. The cache keeps
//! only display-sized CPU images; RGBA upload buffers are transient and dropped
//! immediately after the texture is created.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use crate::editor::EditorState;
use crate::media::decoder::{DecodedPhoto, LoadError, decode_photo};
use crate::media::pixels::RgbaFrame;

const PREFETCH_RADIUS: isize = 2;
const PREFETCH_CAP: usize = 4;
const PREFETCH_MAX_INFLIGHT: usize = 2;
const LOAD_POLL_INTERVAL: Duration = Duration::from_millis(16);

fn decoded_bytes(decoded: &DecodedPhoto) -> u64 {
    decoded.display.as_bytes().len() as u64
}

struct LoadRequest {
    id: u64,
    path: PathBuf,
}

struct LoadMsg {
    id: u64,
    path: PathBuf,
    result: Result<DecodedPhoto, LoadError>,
}

struct PrefetchMsg {
    generation: u64,
    path: PathBuf,
    result: Option<DecodedPhoto>,
}

pub enum LoadState {
    Empty,
    Loading,
    Loaded {
        texture: egui::TextureHandle,
        display_px: egui::Vec2,
        full_px: (u32, u32),
    },
    Failed(String),
}

pub struct ImageStore {
    load_tx: Sender<LoadRequest>,
    rx: Receiver<LoadMsg>,
    next_id: u64,
    tex_seq: u64,
    current_id: u64,
    texture: Option<egui::TextureHandle>,
    base_texture: Option<egui::TextureHandle>,
    display_px: egui::Vec2,
    base_display_px: egui::Vec2,
    display_img: Option<image::DynamicImage>,
    full_px: (u32, u32),
    error: Option<String>,
    has_selection: bool,
    pre_tx: Sender<PrefetchMsg>,
    pre_rx: Receiver<PrefetchMsg>,
    prefetch: HashMap<PathBuf, DecodedPhoto>,
    prefetch_order: VecDeque<PathBuf>,
    prefetch_bytes: u64,
    prefetch_budget_bytes: u64,
    inflight: HashSet<PathBuf>,
    prefetch_generation: u64,
}

impl ImageStore {
    #[must_use]
    pub fn new() -> Self {
        let (load_tx, load_rx) = mpsc::channel::<LoadRequest>();
        let (result_tx, result_rx) = mpsc::channel::<LoadMsg>();
        std::thread::spawn(move || {
            while let Ok(mut request) = load_rx.recv() {
                // A decode that already started cannot be cheaply cancelled, but
                // queued selections can. Keep only the newest pending request.
                while let Ok(newer) = load_rx.try_recv() {
                    request = newer;
                }
                let result = decode_photo(&request.path);
                if result_tx
                    .send(LoadMsg {
                        id: request.id,
                        path: request.path,
                        result,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        let (pre_tx, pre_rx) = mpsc::channel();
        Self {
            load_tx,
            rx: result_rx,
            next_id: 0,
            tex_seq: 0,
            current_id: 0,
            texture: None,
            base_texture: None,
            display_px: egui::Vec2::ZERO,
            base_display_px: egui::Vec2::ZERO,
            display_img: None,
            full_px: (0, 0),
            error: None,
            has_selection: false,
            pre_tx,
            pre_rx,
            prefetch: HashMap::new(),
            prefetch_order: VecDeque::new(),
            prefetch_bytes: 0,
            prefetch_budget_bytes: 0,
            inflight: HashSet::new(),
            prefetch_generation: 0,
        }
    }

    fn texture_from_image(
        &mut self,
        ctx: &egui::Context,
        image: &image::DynamicImage,
        tag: &str,
    ) -> (egui::TextureHandle, egui::Vec2) {
        // RgbaFrame is intentionally transient: after upload we retain the
        // original display-sized DynamicImage and the GPU texture, not a second
        // RGBA CPU copy.
        let frame = RgbaFrame::from_dynamic(image);
        self.tex_seq += 1;
        let size = egui::Vec2::new(frame.width() as f32, frame.height() as f32);
        let color = egui::ColorImage::from_rgba_unmultiplied(
            [frame.width() as usize, frame.height() as usize],
            frame.bytes(),
        );
        let texture = ctx.load_texture(
            format!("photo-{tag}-{}", self.tex_seq),
            color,
            egui::TextureOptions::LINEAR,
        );
        (texture, size)
    }

    fn install_base(&mut self, ctx: &egui::Context, decoded: DecodedPhoto, tag: &str) {
        self.full_px = decoded.full_size;
        let (texture, size) = self.texture_from_image(ctx, &decoded.display, tag);
        self.display_px = size;
        self.base_display_px = size;
        self.base_texture = Some(texture.clone());
        self.texture = Some(texture);
        self.display_img = Some(decoded.display);
    }

    pub fn select(&mut self, ctx: &egui::Context, photo: &crate::fs_browser::PhotoPath) {
        self.next_id += 1;
        let id = self.next_id;
        self.current_id = id;
        self.has_selection = true;
        self.texture = None;
        self.base_texture = None;
        self.display_img = None;
        self.error = None;
        let path = photo.path().to_path_buf();

        if let Some(decoded) = self.prefetch.remove(&path) {
            self.prefetch_bytes = self.prefetch_bytes.saturating_sub(decoded_bytes(&decoded));
            self.install_base(ctx, decoded, "prefetch-hit");
            return;
        }

        if self.load_tx.send(LoadRequest { id, path }).is_err() {
            self.error = Some(String::from("loader de imagens foi encerrado"));
            return;
        }
        ctx.request_repaint_after(LOAD_POLL_INTERVAL);
    }

    pub fn ensure_prefetched(
        &mut self,
        photos: &[crate::fs_browser::PhotoPath],
        center: usize,
        budget_bytes: u64,
    ) {
        self.prefetch_budget_bytes = budget_bytes;
        if photos.is_empty() || budget_bytes == 0 {
            self.clear_prefetch();
            return;
        }
        if self.prefetch_bytes >= budget_bytes {
            return;
        }

        let generation = self.prefetch_generation;
        for distance in -PREFETCH_RADIUS..=PREFETCH_RADIUS {
            if distance == 0 || self.inflight.len() >= PREFETCH_MAX_INFLIGHT {
                continue;
            }
            let Some(index) = center
                .checked_add_signed(distance)
                .filter(|index| *index < photos.len())
            else {
                continue;
            };
            let path = photos[index].path().to_path_buf();
            if self.prefetch.contains_key(&path) || !self.inflight.insert(path.clone()) {
                continue;
            }
            let tx = self.pre_tx.clone();
            std::thread::spawn(move || {
                let result = decode_photo(&path).ok();
                let _ = tx.send(PrefetchMsg {
                    generation,
                    path,
                    result,
                });
            });
        }
    }

    pub fn poll(&mut self, ctx: &egui::Context) -> bool {
        while let Ok(message) = self.pre_rx.try_recv() {
            if message.generation != self.prefetch_generation {
                continue;
            }
            self.inflight.remove(&message.path);
            if let Some(decoded) = message.result {
                self.prefetch_bytes += decoded_bytes(&decoded);
                self.prefetch_order.push_back(message.path.clone());
                self.prefetch.insert(message.path, decoded);
            }
        }
        self.trim_prefetch();

        let mut pending = self.has_selection && self.texture.is_none() && self.error.is_none();
        while let Ok(message) = self.rx.try_recv() {
            if message.id != self.current_id {
                continue;
            }
            pending = false;
            match message.result {
                Ok(decoded) => self.install_base(ctx, decoded, "base"),
                Err(error) => {
                    self.error = Some(format!("{}: {error}", message.path.display()));
                }
            }
        }

        if pending {
            ctx.request_repaint_after(LOAD_POLL_INTERVAL);
        }
        pending
    }

    fn trim_prefetch(&mut self) {
        while self.prefetch.len() > PREFETCH_CAP
            || (self.prefetch_budget_bytes == 0 && !self.prefetch.is_empty())
            || (self.prefetch_budget_bytes > 0 && self.prefetch_bytes > self.prefetch_budget_bytes)
        {
            let Some(oldest) = self.prefetch_order.pop_front() else {
                break;
            };
            if let Some(evicted) = self.prefetch.remove(&oldest) {
                self.prefetch_bytes = self.prefetch_bytes.saturating_sub(decoded_bytes(&evicted));
            }
        }
    }

    pub fn rebuild_preview(&mut self, ctx: &egui::Context, state: &EditorState) {
        let Some(base) = self.display_img.as_ref() else {
            return;
        };
        let edited = crate::editor::apply_to_image(base, state);
        let (texture, size) = self.texture_from_image(ctx, &edited, "preview");
        self.texture = Some(texture);
        self.display_px = size;
    }

    pub fn restore_base(&mut self, _ctx: &egui::Context) {
        let Some(texture) = self.base_texture.as_ref() else {
            return;
        };
        self.texture = Some(texture.clone());
        self.display_px = self.base_display_px;
    }

    #[must_use]
    pub fn display_base_dims(&self) -> Option<(u32, u32)> {
        self.display_img
            .as_ref()
            .map(|display| (display.width(), display.height()))
    }

    #[must_use]
    pub fn state(&self) -> LoadState {
        if !self.has_selection {
            return LoadState::Empty;
        }
        if let Some(error) = &self.error {
            return LoadState::Failed(error.clone());
        }
        match &self.texture {
            Some(texture) => LoadState::Loaded {
                texture: texture.clone(),
                display_px: self.display_px,
                full_px: self.full_px,
            },
            None => LoadState::Loading,
        }
    }

    pub fn clear_prefetch(&mut self) {
        self.prefetch_generation = self.prefetch_generation.wrapping_add(1);
        self.prefetch.clear();
        self.prefetch_order.clear();
        self.prefetch_bytes = 0;
        self.inflight.clear();
    }
}

impl Default for ImageStore {
    fn default() -> Self {
        Self::new()
    }
}

