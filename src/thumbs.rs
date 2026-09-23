//! Viewport-driven thumbnail cache with bounded background work.
//!
//! The UI enqueues only thumbnails near the visible viewport. Queue growth is
//! capped, folder resets advance a generation token, and the worker rejects stale
//! generation requests before decoding them.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use crate::media::decoder::decode_full_photo;

pub const THUMB_MAX: u32 = 160;
const THUMB_MARGIN: usize = 32;
const THUMB_CAP: usize = 200;
const THUMB_QUEUE_CAP: usize = 48;
const JOBS_PER_FRAME: usize = 12;
const THUMB_POLL_INTERVAL: Duration = Duration::from_millis(24);

struct ThumbRequest {
    generation: u64,
    path: PathBuf,
}

struct ThumbMsg {
    generation: u64,
    path: PathBuf,
    result: Option<egui::ColorImage>,
}

fn decode_thumb(path: &PathBuf) -> Option<egui::ColorImage> {
    let oriented = decode_full_photo(path).ok()?;
    let thumb = oriented.thumbnail(THUMB_MAX, THUMB_MAX);
    let rgba = thumb.to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    ))
}

pub struct ThumbCache {
    tx: Sender<ThumbRequest>,
    rx: Receiver<ThumbMsg>,
    cache: HashMap<PathBuf, egui::TextureHandle>,
    queued: HashSet<PathBuf>,
    failed: HashSet<PathBuf>,
    seq: u64,
    generation: u64,
    worker_generation: Arc<AtomicU64>,
}

impl ThumbCache {
    #[must_use]
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<ThumbRequest>();
        let (res_tx, res_rx) = mpsc::channel::<ThumbMsg>();
        let worker_generation = Arc::new(AtomicU64::new(0));
        let worker_generation_ref = Arc::clone(&worker_generation);

        std::thread::spawn(move || {
            while let Ok(request) = rx.recv() {
                // A folder/cache reset can invalidate many queued requests at
                // once. Reject them before opening or decoding the file.
                if request.generation != worker_generation_ref.load(Ordering::Acquire) {
                    continue;
                }
                let result = decode_thumb(&request.path);
                if res_tx
                    .send(ThumbMsg {
                        generation: request.generation,
                        path: request.path,
                        result,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        Self {
            tx,
            rx: res_rx,
            cache: HashMap::new(),
            queued: HashSet::new(),
            failed: HashSet::new(),
            seq: 0,
            generation: 0,
            worker_generation,
        }
    }

    #[must_use]
    pub fn get(&self, path: &std::path::Path) -> Option<&egui::TextureHandle> {
        self.cache.get(path)
    }

    pub fn update(
        &mut self,
        ctx: &egui::Context,
        visible: &[crate::fs_browser::PhotoPath],
        viewport: Option<(usize, usize)>,
        sel: Option<usize>,
    ) {
        let (view_start, view_end) = viewport
            .map(|(start, end)| (start.min(visible.len()), end.min(visible.len())))
            .unwrap_or((0, 0));
        let (lo, hi_exclusive) =
            expanded_viewport_range(visible.len(), view_start, view_end, THUMB_MARGIN);

        let mut keep: HashSet<PathBuf> = visible[lo..hi_exclusive]
            .iter()
            .map(|photo| photo.path().to_path_buf())
            .collect();
        if let Some(selected) = sel.and_then(|index| visible.get(index)) {
            keep.insert(selected.path().to_path_buf());
        }

        // Failed entries should be bounded even when every decode fails and the
        // texture cache never reaches THUMB_CAP.
        self.failed.retain(|path| keep.contains(path));

        let mut candidate_indices: Vec<usize> = (lo..hi_exclusive).collect();
        if let Some(selected) = sel.filter(|selected| *selected < visible.len())
            && !candidate_indices.contains(&selected)
        {
            candidate_indices.insert(0, selected);
        }

        let available_slots = THUMB_QUEUE_CAP.saturating_sub(self.queued.len());
        let request_limit = JOBS_PER_FRAME.min(available_slots);
        let mut candidates: Vec<PathBuf> = candidate_indices
            .into_iter()
            .map(|index| visible[index].path().to_path_buf())
            .filter(|path| {
                !self.cache.contains_key(path)
                    && !self.failed.contains(path)
                    && !self.queued.contains(path)
            })
            .collect();

        // Small compressed files are usually cheap wins. This is scheduling only;
        // memory accounting never uses compressed file size.
        candidates.sort_by_key(|path| {
            std::fs::metadata(path)
                .map(|metadata| metadata.len())
                .unwrap_or(u64::MAX)
        });

        let mut sent = 0usize;
        for path in candidates.into_iter().take(request_limit) {
            if !self.queued.insert(path.clone()) {
                continue;
            }
            if self
                .tx
                .send(ThumbRequest {
                    generation: self.generation,
                    path: path.clone(),
                })
                .is_err()
            {
                self.queued.remove(&path);
                break;
            }
            sent += 1;
        }

        let mut received = 0usize;
        while let Ok(message) = self.rx.try_recv() {
            if message.generation != self.generation {
                continue;
            }
            received += 1;
            self.queued.remove(&message.path);
            match message.result {
                Some(color) => {
                    self.seq += 1;
                    let texture = ctx.load_texture(
                        format!("thumb-{}-{}", self.seq, message.path.display()),
                        color,
                        egui::TextureOptions::LINEAR,
                    );
                    self.cache.insert(message.path, texture);
                }
                None => {
                    self.failed.insert(message.path);
                }
            }
        }

        if self.cache.len() > THUMB_CAP {
            self.cache.retain(|path, _| keep.contains(path));
        }

        if sent > 0 || received > 0 {
            ctx.request_repaint();
        } else if !self.queued.is_empty() {
            ctx.request_repaint_after(THUMB_POLL_INTERVAL);
        }
    }

    pub fn invalidate(&mut self, path: &std::path::Path) {
        self.cache.remove(path);
        self.failed.remove(path);
    }

    pub fn clear(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.worker_generation
            .store(self.generation, Ordering::Release);
        self.cache.clear();
        self.queued.clear();
        self.failed.clear();
    }
}

impl Default for ThumbCache {
    fn default() -> Self {
        Self::new()
    }
}

#[must_use]
pub fn expanded_viewport_range(
    len: usize,
    start: usize,
    end: usize,
    margin: usize,
) -> (usize, usize) {
    let start = start.min(len);
    let end = end.max(start).min(len);
    (
        start.saturating_sub(margin),
        end.saturating_add(margin).min(len),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_margin_clamps_at_edges() {
        assert_eq!(expanded_viewport_range(0, 0, 0, 32), (0, 0));
        assert_eq!(expanded_viewport_range(10, 0, 3, 2), (0, 5));
        assert_eq!(expanded_viewport_range(100, 40, 50, 5), (35, 55));
        assert_eq!(expanded_viewport_range(100, 95, 100, 10), (85, 100));
    }

    #[test]
    fn viewport_margin_normalizes_inverted_range() {
        assert_eq!(expanded_viewport_range(100, 60, 40, 5), (55, 65));
    }
}
