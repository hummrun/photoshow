//! Cache de imagens: decode em background -> texturas egui.
//!
//! Fluxo: `select()` dispara uma thread que decodifica (com correção EXIF)
//! e reduz para display (max [`DISPLAY_MAX_DIM`] px); o resultado volta por
//! `mpsc` com id de geração — obsoletos são descartados. `poll()` cria a
//! `TextureHandle` e pede repaint enquanto há carga pendente.
//!
//! extras Fase 3/4:
//! - `ensure_prefetched()`: decodifica vizinhos (±2) em background; `select()`
//!   consome o cache e vira instantâneo (cap 4, path-keyed).
//! - `rebuild_preview()` / `restore_base()`: aplica o [`EditorState`] na
//!   imagem de display e troca a textura (preview de rotate/crop).

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

use thiserror::Error;

use crate::editor::EditorState;
use crate::exif::{apply_orientation, read_orientation};

/// Maior lado (px) da versão de display. Full-res fica em `full`.
pub const DISPLAY_MAX_DIM: u32 = 2048;
/// Vizinhos pré-decodificados para cada lado + tamanho do cache.
const PREFETCH_RADIUS: isize = 2;
const PREFETCH_CAP: usize = 4;
/// Teto de RAM do prefetch (full-res de arquivos grandes pesa GBs).
const PREFETCH_BYTES_CAP: u64 = 512 * 1024 * 1024;

/// Estima a RAM de uma foto decodificada (RGBA, full + display).
fn decoded_bytes(dec: &DecodedPhoto) -> u64 {
    let full = dec.full.width() as u64 * dec.full.height() as u64 * 4;
    let disp = dec.display.width() as u64 * dec.display.height() as u64 * 4;
    full + disp
}

/// Erros de carregamento de foto.
#[derive(Debug, Error)]
pub enum LoadError {
    /// Falha de IO ao ler o arquivo.
    #[error("io: {0}")]
    Io(String),
    /// Falha ao decodificar (formato/arquivo corrompido).
    #[error("decode: {0}")]
    Decode(String),
}

impl From<image::ImageError> for LoadError {
    fn from(e: image::ImageError) -> Self {
        Self::Decode(e.to_string())
    }
}

/// Foto decodificada: full-res corrigida + versão de display (orientada).
#[derive(Debug)]
pub struct DecodedPhoto {
    /// Imagem original corrigida (para bake da Fase 4).
    pub full: image::DynamicImage,
    /// Versão reduzida orientada (para display e preview do editor).
    pub display: image::DynamicImage,
    /// Dimensões (w, h) da full-res.
    pub full_size: (u32, u32),
}

/// Decodifica + corrige EXIF + reduz. Função pura para facilitar teste.
pub fn decode_photo(path: &Path) -> Result<DecodedPhoto, LoadError> {
    let reader = image::ImageReader::open(path).map_err(|e| LoadError::Io(e.to_string()))?;
    let raw = reader.decode()?;
    let full = apply_orientation(raw, read_orientation(path));
    let full_size = (full.width(), full.height());
    let display = if full.width().max(full.height()) > DISPLAY_MAX_DIM {
        full.thumbnail(DISPLAY_MAX_DIM, DISPLAY_MAX_DIM)
    } else {
        full.clone()
    };
    Ok(DecodedPhoto {
        full,
        display,
        full_size,
    })
}

/// Converte para upload em GPU.
fn to_color(img: &image::DynamicImage) -> egui::ColorImage {
    let rgba = img.to_rgba8();
    egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    )
}

struct LoadMsg {
    id: u64,
    path: PathBuf,
    result: Result<DecodedPhoto, LoadError>,
}

struct PrefetchMsg {
    path: PathBuf,
    result: Option<DecodedPhoto>,
}

/// Estado visível do carregamento atual.
pub enum LoadState {
    /// Nada selecionado.
    Empty,
    /// Decodificando em background.
    Loading,
    /// Pronta para exibir.
    Loaded {
        /// Textura atual (base ou preview do editor).
        texture: egui::TextureHandle,
        /// Tamanho em px da imagem exibida (pós-rotate do editor).
        display_px: egui::Vec2,
        /// Tamanho em px da full-res.
        full_px: (u32, u32),
    },
    /// Falha (msg para status bar).
    Failed(String),
}

/// Guarda seleção atual + textura; `display_img` alimenta o editor.
pub struct ImageStore {
    tx: Sender<LoadMsg>,
    rx: Receiver<LoadMsg>,
    next_id: u64,
    tex_seq: u64,
    current_id: u64,
    texture: Option<egui::TextureHandle>,
    display_px: egui::Vec2,
    display_img: Option<image::DynamicImage>,
    full: Option<image::DynamicImage>,
    full_px: (u32, u32),
    error: Option<String>,
    has_selection: bool,
    // Prefetch de vizinhos.
    pre_tx: Sender<PrefetchMsg>,
    pre_rx: Receiver<PrefetchMsg>,
    prefetch: HashMap<PathBuf, DecodedPhoto>,
    prefetch_order: VecDeque<PathBuf>,
    prefetch_bytes: u64,
    inflight: HashSet<PathBuf>,
}

impl ImageStore {
    /// Cria vazio.
    #[must_use]
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let (pre_tx, pre_rx) = mpsc::channel();
        Self {
            tx,
            rx,
            next_id: 0,
            tex_seq: 0,
            current_id: 0,
            texture: None,
            display_px: egui::Vec2::ZERO,
            display_img: None,
            full: None,
            full_px: (0, 0),
            error: None,
            has_selection: false,
            pre_tx,
            pre_rx,
            prefetch: HashMap::new(),
            prefetch_order: VecDeque::new(),
            prefetch_bytes: 0,
            inflight: HashSet::new(),
        }
    }

    fn upload(&mut self, ctx: &egui::Context, img: &image::DynamicImage, tag: &str) {
        self.tex_seq += 1;
        self.display_px = egui::Vec2::new(img.width() as f32, img.height() as f32);
        self.texture = Some(ctx.load_texture(
            format!("photo-{tag}-{}", self.tex_seq),
            to_color(img),
            egui::TextureOptions::LINEAR,
        ));
    }

    /// Seleciona foto: consome prefetch (instantâneo) ou decodifica em background.
    pub fn select(&mut self, ctx: &egui::Context, photo: &crate::fs_browser::PhotoPath) {
        self.next_id += 1;
        let id = self.next_id;
        self.current_id = id;
        self.has_selection = true;
        self.texture = None;
        self.display_img = None;
        self.full = None;
        self.error = None;
        let path = photo.path().to_path_buf();

        if let Some(dec) = self.prefetch.remove(&path) {
            self.prefetch_bytes = self.prefetch_bytes.saturating_sub(decoded_bytes(&dec));
            self.display_px =
                egui::Vec2::new(dec.display.width() as f32, dec.display.height() as f32);
            self.full_px = dec.full_size;
            self.display_img = Some(dec.display);
            self.full = Some(dec.full);
            let base = self.display_img.as_ref().expect("display recém-guardado");
            let base = base.clone();
            self.upload(ctx, &base, "hit");
            return;
        }

        let tx = self.tx.clone();
        let thread_ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = decode_photo(&path);
            let _ = tx.send(LoadMsg { id, path, result });
            thread_ctx.request_repaint();
        });
        ctx.request_repaint();
    }

    /// Garante prefetch dos vizinhos [center-R, center+R] (chamar todo frame; barato).
    /// `max_file_bytes`: pula arquivos maiores (0 = prefetch desativado).
    /// Metadados (tamanho) são baratos; o decode pesado fica na thread.
    pub fn ensure_prefetched(
        &mut self,
        photos: &[crate::fs_browser::PhotoPath],
        center: usize,
        max_file_bytes: u64,
    ) {
        if photos.is_empty() || max_file_bytes == 0 {
            return;
        }
        if photos.is_empty() {
            return;
        }
        for d in -PREFETCH_RADIUS..=PREFETCH_RADIUS {
            if d == 0 {
                continue;
            }
            let Some(i) = center.checked_add_signed(d).filter(|i| *i < photos.len()) else {
                continue;
            };
            let path = photos[i].path().to_path_buf();
            if self.prefetch.contains_key(&path) || !self.inflight.insert(path.clone()) {
                continue;
            }
            // Guarda barato: arquivo gigante nem entra na fila de decode.
            let small_enough = std::fs::metadata(&path)
                .map(|m| m.len() <= max_file_bytes)
                .unwrap_or(true);
            if !small_enough {
                self.inflight.remove(&path);
                continue;
            }
            let tx = self.pre_tx.clone();
            std::thread::spawn(move || {
                let result = decode_photo(&path).ok();
                let _ = tx.send(PrefetchMsg { path, result });
            });
        }
    }

    /// Drena resultados (principal + prefetch). Retorna `true` se há carga pendente.
    pub fn poll(&mut self, ctx: &egui::Context) -> bool {
        while let Ok(msg) = self.pre_rx.try_recv() {
            self.inflight.remove(&msg.path);
            if let Some(dec) = msg.result {
                self.prefetch_bytes += decoded_bytes(&dec);
                self.prefetch_order.push_back(msg.path.clone());
                self.prefetch.insert(msg.path, dec);
                // Despeja os mais antigos por contagem E por bytes.
                while self.prefetch.len() > PREFETCH_CAP || self.prefetch_bytes > PREFETCH_BYTES_CAP
                {
                    if let Some(old) = self.prefetch_order.pop_front() {
                        if let Some(evicted) = self.prefetch.remove(&old) {
                            self.prefetch_bytes =
                                self.prefetch_bytes.saturating_sub(decoded_bytes(&evicted));
                        }
                    } else {
                        break;
                    }
                }
            }
        }
        let mut pending = self.has_selection && self.texture.is_none() && self.error.is_none();
        while let Ok(msg) = self.rx.try_recv() {
            if msg.id != self.current_id {
                continue; // obsoleto: usuário já navegou para outra foto
            }
            pending = false;
            match msg.result {
                Ok(dec) => {
                    self.full_px = dec.full_size;
                    self.display_img = Some(dec.display);
                    self.full = Some(dec.full);
                    let base = self.display_img.as_ref().expect("display recém-guardado");
                    let base = base.clone();
                    self.upload(ctx, &base, "base");
                }
                Err(e) => self.error = Some(format!("{}: {e}", msg.path.display())),
            }
        }
        if pending {
            ctx.request_repaint();
        }
        pending
    }

    /// Reconstrói a textura aplicando o estado do editor na imagem de display.
    pub fn rebuild_preview(&mut self, ctx: &egui::Context, state: &EditorState) {
        let Some(base) = self.display_img.clone() else {
            return;
        };
        let edited = crate::editor::apply_to_image(&base, state);
        self.upload(ctx, &edited, "preview");
    }

    /// Restaura a textura base (sem edições).
    pub fn restore_base(&mut self, ctx: &egui::Context) {
        let Some(base) = self.display_img.clone() else {
            return;
        };
        self.upload(ctx, &base, "base");
    }

    /// Dimensões da imagem de display pré-edição (para rotate/bake).
    #[must_use]
    pub fn display_base_dims(&self) -> Option<(u32, u32)> {
        self.display_img.as_ref().map(|d| (d.width(), d.height()))
    }

    /// Cópia da full-res para o thread de salvamento.
    #[must_use]
    pub fn full_image(&self) -> Option<image::DynamicImage> {
        self.full.clone()
    }

    /// Estado atual para a UI.
    #[must_use]
    pub fn state(&self) -> LoadState {
        if !self.has_selection {
            return LoadState::Empty;
        }
        if let Some(e) = &self.error {
            return LoadState::Failed(e.clone());
        }
        match &self.texture {
            Some(t) => LoadState::Loaded {
                texture: t.clone(),
                display_px: self.display_px,
                full_px: self.full_px,
            },
            None => LoadState::Loading,
        }
    }

    /// Limpa prefetch (troca de pasta/arquivos).
    pub fn clear_prefetch(&mut self) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn write_test_png(dir: &Path, name: &str, w: u32, h: u32) -> PathBuf {
        let img = image::RgbImage::from_fn(w, h, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        });
        let path = dir.join(name);
        img.save(&path).expect("save png");
        path
    }

    #[test]
    fn decode_small_image_keeps_size() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_test_png(dir.path(), "s.png", 32, 24);
        let dec = decode_photo(&path).expect("decode");
        assert_eq!(dec.full_size, (32, 24));
        assert_eq!((dec.display.width(), dec.display.height()), (32, 24));
    }

    #[test]
    fn decode_large_image_downscales_display() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_test_png(dir.path(), "big.png", 3000, 2000);
        let dec = decode_photo(&path).expect("decode");
        assert_eq!(dec.full_size, (3000, 2000));
        assert!(dec.display.width().max(dec.display.height()) <= DISPLAY_MAX_DIM);
    }

    #[test]
    fn decode_missing_file_errors() {
        let err = decode_photo(Path::new("/nao/existe/foto.png")).expect_err("deveria falhar");
        assert!(matches!(err, LoadError::Io(_)));
    }
}
