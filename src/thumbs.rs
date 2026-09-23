//! Faixa de thumbnails: worker único com fila + cache de texturas.
//!
//! O worker decodifica (com correção EXIF) para no máximo [`THUMB_MAX`] px;
//! `update()` recebe o intervalo realmente visível da galeria, agenda uma
//! pequena margem, drena resultados criando texturas e despeja os distantes.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use crate::exif::{apply_orientation, read_orientation};

/// Maior lado do thumbnail.
pub const THUMB_MAX: u32 = 160;
/// Margem de itens antes/depois do viewport mantida quente.
const THUMB_MARGIN: usize = 32;
/// Teto de texturas; além disso, despeja fora da janela.
const THUMB_CAP: usize = 200;
/// Novos jobs por frame (não sufocar a UI).
const JOBS_PER_FRAME: usize = 12;

struct ThumbMsg {
    path: PathBuf,
    result: Option<(egui::ColorImage, (u32, u32))>,
}

fn decode_thumb(path: &PathBuf) -> Option<(egui::ColorImage, (u32, u32))> {
    let raw = image::ImageReader::open(path).ok()?.decode().ok()?;
    let oriented = apply_orientation(raw, read_orientation(path));
    let thumb = oriented.thumbnail(THUMB_MAX, THUMB_MAX);
    let rgba = thumb.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let color = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw());
    Some((color, (w, h)))
}

/// Cache de thumbnails com worker em background.
pub struct ThumbCache {
    tx: Sender<PathBuf>,
    rx: Receiver<ThumbMsg>,
    cache: HashMap<PathBuf, egui::TextureHandle>,
    queued: HashSet<PathBuf>,
    failed: HashSet<PathBuf>,
    seq: u64,
}

impl ThumbCache {
    /// Cria e dispara o worker.
    #[must_use]
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<PathBuf>();
        let (res_tx, res_rx) = mpsc::channel::<ThumbMsg>();
        std::thread::spawn(move || {
            while let Ok(path) = rx.recv() {
                let result = decode_thumb(&path);
                if res_tx.send(ThumbMsg { path, result }).is_err() {
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
        }
    }

    /// Textura pronta, se houver.
    #[must_use]
    pub fn get(&self, path: &std::path::Path) -> Option<&egui::TextureHandle> {
        self.cache.get(path)
    }

    /// Atualiza fila + drena prontos + despeja distantes.
    /// Só pede repaint quando há trabalho (fila) ou progresso (resultado).
    /// Candidatos ordenados por tamanho do arquivo: thumbs de JPGs pequenos
    /// aparecem primeiro; TIFFs gigantes resolvem por último sem bloquear.
    pub fn update(
        &mut self,
        ctx: &egui::Context,
        visible: &[crate::fs_browser::PhotoPath],
        viewport: Option<(usize, usize)>,
        sel: Option<usize>,
    ) {
        // Sempre drena resultados prontos, mesmo quando a galeria não está visível.
        let (view_start, view_end) = viewport
            .map(|(start, end)| (start.min(visible.len()), end.min(visible.len())))
            .unwrap_or((0, 0));
        let (lo, hi_exclusive) =
            expanded_viewport_range(visible.len(), view_start, view_end, THUMB_MARGIN);

        let mut candidate_indices: Vec<usize> = (lo..hi_exclusive).collect();
        if let Some(selected) = sel.filter(|selected| *selected < visible.len())
            && !candidate_indices.contains(&selected)
        {
            candidate_indices.insert(0, selected);
        }

        let mut candidates: Vec<PathBuf> = candidate_indices
            .into_iter()
            .map(|index| visible[index].path().to_path_buf())
            .filter(|path| {
                !self.cache.contains_key(path)
                    && !self.failed.contains(path)
                    && !self.queued.contains(path)
            })
            .collect();
        // Baratos primeiro (metadados; falha = por último).
        candidates.sort_by_key(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(u64::MAX));
        let mut sent = 0;
        for path in candidates.into_iter().take(JOBS_PER_FRAME) {
            if !self.queued.insert(path.clone()) {
                continue;
            }
            if self.tx.send(path).is_err() {
                break;
            }
            sent += 1;
        }

        let mut received = 0;
        while let Ok(msg) = self.rx.try_recv() {
            received += 1;
            self.queued.remove(&msg.path);
            match msg.result {
                Some((color, _)) => {
                    self.seq += 1;
                    let tex = ctx.load_texture(
                        format!("thumb-{}-{}", self.seq, msg.path.display()),
                        color,
                        egui::TextureOptions::LINEAR,
                    );
                    self.cache.insert(msg.path, tex);
                }
                None => {
                    self.failed.insert(msg.path);
                }
            }
        }

        if self.cache.len() > THUMB_CAP {
            let mut keep: HashSet<PathBuf> = visible[lo..hi_exclusive]
                .iter()
                .map(|photo| photo.path().to_path_buf())
                .collect();
            if let Some(selected) = sel.and_then(|index| visible.get(index)) {
                keep.insert(selected.path().to_path_buf());
            }
            self.cache.retain(|path, _| keep.contains(path));
            self.failed.retain(|path| keep.contains(path));
        }
        if sent > 0 || received > 0 || !self.queued.is_empty() {
            ctx.request_repaint();
        }
    }

    /// Esquece uma foto (ex.: após sobrescrever o arquivo).
    pub fn invalidate(&mut self, path: &std::path::Path) {
        self.cache.remove(path);
        self.failed.remove(path);
    }

    /// Limpa tudo (troca de pasta/arquivos).
    pub fn clear(&mut self) {
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

/// Expande o viewport por uma margem, sempre limitado ao conjunto real.
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
}
