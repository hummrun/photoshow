//! App principal: toolbar, browser (favoritas + árvore + arquivos), viewer,
//! filmstrip, painel de edição, status bar, config e menus de contexto.
//!
//! Edição: rotate ⟲/⟳ com preview, crop por arrasto (mover + gizmos,
//! proporção opcional, Enter aplica), undo/redo (Ctrl+Z/Y), Salvar e
//! Salvar como (thread, qualidade JPEG configurável).

mod browser;
mod filmstrip;
mod viewer;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

use crate::config::{AppConfig, THEMES};
use crate::editor::{CropRect, EditorStack, bake, save_baked_atomic};
use crate::fs_browser::{self, PhotoPath, ScanOptions, ScanResult};
use crate::icons::{self, labeled};
use crate::image_store::{ImageStore, LoadState, decode_full_photo};
use crate::platform;
use crate::thumbs::ThumbCache;
use crate::ui::theme;

/// Opções do filtro de formato (dropdown da toolbar).
const FORMAT_FILTERS: &[&str] = &["Todas", "JPG", "PNG", "WebP", "TIFF", "BMP", "GIF"];
/// Proporções de crop: rótulo + (w, h). None = livre.
const ASPECT_OPTIONS: &[(&str, Option<(u32, u32)>)] = &[
    ("Livre", None),
    ("1:1", Some((1, 1))),
    ("4:3", Some((4, 3))),
    ("3:2", Some((3, 2))),
    ("16:9", Some((16, 9))),
    ("9:16", Some((9, 16))),
];

/// Zoom mínimo/máximo (multiplicador do fit).
const ZOOM_MIN: f32 = 0.1;
const ZOOM_MAX: f32 = 20.0;
/// Recorte mínimo em px (display) para aceitar o gesto.
const CROP_MIN_PX: f32 = 8.0;
/// Raio de clique dos gizmos de crop (px de tela).
const HANDLE_GRAB: f32 = 11.0;

/// Resultado do thread de salvamento.
struct SaveMsg {
    note: String,
    /// Arquivo sobrescrito: recarregar do disco e limpar o editor.
    reload: Option<PathBuf>,
}

/// Resultado assíncrono de cópia de imagem para o clipboard.
struct CopyMsg {
    note: String,
}

/// Alças de redimensionamento do crop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Handle {
    Nw,
    N,
    Ne,
    E,
    Se,
    S,
    Sw,
    W,
}

impl Handle {
    fn all() -> [Handle; 8] {
        use Handle::{E, N, Ne, Nw, S, Se, Sw, W};
        [Nw, N, Ne, E, Se, S, Sw, W]
    }

    /// Posição da alça no rect.
    fn point(self, r: &egui::Rect) -> egui::Pos2 {
        use Handle::{E, N, Ne, Nw, S, Se, Sw, W};
        let c = r.center();
        match self {
            Nw => r.min,
            N => egui::Pos2::new(c.x, r.min.y),
            Ne => egui::Pos2::new(r.max.x, r.min.y),
            E => egui::Pos2::new(r.max.x, c.y),
            Se => r.max,
            S => egui::Pos2::new(c.x, r.max.y),
            Sw => egui::Pos2::new(r.min.x, r.max.y),
            W => egui::Pos2::new(r.min.x, c.y),
        }
    }

    /// Ponto âncora oposto (fixo durante o resize).
    fn anchor(self, r: &egui::Rect) -> egui::Pos2 {
        use Handle::{E, N, Ne, Nw, S, Se, Sw, W};
        let c = r.center();
        match self {
            Nw => r.max,
            N => egui::Pos2::new(c.x, r.max.y),
            Ne => egui::Pos2::new(r.min.x, r.max.y),
            E => egui::Pos2::new(r.min.x, c.y),
            Se => r.min,
            S => egui::Pos2::new(c.x, r.min.y),
            Sw => egui::Pos2::new(r.max.x, r.min.y),
            W => egui::Pos2::new(r.max.x, c.y),
        }
    }
}

/// Gesto de crop em andamento.
enum CropDrag {
    /// Nova seleção a partir da âncora.
    New(egui::Pos2),
    /// Movendo (offset clique − rect.min).
    Move(egui::Vec2),
    /// Redimensionando pela alça (âncora oposta fixa; alça já consumida no hit).
    Resize(egui::Pos2),
}

/// Nó da árvore de pastas (filhos carregados sob demanda).
#[derive(Debug)]
struct DirNode {
    path: PathBuf,
    children: Option<Vec<DirNode>>,
    expanded: bool,
}

impl DirNode {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            children: None,
            expanded: false,
        }
    }

    fn name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }
}

/// Ação da árvore coletada durante o render (aplica depois, sem borrow duplo).
enum TreeAction {
    Toggle(usize),
    Open(PathBuf),
}

/// Ação do menu de contexto da imagem.
enum ImgAction {
    CopyPath,
    CopyImage,
    OpenDefault,
    Reveal,
    Rename,
}

/// Foto passa no filtro? Grupos: JPG=jpg+jpeg, TIFF=tiff+tif.
fn matches_filter(photo: &PhotoPath, filter: &str) -> bool {
    if filter == "Todas" {
        return true;
    }
    let ext = photo
        .path()
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match filter {
        "JPG" => ext == "jpg" || ext == "jpeg",
        "TIFF" => ext == "tiff" || ext == "tif",
        other => ext == other.to_ascii_lowercase(),
    }
}

/// Rect de (âncora, ponteiro) respeitando a proporção w/h (None = livre).
fn enforce_aspect(anchor: egui::Pos2, pointer: egui::Pos2, ratio: Option<f32>) -> egui::Rect {
    let Some(ratio) = ratio else {
        return egui::Rect::from_two_pos(anchor, pointer);
    };
    let d = pointer - anchor;
    if d.x == 0.0 || d.y == 0.0 {
        return egui::Rect::from_two_pos(anchor, pointer);
    }
    let (w, h) = (d.x.abs(), d.y.abs());
    let (w, h) = if w / h > ratio {
        (h * ratio, h)
    } else {
        (w, w / ratio)
    };
    egui::Rect::from_two_pos(
        anchor,
        anchor + egui::Vec2::new(d.x.signum() * w, d.y.signum() * h),
    )
}

/// Abas do dock (painéis redimensionáveis arrastando bordas e abas).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DockTab {
    Browser,
    Viewer,
    Filmstrip,
}

/// Ação do menu Arquivo (coletada no menu, aplicada depois do borrow).
enum FileAction {
    OpenFolder,
    OpenFiles,
    Save,
    SaveAs,
    Rename,
}

/// Estado da aplicação.
pub struct PhotoShowApp {
    cfg: AppConfig,
    tree_root: Option<DirNode>,
    current_dir: Option<PathBuf>,
    photos: Vec<PhotoPath>,
    visible: Vec<PhotoPath>,
    sel: Option<usize>,
    current: Option<PhotoPath>,
    store: ImageStore,
    thumbs: ThumbCache,
    editor: EditorStack,
    crop_mode: bool,
    crop_drag: Option<CropDrag>,
    crop_rect: Option<egui::Rect>,
    crop_aspect_name: String,
    applied_aspect: String,
    /// (draw rect, preview dims px) do último frame — para mapear o crop.
    last_draw: Option<(egui::Rect, (u32, u32))>,
    zoom: f32,
    offset: egui::Vec2,
    fullscreen: bool,
    format_filter: String,
    applied_filter: String,
    status: String,
    save_tx: Sender<SaveMsg>,
    save_rx: Receiver<SaveMsg>,
    saving: bool,
    copy_tx: Sender<CopyMsg>,
    copy_rx: Receiver<CopyMsg>,
    copying: bool,
    rename_open: bool,
    rename_buf: String,
    settings_open: bool,
    dock: egui_dock::DockState<DockTab>,
    /// Viewer maximizado? + layout anterior para restaurar.
    maximized: bool,
    saved_dock: Option<egui_dock::DockState<DockTab>>,
    /// Varredura em andamento (fora da thread da UI).
    scan_rx: Option<Receiver<ScanResult>>,
    scan_seq: u64,
    scanning: Option<PathBuf>,
    /// Foto a preservar ao aplicar resultado (rescan com mesmas fotos).
    preserve_on_scan: Option<PathBuf>,
    /// Intervalo [start, end) realmente visível na galeria neste frame.
    thumb_viewport: Option<(usize, usize)>,
}

impl PhotoShowApp {
    /// Cria com contexto de inicialização do eframe.
    #[must_use]
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Galeria nativa: 1 passe basta (taffy removido).
        cc.egui_ctx.options_mut(|o| {
            o.max_passes = std::num::NonZeroUsize::new(1).expect("1 > 0");
        });
        // Ícones Phosphor como fallback da fonte proporcional.
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        cc.egui_ctx.set_fonts(fonts);
        let (save_tx, save_rx) = mpsc::channel();
        let (copy_tx, copy_rx) = mpsc::channel();
        let mut app = Self {
            cfg: AppConfig::load(),
            tree_root: None,
            current_dir: None,
            photos: Vec::new(),
            visible: Vec::new(),
            sel: None,
            current: None,
            store: ImageStore::new(),
            thumbs: ThumbCache::new(),
            editor: EditorStack::new(),
            crop_mode: false,
            crop_drag: None,
            crop_rect: None,
            crop_aspect_name: String::from("Livre"),
            applied_aspect: String::from("Livre"),
            last_draw: None,
            zoom: 1.0,
            offset: egui::Vec2::ZERO,
            fullscreen: false,
            format_filter: String::from("Todas"),
            applied_filter: String::from("Todas"),
            status: String::from("Abra uma pasta ou fixe uma favorita para começar."),
            save_tx,
            save_rx,
            saving: false,
            copy_tx,
            copy_rx,
            copying: false,
            rename_open: false,
            rename_buf: String::new(),
            settings_open: false,
            dock: Self::default_dock(),
            maximized: false,
            saved_dock: None,
            scan_rx: None,
            scan_seq: 0,
            scanning: None,
            preserve_on_scan: None,
            thumb_viewport: None,
        };
        theme::apply(&app.cfg.theme, &cc.egui_ctx);
        // Reabre a última pasta para navegação imediata.
        if app.cfg.open_last_on_startup
            && let Some(dir) = app.cfg.last_folder.clone()
            && dir.is_dir()
        {
            app.open_dir_path(&cc.egui_ctx, dir);
        }
        app
    }

    /// Layout inicial do dock: Navegador | Visualizador / Miniaturas.
    /// NOTA: `fraction` é a fatia do filho NOVO (esquerda/baixo), não do antigo
    /// como o docstring sugere — verificado no fonte do egui_dock.
    fn default_dock() -> egui_dock::DockState<DockTab> {
        let mut dock = egui_dock::DockState::new(vec![DockTab::Viewer]);
        let surface = dock.main_surface_mut();
        // Navegador à esquerda com ~24%.
        let [viewer_node, _browser_node] =
            surface.split_left(egui_dock::NodeIndex::root(), 0.24, vec![DockTab::Browser]);
        // Miniaturas abaixo do visualizador (~20%).
        surface.split_below(viewer_node, 0.80, vec![DockTab::Filmstrip]);
        dock
    }

    /// Persiste config; erro vira status (nunca quebra o app).
    fn persist(&mut self) {
        if let Err(e) = self.cfg.save() {
            self.status = format!("Falha ao salvar config: {e}");
        }
    }

    // --- Pastas / árvore / favoritas ---

    /// Opções de varredura a partir das preferências.
    fn scan_opts(&self) -> ScanOptions {
        ScanOptions {
            respect_gitignore: self.cfg.respect_gitignore,
            skip_hidden: self.cfg.skip_hidden,
        }
    }

    /// Subpastas para a árvore, respeitando "exibir ocultas".
    fn child_dirs(&self, dir: &Path) -> Vec<DirNode> {
        fs_browser::list_subdirs(dir)
            .into_iter()
            .filter(|p| self.cfg.show_hidden_folders || !fs_browser::is_hidden(p))
            .map(DirNode::new)
            .collect()
    }

    /// Limpa o cache de filhos da árvore (rebuild lazy na próxima frame).
    /// Mantém os flags de expandido; só descarta as listas já lidas.
    fn clear_tree_cache(node: &mut DirNode) {
        if let Some(kids) = node.children.as_mut() {
            for kid in kids.iter_mut() {
                Self::clear_tree_cache(kid);
            }
        }
        node.children = None;
    }

    /// Dispara varredura assíncrona (não trava a UI); resultado aplica depois.
    /// `preserve`: foto a manter selecionada se continuar presente.
    fn start_scan(&mut self, ctx: &egui::Context, dir: PathBuf, preserve: Option<PathBuf>) {
        self.scan_seq += 1;
        let opts = self.scan_opts();
        let seq = self.scan_seq;
        self.scan_rx = Some(fs_browser::scan_dir_async(dir.clone(), opts, seq));
        self.scanning = Some(dir.clone());
        self.preserve_on_scan = preserve;
        self.status = format!("Varrendo {}…", dir.display());
        ctx.request_repaint();
    }

    /// Consome resultado de varredura pronto (mesma geração).
    fn poll_scans(&mut self, ctx: &egui::Context) {
        let res = match self.scan_rx.as_mut().and_then(|rx| rx.try_recv().ok()) {
            Some(r) => r,
            None => return,
        };
        if res.id != self.scan_seq {
            return; // obsoleto: usuário abriu outra pasta no meio
        }
        self.scan_rx = None;
        self.scanning = None;
        let dir_label = res.dir.display().to_string();
        if res.errors_seen > 0 {
            for error in &res.sample_errors {
                eprintln!("photoshow scan warning: {error}");
            }
        }
        let error_suffix = if res.errors_seen == 0 {
            String::new()
        } else {
            format!(" · {} erro(s) de leitura", res.errors_seen)
        };
        if res.photos.is_empty() {
            self.status = format!(
                "Nenhuma imagem em {} ({} arquivos verificados{})",
                dir_label, res.files_seen, error_suffix
            );
            self.replace_photos(ctx, Vec::new());
            return;
        }
        self.status = format!(
            "{} fotos em {} ({} arquivos verificados{})",
            res.photos.len(),
            dir_label,
            res.files_seen,
            error_suffix
        );
        // Preserva a seleção no rescan (troca de opção de varredura).
        let preserve = self.preserve_on_scan.take().and_then(|p| {
            res.photos
                .iter()
                .position(|ph| ph.path() == p)
                .map(|i| (i, p))
        });
        self.replace_photos(ctx, res.photos);
        if let Some((i, _)) = preserve
            && let Some(photo) = self.visible.get(i).cloned()
        {
            self.select_photo(ctx, i, photo);
        }
    }

    /// Define a pasta raiz (diálogo, favorita ou startup) e varre em background.
    fn open_dir_path(&mut self, ctx: &egui::Context, dir: PathBuf) {
        let mut root = DirNode::new(dir.clone());
        root.expanded = true;
        root.children = Some(self.child_dirs(&dir));
        self.tree_root = Some(root);
        self.current_dir = Some(dir.clone());
        self.cfg.last_folder = Some(dir.clone());
        self.persist();
        self.start_scan(ctx, dir, None);
    }

    fn open_folder_dialog(&mut self, ctx: &egui::Context) {
        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
            self.open_dir_path(ctx, dir);
        }
    }

    /// Carrega as fotos (em background) de uma pasta já escolhida.
    fn load_folder_contents(&mut self, ctx: &egui::Context, dir: &Path) {
        self.start_scan(ctx, dir.to_path_buf(), None);
    }

    fn open_files(&mut self, ctx: &egui::Context) {
        let files = rfd::FileDialog::new()
            .add_filter(
                "Imagens",
                &["jpg", "jpeg", "png", "webp", "tiff", "tif", "bmp", "gif"],
            )
            .pick_files()
            .unwrap_or_default();
        let photos = fs_browser::filter_loose_files(files);
        if photos.is_empty() {
            self.status = String::from("Nenhuma imagem válida selecionada.");
            return;
        }
        self.status = format!("{} arquivos soltos", photos.len());
        self.tree_root = None;
        self.current_dir = None;
        self.replace_photos(ctx, photos);
    }

    fn replace_photos(&mut self, ctx: &egui::Context, photos: Vec<PhotoPath>) {
        self.photos = photos;
        self.store.clear_prefetch();
        self.thumbs.clear();
        self.editor.clear();
        self.exit_crop_mode();
        self.applied_filter = String::from("\0"); // força refresh
        let _ = ctx;
        self.apply_filter_if_changed(ctx);
    }

    /// Recomputa `visible` se o dropdown mudou; preserva a foto atual.
    fn apply_filter_if_changed(&mut self, ctx: &egui::Context) {
        if self.format_filter == self.applied_filter {
            return;
        }
        self.applied_filter.clone_from(&self.format_filter);
        self.visible = self
            .photos
            .iter()
            .filter(|p| matches_filter(p, &self.applied_filter))
            .cloned()
            .collect();
        // Preserva a seleção se a foto continua visível.
        if let Some(cur) = &self.current
            && let Some(i) = self.visible.iter().position(|p| p == cur)
        {
            self.sel = Some(i);
            return;
        }
        self.sel = None;
        self.current = None;
        if let Some(first) = self.visible.first().cloned() {
            self.select_photo(ctx, 0, first);
        }
    }

    fn select_photo(&mut self, ctx: &egui::Context, index: usize, photo: PhotoPath) {
        self.sel = Some(index);
        self.current = Some(photo.clone());
        self.store.select(ctx, &photo);
        self.editor.clear();
        self.exit_crop_mode();
        self.zoom = 1.0;
        self.offset = egui::Vec2::ZERO;
    }

    fn exit_crop_mode(&mut self) {
        self.crop_mode = false;
        self.crop_drag = None;
        self.crop_rect = None;
    }

    fn step(&mut self, ctx: &egui::Context, delta: isize) {
        if self.visible.is_empty() {
            return;
        }
        let cur = self.sel.unwrap_or(0) as isize;
        let next = (cur + delta).clamp(0, self.visible.len() as isize - 1) as usize;
        if Some(next) != self.sel
            && let Some(photo) = self.visible.get(next).cloned()
        {
            self.select_photo(ctx, next, photo);
        }
    }

    fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        self.fullscreen = !self.fullscreen;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
    }

    /// Maximiza só o Visualizador (toggle); o resto do layout é preservado.
    fn toggle_maximize(&mut self) {
        if self.maximized {
            if let Some(saved) = self.saved_dock.take() {
                self.dock = saved;
            }
            self.maximized = false;
            self.status = String::from("Painéis restaurados.");
        } else {
            self.saved_dock = Some(self.dock.clone());
            self.dock = egui_dock::DockState::new(vec![DockTab::Viewer]);
            self.maximized = true;
            self.status =
                String::from("Visualizador maximizado — F9 ou o botão flutuante restaura.");
        }
    }

    /// Tamanho do thumbnail da galeria (persiste).
    fn set_thumb_size(&mut self, size: f32) {
        let clamped = size.clamp(48.0, 192.0);
        if clamped != self.cfg.thumb_size {
            self.cfg.thumb_size = clamped;
            self.persist();
        }
    }

    /// Proporção atual do crop (None = livre).
    fn crop_ratio(&self) -> Option<f32> {
        ASPECT_OPTIONS
            .iter()
            .find(|(name, _)| *name == self.crop_aspect_name)
            .and_then(|(_, r)| *r)
            .map(|(w, h)| w as f32 / h as f32)
    }

    /// Reaplica a proporção ao rect existente (troca no dropdown).
    fn refit_crop_to_aspect(&mut self) {
        let (Some(r), Some((draw, _))) = (self.crop_rect, self.last_draw) else {
            return;
        };
        let Some(ratio) = self.crop_ratio() else {
            return;
        };
        let c = r.center();
        let (mut w, mut h) = (r.width(), r.height());
        if w / h > ratio {
            w = h * ratio;
        } else {
            h = w / ratio;
        }
        let size = egui::Vec2::new(w.max(4.0), h.max(4.0));
        self.crop_rect = Some(egui::Rect::from_center_size(c, size).intersect(draw));
    }

    // --- Edição ---

    fn rotate(&mut self, ctx: &egui::Context, cw: bool) {
        let Some(base) = self.store.display_base_dims() else {
            return;
        };
        if cw {
            self.editor.rotate_cw(base);
        } else {
            self.editor.rotate_ccw(base);
        }
        self.store.rebuild_preview(ctx, &self.editor.state());
        // O rect de tela perde a referência: limpa o gesto, mantém o modo.
        self.crop_drag = None;
        self.crop_rect = None;
    }

    fn undo_edit(&mut self, ctx: &egui::Context) {
        if !self.editor.undo() {
            return;
        }
        self.refresh_preview(ctx);
    }

    fn redo_edit(&mut self, ctx: &egui::Context) {
        if !self.editor.redo() {
            return;
        }
        self.refresh_preview(ctx);
    }

    fn refresh_preview(&mut self, ctx: &egui::Context) {
        if self.editor.is_dirty() {
            self.store.rebuild_preview(ctx, &self.editor.state());
        } else {
            self.store.restore_base(ctx);
        }
    }

    fn reset_edits(&mut self, ctx: &egui::Context) {
        self.editor.clear();
        self.exit_crop_mode();
        self.store.restore_base(ctx);
    }

    /// Converte o rect de crop (coords de tela) para px de preview; None se inválido.
    fn crop_to_preview_px(&self) -> Option<CropRect> {
        let (draw, (pw, ph)) = self.last_draw?;
        let r = self.crop_rect?;
        let to_px = |p: egui::Pos2| {
            let rel = (p - draw.center()) / draw.size();
            egui::Vec2::new(
                (rel.x * pw as f32 + pw as f32 / 2.0).clamp(0.0, pw as f32),
                (rel.y * ph as f32 + ph as f32 / 2.0).clamp(0.0, ph as f32),
            )
        };
        let a = to_px(r.min);
        let b = to_px(r.max);
        let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
        let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
        if x1 - x0 < CROP_MIN_PX || y1 - y0 < CROP_MIN_PX {
            return None;
        }
        Some(CropRect {
            x: x0 as u32,
            y: y0 as u32,
            w: (x1 - x0) as u32,
            h: (y1 - y0) as u32,
        })
    }

    fn apply_crop(&mut self, ctx: &egui::Context) {
        match self.crop_to_preview_px() {
            Some(c) => {
                self.editor.set_crop(Some(c));
                self.store.rebuild_preview(ctx, &self.editor.state());
                self.exit_crop_mode();
                self.status = String::from("Crop aplicado no preview — Salvar para gravar.");
            }
            None => self.status = String::from("Seleção de crop muito pequena."),
        }
    }

    // --- Renomear ---

    fn open_rename(&mut self) {
        if let Some(cur) = &self.current {
            self.rename_buf = cur.display_name();
            self.rename_open = true;
        }
    }

    fn apply_rename(&mut self) {
        let Some(cur) = self.current.clone() else {
            self.rename_open = false;
            return;
        };
        let old_path = cur.path().to_path_buf();
        let new_name = self.rename_buf.trim().to_owned();
        let dest = old_path
            .parent()
            .map(|p| p.join(&new_name))
            .unwrap_or_else(|| PathBuf::from(&new_name));
        // Valida a extensão antes de tocar no disco.
        if PhotoPath::new(dest.clone()).is_none() {
            self.status = String::from("Use um nome com extensão de imagem (.jpg, .png, …).");
            self.rename_open = false;
            return;
        }
        match fs_browser::rename_photo(&old_path, &new_name) {
            Ok(dest) => {
                let new_photo = PhotoPath::new(dest.clone()).expect("extensão validada");
                for list in [&mut self.photos, &mut self.visible] {
                    for p in list.iter_mut() {
                        if p.path() == old_path {
                            *p = new_photo.clone();
                        }
                    }
                }
                self.current = Some(new_photo);
                self.thumbs.invalidate(&old_path);
                self.status = format!("Renomeado para {}", dest.display());
            }
            Err(e) => self.status = e,
        }
        self.rename_open = false;
    }

    // --- Salvamento (thread) ---

    fn start_save(&mut self, ctx: &egui::Context, dest: PathBuf, overwrite: bool) {
        let (Some(source), Some(base)) = (
            self.current
                .as_ref()
                .map(|photo| photo.path().to_path_buf()),
            self.store.display_base_dims(),
        ) else {
            self.status = String::from("Nada para salvar.");
            return;
        };
        let state = self.editor.state();
        let quality = self.cfg.jpeg_quality;
        let tx = self.save_tx.clone();
        let repaint = ctx.clone();
        self.saving = true;
        self.status = String::from("Salvando…");
        std::thread::spawn(move || {
            let msg = match decode_full_photo(&source) {
                Ok(full) => {
                    let baked = bake(&full, base, &state);
                    match save_baked_atomic(&baked, &source, &dest, quality) {
                        Ok(report) => SaveMsg {
                            note: format!(
                                "Salvo em {}{}",
                                dest.display(),
                                report.status_suffix()
                            ),
                            reload: overwrite.then_some(dest),
                        },
                        Err(error) => SaveMsg {
                            note: format!("Falha ao salvar: {error}"),
                            reload: None,
                        },
                    }
                }
                Err(error) => SaveMsg {
                    note: format!("Falha ao decodificar original para salvar: {error}"),
                    reload: None,
                },
            };
            let _ = tx.send(msg);
            repaint.request_repaint();
        });
    }

    fn save_overwrite(&mut self, ctx: &egui::Context) {
        let Some(cur) = self.current.clone() else {
            return;
        };
        if self.cfg.confirm_overwrite {
            let confirmed = rfd::MessageDialog::new()
                .set_title("Sobrescrever original?")
                .set_description(format!(
                    "{} será substituído pela versão editada.",
                    cur.display_name()
                ))
                .set_buttons(rfd::MessageButtons::YesNo)
                .show();
            if confirmed != rfd::MessageDialogResult::Yes {
                return;
            }
        }
        self.start_save(ctx, cur.path().to_path_buf(), true);
    }

    fn save_as(&mut self, ctx: &egui::Context) {
        let name = self
            .current
            .as_ref()
            .map(|c| c.display_name())
            .unwrap_or_else(|| String::from("foto.png"));
        let Some(dest) = rfd::FileDialog::new()
            .set_file_name(&name)
            .add_filter("JPEG", &["jpg", "jpeg"])
            .add_filter("PNG", &["png"])
            .add_filter("WebP", &["webp"])
            .add_filter("TIFF", &["tiff", "tif"])
            .add_filter("BMP", &["bmp"])
            .save_file()
        else {
            return;
        };
        self.start_save(ctx, dest, false);
    }

    fn poll_saves(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.save_rx.try_recv() {
            self.saving = false;
            self.status = msg.note;
            if let Some(path) = msg.reload {
                self.thumbs.invalidate(&path);
                self.editor.clear();
                self.exit_crop_mode();
                // Recarrega do disco (foto já editada gravada).
                if let Some(i) = self.sel
                    && let Some(photo) = self.visible.get(i).cloned()
                {
                    self.store.select(ctx, &photo);
                    self.zoom = 1.0;
                    self.offset = egui::Vec2::ZERO;
                }
            }
        }
    }

    fn poll_copies(&mut self) {
        while let Ok(msg) = self.copy_rx.try_recv() {
            self.copying = false;
            self.status = msg.note;
        }
    }
}

impl eframe::App for PhotoShowApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let _ = self.store.poll(ui.ctx());
        self.poll_saves(ui.ctx());
        self.poll_copies();
        self.poll_scans(ui.ctx());
        if let Some(s) = self.sel {
            let max_bytes = self.cfg.prefetch_max_mb * 1024 * 1024;
            self.store.ensure_prefetched(&self.visible, s, max_bytes);
        }
        self.thumb_viewport = None;

        // Aspect do crop trocado no dropdown: reaplica ao rect existente.
        if self.crop_aspect_name != self.applied_aspect {
            self.applied_aspect.clone_from(&self.crop_aspect_name);
            self.refit_crop_to_aspect();
        }

        // --- Atalhos globais (pausados com o modal de rename aberto) ---
        if !self.rename_open {
            if ui.ctx().input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                self.step(ui.ctx(), 1);
            }
            if ui.ctx().input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
                self.step(ui.ctx(), -1);
            }
            if ui.ctx().input(|i| i.key_pressed(egui::Key::F11)) {
                self.toggle_fullscreen(ui.ctx());
            }
            if ui.ctx().input(|i| i.key_pressed(egui::Key::F9)) {
                self.toggle_maximize();
            }
            if ui.ctx().input(|i| i.key_pressed(egui::Key::F2)) {
                self.open_rename();
            }
            let ctrl = ui.ctx().input(|i| i.modifiers.ctrl);
            let shift = ui.ctx().input(|i| i.modifiers.shift);
            if ctrl && ui.ctx().input(|i| i.key_pressed(egui::Key::Z)) && !shift {
                self.undo_edit(ui.ctx());
            }
            if ctrl && (ui.ctx().input(|i| i.key_pressed(egui::Key::Y)))
                || (ctrl && shift && ui.ctx().input(|i| i.key_pressed(egui::Key::Z)))
            {
                self.redo_edit(ui.ctx());
            }
            if self.crop_mode
                && self.crop_rect.is_some()
                && ui.ctx().input(|i| i.key_pressed(egui::Key::Enter))
            {
                self.apply_crop(ui.ctx());
            }
            if ui.ctx().input(|i| i.key_pressed(egui::Key::Minus)) {
                self.zoom = (self.zoom / 1.2).clamp(ZOOM_MIN, ZOOM_MAX);
            }
            if ui.ctx().input(|i| i.key_pressed(egui::Key::Equals)) {
                self.zoom = (self.zoom * 1.2).clamp(ZOOM_MIN, ZOOM_MAX);
            }
            if ui.ctx().input(|i| i.key_pressed(egui::Key::Num0)) {
                self.zoom = 1.0;
                self.offset = egui::Vec2::ZERO;
            }
        }
        if self.fullscreen && ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
            self.toggle_fullscreen(ui.ctx());
        } else if self.crop_mode && ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
            self.exit_crop_mode();
        }

        self.apply_filter_if_changed(ui.ctx());

        let has_photo = self.current.is_some();

        // --- Barra superior: menu Arquivo + edição centralizada ---
        if !self.fullscreen {
            egui::Panel::top("toolbar").show(ui, |ui| {
                // Barra única: Arquivo | edição | (direita) formato, fullscreen, config.
                egui::ScrollArea::horizontal()
                    .scroll_bar_visibility(
                        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                    )
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let can_save =
                                self.editor.is_dirty() && self.current.is_some() && !self.saving;
                            let can_rename = self.current.is_some();
                            let mut file_action: Option<FileAction> = None;
                            ui.menu_button("Arquivo", |ui| {
                                if ui
                                    .button(labeled(icons::P::FOLDER, "Abrir pasta…"))
                                    .clicked()
                                {
                                    file_action = Some(FileAction::OpenFolder);
                                    ui.close();
                                }
                                if ui
                                    .button(labeled(icons::P::FILE_IMAGE, "Abrir arquivos…"))
                                    .clicked()
                                {
                                    file_action = Some(FileAction::OpenFiles);
                                    ui.close();
                                }
                                ui.separator();
                                if ui
                                    .add_enabled(
                                        can_save,
                                        egui::Button::new(labeled(icons::P::FLOPPY_DISK, "Salvar")),
                                    )
                                    .on_hover_text("Sobrescreve o original")
                                    .clicked()
                                {
                                    file_action = Some(FileAction::Save);
                                    ui.close();
                                }
                                if ui
                                    .add_enabled(
                                        self.current.is_some() && !self.saving,
                                        egui::Button::new(labeled(
                                            icons::P::FLOPPY_DISK,
                                            "Salvar como…",
                                        )),
                                    )
                                    .clicked()
                                {
                                    file_action = Some(FileAction::SaveAs);
                                    ui.close();
                                }
                                if ui
                                    .add_enabled(
                                        can_rename,
                                        egui::Button::new(labeled(
                                            icons::P::PENCIL_LINE,
                                            "Renomear…",
                                        )),
                                    )
                                    .on_hover_text("F2")
                                    .clicked()
                                {
                                    file_action = Some(FileAction::Rename);
                                    ui.close();
                                }
                                ui.separator();
                                if ui
                                    .button(labeled(icons::P::ARROWS_OUT, "Restaurar layout"))
                                    .on_hover_text("Volta Navegador | Visualizador / Miniaturas")
                                    .clicked()
                                {
                                    self.dock = Self::default_dock();
                                    self.status = String::from("Layout padrão restaurado.");
                                    ui.close();
                                }
                            });
                            match file_action {
                                Some(FileAction::OpenFolder) => {
                                    self.open_folder_dialog(ui.ctx());
                                }
                                Some(FileAction::OpenFiles) => self.open_files(ui.ctx()),
                                Some(FileAction::Save) => self.save_overwrite(ui.ctx()),
                                Some(FileAction::SaveAs) => self.save_as(ui.ctx()),
                                Some(FileAction::Rename) => self.open_rename(),
                                None => {}
                            }
                            if has_photo {
                                ui.separator();
                                self.show_edit_bar(ui);
                            }
                            // Grupo à direita.
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .button(labeled(icons::P::GEAR, "Config"))
                                        .on_hover_text("Configurações")
                                        .clicked()
                                    {
                                        self.settings_open = true;
                                    }
                                    if ui
                                        .button(labeled(icons::P::ARROWS_OUT, "Fullscreen"))
                                        .clicked()
                                    {
                                        self.toggle_fullscreen(ui.ctx());
                                    }
                                    ui.label("Formato:");
                                    ui.add(
                                        egui_dropdown::DropDownBox::from_iter(
                                            FORMAT_FILTERS.iter(),
                                            "format_filter",
                                            &mut self.format_filter,
                                            |ui, text| ui.selectable_label(false, text),
                                        )
                                        .filter_by_input(false)
                                        .desired_width(90.0),
                                    );
                                },
                            );
                        });
                    });
            });

            egui::Panel::bottom("statusbar").show(ui, |ui| {
                ui.horizontal(|ui| {
                    let pos = match self.sel {
                        Some(i) => format!("{}/{}", i + 1, self.visible.len()),
                        None => String::from("0/0"),
                    };
                    let detail = match self.store.state() {
                        LoadState::Loaded { full_px, .. } => {
                            format!(
                                "{}×{} px · {}%",
                                full_px.0,
                                full_px.1,
                                (self.zoom * 100.0) as u32
                            )
                        }
                        LoadState::Loading => String::from("carregando…"),
                        LoadState::Failed(e) => e,
                        LoadState::Empty => String::new(),
                    };
                    let saving = if self.saving { " · salvando…" } else { "" };
                    let copying = if self.copying { " · copiando…" } else { "" };
                    ui.label(format!("{pos}  {}  {detail}{saving}{copying}", self.status));
                });
            });

            // --- Dock: Navegador | Visualizador / Miniaturas (tudo redimensionável) ---
            egui::CentralPanel::default().show(ui, |ui| {
                let mut dock = std::mem::replace(&mut self.dock, egui_dock::DockState::new(vec![]));
                egui_dock::DockArea::new(&mut dock).show_inside(ui, self);
                self.dock = dock;
            });
        } else {
            // --- Fullscreen: só o viewer ---
            egui::CentralPanel::default().show(ui, |ui| {
                self.viewer_tab_content(ui);
            });
        }

        self.show_rename_window(ui);
        self.show_settings_window(ui);

        self.thumbs
            .update(ui.ctx(), &self.visible, self.thumb_viewport, self.sel);
    }
}

impl egui_dock::TabViewer for PhotoShowApp {
    type Tab = DockTab;

    fn id(&mut self, tab: &mut DockTab) -> egui::Id {
        egui::Id::new(tab)
    }

    fn title(&mut self, tab: &mut DockTab) -> egui::WidgetText {
        match tab {
            DockTab::Browser => "Navegador",
            DockTab::Viewer => "Visualizador",
            DockTab::Filmstrip => "Miniaturas",
        }
        .into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut DockTab) {
        match tab {
            DockTab::Browser => self.show_browser(ui),
            DockTab::Viewer => self.viewer_tab_content(ui),
            DockTab::Filmstrip => self.show_filmstrip_content(ui),
        }
    }

    fn is_closeable(&self, _tab: &DockTab) -> bool {
        false
    }

    /// Desliga o scroll externo do dock: cada aba gerencia o próprio scroll,
    /// senão a roda vai para o contêiner externo e as listas internas travam.
    fn scroll_bars(&self, _tab: &DockTab) -> [bool; 2] {
        [false, false]
    }
}

impl PhotoShowApp {
    /// Cabeçalho de seção estilo Finder: versalete cinza, compacto.
    fn section_header(ui: &mut egui::Ui, text: &str) {
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(text)
                .small()
                .strong()
                .color(ui.visuals().weak_text_color()),
        );
        ui.add_space(2.0);
    }

    /// Barra de edição centralizada (segunda linha do topo; arquivo mora no menu).
    fn show_edit_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui
                .button(labeled(icons::P::ARROW_COUNTER_CLOCKWISE, "90°"))
                .on_hover_text("Rotacionar anti-horário")
                .clicked()
            {
                self.rotate(ui.ctx(), false);
            }
            if ui
                .button(labeled(icons::P::ARROW_CLOCKWISE, "90°"))
                .on_hover_text("Rotacionar horário")
                .clicked()
            {
                self.rotate(ui.ctx(), true);
            }
            ui.separator();
            let crop_label = if self.crop_mode {
                labeled(icons::P::CROP, "Crop… (ativo)")
            } else {
                labeled(icons::P::CROP, "Crop")
            };
            if ui.button(crop_label).clicked() {
                if self.crop_mode {
                    self.exit_crop_mode();
                } else {
                    self.crop_mode = true;
                    self.crop_drag = None;
                    self.crop_rect = None;
                    self.status = String::from(
                        "Arraste: novo · dentro: mover · alças: redimensionar · Enter aplica.",
                    );
                }
            }
            ui.label("Proporção:");
            ui.add(
                egui_dropdown::DropDownBox::from_iter(
                    ASPECT_OPTIONS.iter().map(|(name, _)| *name),
                    "crop_aspect",
                    &mut self.crop_aspect_name,
                    |ui, text| ui.selectable_label(false, text),
                )
                .filter_by_input(false)
                .desired_width(80.0),
            );
            if self.crop_mode
                && ui
                    .add_enabled(
                        self.crop_rect.is_some(),
                        egui::Button::new(labeled(icons::P::CHECK, "Aplicar")),
                    )
                    .on_hover_text("Enter · Esc cancela")
                    .clicked()
            {
                self.apply_crop(ui.ctx());
            }
            ui.separator();
            if ui
                .add_enabled(
                    self.editor.can_undo(),
                    egui::Button::new(labeled(icons::P::ARROW_U_UP_LEFT, "Desfazer")),
                )
                .on_hover_text("Ctrl+Z")
                .clicked()
            {
                self.undo_edit(ui.ctx());
            }
            if ui
                .add_enabled(
                    self.editor.can_redo(),
                    egui::Button::new(labeled(icons::P::ARROW_U_UP_RIGHT, "Refazer")),
                )
                .on_hover_text("Ctrl+Y / Ctrl+Shift+Z")
                .clicked()
            {
                self.redo_edit(ui.ctx());
            }
            if ui
                .add_enabled(self.editor.is_dirty(), egui::Button::new("Reset"))
                .clicked()
            {
                self.reset_edits(ui.ctx());
            }
            if self.editor.is_dirty() {
                ui.colored_label(egui::Color32::YELLOW, "• editado");
            }
        });
    }

    /// Modal de renomear (F2, painel, menu de contexto).
    fn show_rename_window(&mut self, ui: &mut egui::Ui) {
        if !self.rename_open {
            return;
        }
        let mut confirm = false;
        let mut cancel = false;
        egui::Window::new("Renomear")
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                ui.label("Novo nome (mantenha a extensão):");
                let resp = ui.text_edit_singleline(&mut self.rename_buf);
                // Foco automático ao abrir.
                resp.request_focus();
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        confirm = true;
                    }
                    if ui.button("Cancelar").clicked() {
                        cancel = true;
                    }
                });
                if ui.ctx().input(|i| i.key_pressed(egui::Key::Enter)) {
                    confirm = true;
                }
                if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
            });
        if confirm {
            self.apply_rename();
        } else if cancel {
            self.rename_open = false;
        }
    }

    /// Janela de configurações (persistente).
    fn show_settings_window(&mut self, ui: &mut egui::Ui) {
        if !self.settings_open {
            return;
        }
        let mut changed = false;
        let scan_before = (self.cfg.respect_gitignore, self.cfg.skip_hidden);
        egui::Window::new("Configurações")
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                changed |= ui
                    .checkbox(&mut self.cfg.confirm_overwrite, "Confirmar ao sobrescrever")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.cfg.show_filmstrip, "Exibir faixa de thumbnails")
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.cfg.open_last_on_startup,
                        "Reabrir última pasta ao iniciar",
                    )
                    .changed();
                ui.separator();
                ui.label("Varredura (pastas grandes):");
                changed |= ui
                    .checkbox(&mut self.cfg.respect_gitignore, "Respeitar .gitignore")
                    .on_hover_text("Pula o que o projeto ignora; vale fora de repo git")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.cfg.skip_hidden, "Pular pastas/arquivos ocultos")
                    .on_hover_text("Vale para a varredura de fotos")
                    .changed();
                let hidden_before = self.cfg.show_hidden_folders;
                changed |= ui
                    .checkbox(
                        &mut self.cfg.show_hidden_folders,
                        "Exibir pastas ocultas na árvore",
                    )
                    .changed();
                if self.cfg.show_hidden_folders != hidden_before
                    && let Some(root) = self.tree_root.as_mut()
                {
                    Self::clear_tree_cache(root);
                }
                ui.separator();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.cfg.jpeg_quality, 50..=100)
                            .text("Qualidade JPEG"),
                    )
                    .changed();
                self.cfg.jpeg_quality = self.cfg.jpeg_quality.clamp(1, 100);
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.cfg.prefetch_max_mb, 0..=256)
                            .text("Prefetch RAM (MB, 0 = off)"),
                    )
                    .changed();
                self.cfg.prefetch_max_mb = self.cfg.prefetch_max_mb.min(1024);
                ui.horizontal(|ui| {
                    ui.label("Tema:");
                    let before = self.cfg.theme.clone();
                    ui.add(
                        egui_dropdown::DropDownBox::from_iter(
                            THEMES.iter(),
                            "theme",
                            &mut self.cfg.theme,
                            |ui, text| ui.selectable_label(false, text),
                        )
                        .filter_by_input(false)
                        .desired_width(100.0),
                    );
                    if self.cfg.theme != before {
                        if THEMES.contains(&self.cfg.theme.as_str()) {
                            theme::apply(&self.cfg.theme, ui.ctx());
                        } else {
                            self.cfg.theme = before;
                        }
                        changed = true;
                    }
                });
                ui.weak(format!("{} pasta(s) fixada(s)", self.cfg.favorites.len()));
                if ui.button("Fechar").clicked() {
                    self.settings_open = false;
                }
            });
        if changed {
            self.persist();
            // Opção de varredura mudou com pasta aberta: revarre preservando a foto.
            let scan_after = (self.cfg.respect_gitignore, self.cfg.skip_hidden);
            if scan_after != scan_before
                && let Some(dir) = self.current_dir.clone()
            {
                let preserve = self.current.as_ref().map(|c| c.path().to_path_buf());
                self.start_scan(ui.ctx(), dir, preserve);
            }
        }
        if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) && !self.rename_open {
            self.settings_open = false;
        }
    }
}

/// Alça mais próxima do ponto (raio [`HANDLE_GRAB`]); retorna (alça, âncora).
fn hit_handle(r: &egui::Rect, p: egui::Pos2) -> Option<(Handle, egui::Pos2)> {
    Handle::all()
        .into_iter()
        .find(|h| h.point(r).distance(p) <= HANDLE_GRAB)
        .map(|h| (h, h.anchor(r)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn filter_groups_extensions() {
        let jpg = PhotoPath::new(PathBuf::from("a.JPG")).unwrap();
        let jpeg = PhotoPath::new(PathBuf::from("b.jpeg")).unwrap();
        let png = PhotoPath::new(PathBuf::from("c.png")).unwrap();
        assert!(matches_filter(&jpg, "Todas"));
        assert!(matches_filter(&jpg, "JPG"));
        assert!(matches_filter(&jpeg, "JPG"));
        assert!(!matches_filter(&png, "JPG"));
        assert!(matches_filter(&png, "PNG"));
    }

    #[test]
    fn aspect_enforcement_keeps_ratio_from_anchor() {
        let a = egui::Pos2::new(0.0, 0.0);
        let r = enforce_aspect(a, egui::Pos2::new(100.0, 10.0), Some(1.0));
        assert_eq!((r.width(), r.height()), (10.0, 10.0));
        let r = enforce_aspect(a, egui::Pos2::new(10.0, 100.0), Some(2.0));
        assert_eq!((r.width(), r.height()), (10.0, 5.0));
        // Negativo preserva o quadrante.
        let r = enforce_aspect(a, egui::Pos2::new(-40.0, -10.0), Some(1.0));
        assert_eq!((r.min.x, r.min.y), (-10.0, -10.0));
        // Livre não altera.
        let r = enforce_aspect(a, egui::Pos2::new(30.0, 7.0), None);
        assert_eq!((r.width(), r.height()), (30.0, 7.0));
    }

    #[test]
    fn handles_hit_corners_and_report_opposite_anchor() {
        let r = egui::Rect::from_min_size(egui::Pos2::new(10.0, 10.0), egui::Vec2::new(80.0, 60.0));
        let (h, anchor) = hit_handle(&r, egui::Pos2::new(10.0, 10.0)).expect("canto NW");
        assert_eq!(h, Handle::Nw);
        assert_eq!(anchor, egui::Pos2::new(90.0, 70.0));
        assert!(hit_handle(&r, egui::Pos2::new(50.0, 40.0)).is_none());
    }
}
