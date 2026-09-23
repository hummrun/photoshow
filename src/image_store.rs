//! Cache de imagens: decode em worker dedicado -> textura egui.
//!
//! O decode core produz buffers neutros. O egui aparece somente nesta camada de
//! adaptação/upload. A full-resolution não fica residente: ela é decodificada
//! sob demanda por operações como save/copy.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use thiserror::Error;

use crate::editor::EditorState;
use crate::exif::{apply_orientation, read_orientation};
use crate::media::pixels::RgbaFrame;

/// Maior lado (px) da versão de display.
pub const DISPLAY_MAX_DIM: u32 = 2048;
const PREFETCH_RADIUS: isize = 2;
const PREFETCH_CAP: usize = 4;
const PREFETCH_MAX_INFLIGHT: usize = 2;
const LOAD_POLL_INTERVAL: Duration = Duration::from_millis(16);

fn decoded_bytes(decoded: &DecodedPhoto) -> u64 {
    decoded.display.as_bytes().len() as u64 + decoded.frame.byte_len() as u64
}

/// Erros de carregamento de foto.
#[derive(Debug, Error)]
pub enum LoadError {
    #[error("io: {0}")]
    Io(String),
    #[error("decode: {0}")]
    Decode(String),
}

impl From<image::ImageError> for LoadError {
    fn from(error: image::ImageError) -> Self {
        Self::Decode(error.to_string())
    }
}

/// Decodifica a imagem completa, aplica orientação EXIF e não cria cópias
/// permanentes. Use somente em operações que realmente precisam de full-res.
pub fn decode_full_photo(path: &Path) -> Result<image::DynamicImage, LoadError> {
    let reader = image::ImageReader::open(path).map_err(|e| LoadError::Io(e.to_string()))?;
    let raw = reader.decode()?;
    Ok(apply_orientation(raw, read_orientation(path)))
}

/// Foto pronta para o viewer/prefetch. Não contém full-resolution.
#[derive(Debug)]
pub struct DecodedPhoto {
    pub display: image::DynamicImage,
    pub frame: RgbaFrame,
    pub full_size: (u32, u32),
}

/// Decodifica, orienta e reduz para o limite do viewer.
///
/// Para imagens pequenas, a própria imagem orientada vira display em vez de ser
/// clonada. Para imagens grandes, a full-res existe apenas durante a criação do
/// thumbnail de display e é liberada antes do resultado entrar no cache.
pub fn decode_photo(path: &Path) -> Result<DecodedPhoto, LoadError> {
    let full = decode_full_photo(path)?;
    let full_size = (full.width(), full.height());
    let display = if full.width().max(full.height()) > DISPLAY_MAX_DIM {
        full.thumbnail(DISPLAY_MAX_DIM, DISPLAY_MAX_DIM)
    } else {
        full
    };
    let frame = RgbaFrame::from_dynamic(&display);
    Ok(DecodedPhoto {
        display,
        frame,
        full_size,
    })
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
    path: PathBuf,
    result: Option<DecodedPhoto>,
}

/// Estado visível do carregamento atual.
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

/// Guarda seleção atual, display CPU e textura GPU.
///
/// O loader principal é um único worker. Ao terminar um decode ele drena pedidos
/// pendentes e processa apenas o mais novo (latest-wins), evitando uma explosão
/// de threads durante navegação rápida.
pub struct ImageStore {
    load_tx: Sender<LoadRequest>,
    rx: Receiver<LoadMsg>,
    next_id: u64,
    tex_seq: u64,
    current_id: u64,
    texture: Option<egui::TextureHandle>,
    display_px: egui::Vec2,
    display_img: Option<image::DynamicImage>,
    display_frame: Option<RgbaFrame>,
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
}

impl ImageStore {
    #[must_use]
    pub fn new() -> Self {
        let (load_tx, load_rx) = mpsc::channel::<LoadRequest>();
        let (result_tx, result_rx) = mpsc::channel::<LoadMsg>();
        std::thread::spawn(move || {
            while let Ok(mut request) = load_rx.recv() {
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
            display_px: egui::Vec2::ZERO,
            display_img: None,
            display_frame: None,
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
        }
    }

    fn upload_frame(&mut self, ctx: &egui::Context, frame: &RgbaFrame, tag: &str) {
        self.tex_seq += 1;
        self.display_px = egui::Vec2::new(frame.width() as f32, frame.height() as f32);
        let color = egui::ColorImage::from_rgba_unmultiplied(
            [frame.width() as usize, frame.height() as usize],
            frame.bytes(),
        );
        self.texture = Some(ctx.load_texture(
            format!("photo-{tag}-{}", self.tex_seq),
            color,
            egui::TextureOptions::LINEAR,
        ));
    }

    /// Seleciona foto: consome prefetch imediatamente ou envia ao loader
    /// latest-wins. A full-resolution não é retida.
    pub fn select(&mut self, ctx: &egui::Context, photo: &crate::fs_browser::PhotoPath) {
        self.next_id += 1;
        let id = self.next_id;
        self.current_id = id;
        self.has_selection = true;
        self.texture = None;
        self.display_img = None;
        self.display_frame = None;
        self.error = None;
        let path = photo.path().to_path_buf();

        if let Some(decoded) = self.prefetch.remove(&path) {
            self.prefetch_bytes = self.prefetch_bytes.saturating_sub(decoded_bytes(&decoded));
            self.full_px = decoded.full_size;
            self.upload_frame(ctx, &decoded.frame, "hit");
            self.display_img = Some(decoded.display);
            self.display_frame = Some(decoded.frame);
            return;
        }

        if self.load_tx.send(LoadRequest { id, path }).is_err() {
            self.error = Some(String::from("loader de imagens foi encerrado"));
            return;
        }
        ctx.request_repaint_after(LOAD_POLL_INTERVAL);
    }

    /// Mantém um pequeno cache de displays vizinhos. O limite é RAM decodificada,
    /// não tamanho comprimido do arquivo. No máximo dois decodes de prefetch
    /// ficam simultaneamente em voo.
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
                let _ = tx.send(PrefetchMsg { path, result });
            });
        }
    }

    /// Drena resultados do loader e do prefetch.
    pub fn poll(&mut self, ctx: &egui::Context) -> bool {
        while let Ok(message) = self.pre_rx.try_recv() {
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
                Ok(decoded) => {
                    self.full_px = decoded.full_size;
                    self.upload_frame(ctx, &decoded.frame, "base");
                    self.display_img = Some(decoded.display);
                    self.display_frame = Some(decoded.frame);
                }
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
            || (self.prefetch_budget_bytes > 0
                && self.prefetch_bytes > self.prefetch_budget_bytes)
        {
            let Some(oldest) = self.prefetch_order.pop_front() else {
                break;
            };
            if let Some(evicted) = self.prefetch.remove(&oldest) {
                self.prefetch_bytes = self.prefetch_bytes.saturating_sub(decoded_bytes(&evicted));
            }
        }
    }

    /// Reconstrói o preview editado. O próximo passo de performance é mover esta
    /// transformação para um worker; o decode e a preparação RGBA já estão fora
    /// da thread da UI no caminho normal de carregamento.
    pub fn rebuild_preview(&mut self, ctx: &egui::Context, state: &EditorState) {
        let Some(base) = self.display_img.as_ref() else {
            return;
        };
        let edited = crate::editor::apply_to_image(base, state);
        let frame = RgbaFrame::from_dynamic(&edited);
        self.upload_frame(ctx, &frame, "preview");
    }

    pub fn restore_base(&mut self, ctx: &egui::Context) {
        let Some(frame) = self.display_frame.as_ref() else {
            return;
        };
        let color = egui::ColorImage::from_rgba_unmultiplied(
            [frame.width() as usize, frame.height() as usize],
            frame.bytes(),
        );
        self.tex_seq += 1;
        self.display_px = egui::Vec2::new(frame.width() as f32, frame.height() as f32);
        self.texture = Some(ctx.load_texture(
            format!("photo-base-{}", self.tex_seq),
            color,
            egui::TextureOptions::LINEAR,
        ));
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

    fn write_test_png(dir: &Path, name: &str, width: u32, height: u32) -> PathBuf {
        let image = image::RgbImage::from_fn(width, height, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        });
        let path = dir.join(name);
        image.save(&path).expect("save png");
        path
    }

    #[test]
    fn decode_small_image_keeps_size_without_full_copy() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_test_png(dir.path(), "s.png", 32, 24);
        let decoded = decode_photo(&path).expect("decode");

        assert_eq!(decoded.full_size, (32, 24));
        assert_eq!((decoded.display.width(), decoded.display.height()), (32, 24));
        assert_eq!(decoded.frame.byte_len(), 32 * 24 * 4);
    }

    #[test]
    fn decode_large_image_downscales_display() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_test_png(dir.path(), "big.png", 3000, 2000);
        let decoded = decode_photo(&path).expect("decode");

        assert_eq!(decoded.full_size, (3000, 2000));
        assert!(decoded.display.width().max(decoded.display.height()) <= DISPLAY_MAX_DIM);
    }

    #[test]
    fn full_decode_is_available_only_on_demand() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_test_png(dir.path(), "full.png", 64, 48);
        let decoded = decode_full_photo(&path).expect("full decode");

        assert_eq!((decoded.width(), decoded.height()), (64, 48));
    }

    #[test]
    fn decode_missing_file_errors() {
        let error =
            decode_photo(Path::new("/nao/existe/foto.png")).expect_err("deveria falhar");
        assert!(matches!(error, LoadError::Io(_)));
    }
}
