//! Edição não-destrutiva: estado normalizado + histórico para undo.
//!
//! Modelo: `rot` (quartos de volta CW acumulados, 0..=3) + um único `crop`
//! definido no espaço de pixels da imagem *já rotacionada* (display).
//! Rotacionar com crop ativo transforma o rect junto — o bake aplica
//! rotate e depois crop na full-res com o mesmo fator de escala.

/// Recorte em pixels da imagem rotacionada (display ou full escalado).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CropRect {
    /// Canto superior-esquerdo + tamanho.
    pub x: u32,
    /// Canto superior-esquerdo + tamanho.
    pub y: u32,
    /// Largura (>0).
    pub w: u32,
    /// Altura (>0).
    pub h: u32,
}

/// Estado normalizado do editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EditorState {
    /// Quartos de volta CW (0..=3).
    pub rot: u8,
    /// Recorte opcional no espaço rotacionado.
    pub crop: Option<CropRect>,
}

impl EditorState {
    /// Estado limpo (sem edições).
    #[must_use]
    pub fn clean() -> Self {
        Self::default()
    }

    /// Sem edições?
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.rot == 0 && self.crop.is_none()
    }

    /// Dimensões após aplicar `rot` a uma base `(w, h)`.
    #[must_use]
    pub fn dims(&self, base: (u32, u32)) -> (u32, u32) {
        if self.rot % 2 == 1 {
            (base.1, base.0)
        } else {
            base
        }
    }
}

/// Pilha de edição com undo/redo (snapshots baratos: 2 campos).
#[derive(Debug, Default)]
pub struct EditorStack {
    history: Vec<EditorState>,
    future: Vec<EditorState>,
}

impl EditorStack {
    /// Cria com estado limpo.
    #[must_use]
    pub fn new() -> Self {
        Self {
            history: vec![EditorState::clean()],
            future: Vec::new(),
        }
    }

    /// Estado atual.
    #[must_use]
    pub fn state(&self) -> EditorState {
        *self.history.last().expect("histórico nunca vazio")
    }

    /// Há edições pendentes?
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        !self.state().is_clean()
    }

    /// Há algo para refazer?
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    /// Há algo para desfazer?
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.history.len() > 1
    }

    fn commit(&mut self, state: EditorState) {
        // Normaliza rotações completas para não acumular histórico inútil.
        let mut state = state;
        state.rot %= 4;
        if state == self.state() {
            return;
        }
        self.history.push(state);
        self.future.clear();
    }

    /// Gira 90° CW; transforma o crop existente junto.
    /// `base` = dimensões da imagem de display pré-rotação.
    pub fn rotate_cw(&mut self, base: (u32, u32)) {
        let cur = self.state();
        let dims = cur.dims(base);
        let crop = cur.crop.map(|r| rot_rect_cw(dims.0, dims.1, r));
        self.commit(EditorState {
            rot: cur.rot + 1,
            crop,
        });
    }

    /// Gira 90° CCW; transforma o crop existente junto.
    /// `base` = dimensões da imagem de display pré-rotação.
    pub fn rotate_ccw(&mut self, base: (u32, u32)) {
        let cur = self.state();
        let dims = cur.dims(base);
        let crop = cur.crop.map(|r| rot_rect_ccw(dims.0, dims.1, r));
        self.commit(EditorState {
            rot: cur.rot + 3,
            crop,
        });
    }

    /// Define/substitui o recorte (espaço rotacionado de display).
    pub fn set_crop(&mut self, crop: Option<CropRect>) {
        let cur = self.state();
        self.commit(EditorState { crop, ..cur });
    }

    /// Desfaz a última operação; `false` se já está limpo.
    pub fn undo(&mut self) -> bool {
        if self.history.len() > 1 {
            let cur = self.history.pop().expect("len > 1");
            self.future.push(cur);
            true
        } else {
            false
        }
    }

    /// Refaz a última operação desfeita; `false` se não há nada.
    pub fn redo(&mut self) -> bool {
        if let Some(st) = self.future.pop() {
            self.history.push(st);
            true
        } else {
            false
        }
    }

    /// Descarta tudo (Reset).
    pub fn clear(&mut self) {
        self.history.truncate(1);
        self.future.clear();
    }
}

/// Rotaciona um rect 90° CW dentro de uma imagem de altura `h`.
fn rot_rect_cw(_w: u32, h: u32, r: CropRect) -> CropRect {
    CropRect {
        x: h.saturating_sub(r.y + r.h),
        y: r.x,
        w: r.h,
        h: r.w,
    }
}

/// Rotaciona um rect 90° CCW dentro de uma imagem de largura `w`.
fn rot_rect_ccw(w: u32, _h: u32, r: CropRect) -> CropRect {
    CropRect {
        x: r.y,
        y: w.saturating_sub(r.x + r.w),
        w: r.h,
        h: r.w,
    }
}

/// Aplica estado a uma imagem (preview em display ou bake parcial).
#[must_use]
pub fn apply_to_image(img: &image::DynamicImage, st: &EditorState) -> image::DynamicImage {
    let mut out = match st.rot % 4 {
        1 => img.rotate90(),
        2 => img.rotate180(),
        3 => img.rotate270(),
        _ => img.clone(),
    };
    if let Some(c) = st.crop {
        let (w, h) = (out.width(), out.height());
        let x = c.x.min(w.saturating_sub(1));
        let y = c.y.min(h.saturating_sub(1));
        let cw = c.w.min(w.saturating_sub(x)).max(1);
        let ch = c.h.min(h.saturating_sub(y)).max(1);
        out = image::DynamicImage::ImageRgba8(
            image::imageops::crop_imm(&out, x, y, cw, ch).to_image(),
        );
    }
    out
}

/// Bake final: aplica `rot` na full-res e o crop escalado do display.
/// `display_base` = dimensões da imagem de display pré-rotação.
#[must_use]
pub fn bake(
    full: &image::DynamicImage,
    display_base: (u32, u32),
    st: &EditorState,
) -> image::DynamicImage {
    let rotated = apply_to_image(full, &EditorState { crop: None, ..*st });
    let Some(c) = st.crop else {
        return rotated;
    };
    // Display e full preservam aspecto: um fator serve para os dois eixos.
    let disp_dims = st.dims(display_base);
    let rw = rotated.width() as f64 / disp_dims.0.max(1) as f64;
    let x = (c.x as f64 * rw).round() as u32;
    let y = (c.y as f64 * rw).round() as u32;
    let w = (c.w as f64 * rw).round() as u32;
    let h = (c.h as f64 * rw).round() as u32;
    let (fw, fh) = (rotated.width(), rotated.height());
    let x = x.min(fw.saturating_sub(1));
    let y = y.min(fh.saturating_sub(1));
    let w = w.min(fw.saturating_sub(x)).max(1);
    let h = h.min(fh.saturating_sub(y)).max(1);
    image::DynamicImage::ImageRgba8(image::imageops::crop_imm(&rotated, x, y, w, h).to_image())
}

/// Grava a imagem assada respeitando a qualidade JPEG configurada.
/// Outras extensões usam o encoder padrão do crate `image`.
pub fn save_baked(
    img: &image::DynamicImage,
    dest: &std::path::Path,
    jpeg_quality: u8,
) -> Result<(), String> {
    let is_jpeg = dest
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"))
        .unwrap_or(false);
    if is_jpeg {
        use std::io::Write as _;
        let file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
        let mut buf = std::io::BufWriter::new(file);
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(
            &mut buf,
            jpeg_quality.clamp(1, 100),
        );
        enc.encode_image(img).map_err(|e| e.to_string())?;
        buf.flush().map_err(|e| e.to_string())?;
        Ok(())
    } else {
        img.save(dest).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotate_moves_crop_with_it() {
        let mut ed = EditorStack::new();
        ed.set_crop(Some(CropRect {
            x: 10,
            y: 20,
            w: 30,
            h: 40,
        }));
        ed.rotate_cw((100, 80));
        // dims atuais (100,80) -> CW: x' = 80-20-40 = 20, y' = 10
        assert_eq!(
            ed.state().crop,
            Some(CropRect {
                x: 20,
                y: 10,
                w: 40,
                h: 30
            })
        );
        assert_eq!(ed.state().rot, 1);
        ed.rotate_ccw((100, 80));
        assert_eq!(ed.state().rot, 0);
        assert_eq!(
            ed.state().crop,
            Some(CropRect {
                x: 10,
                y: 20,
                w: 30,
                h: 40
            })
        );
    }

    #[test]
    fn full_turn_is_noop() {
        let mut ed = EditorStack::new();
        for _ in 0..4 {
            ed.rotate_cw((50, 50));
        }
        assert!(ed.state().is_clean());
        assert!(!ed.is_dirty());
    }

    #[test]
    fn undo_and_clear() {
        let mut ed = EditorStack::new();
        ed.rotate_cw((50, 50));
        assert!(ed.is_dirty());
        assert!(ed.can_undo());
        assert!(ed.undo());
        assert!(!ed.is_dirty());
        assert!(!ed.undo());
        ed.rotate_cw((50, 50));
        ed.clear();
        assert!(!ed.is_dirty());
    }

    #[test]
    fn redo_reapplies_undone_and_new_op_clears_it() {
        let mut ed = EditorStack::new();
        ed.rotate_cw((50, 50));
        assert!(ed.undo());
        assert!(ed.can_redo());
        assert!(ed.redo());
        assert_eq!(ed.state().rot, 1);
        assert!(!ed.redo());
        // Nova operação limpa o redo.
        assert!(ed.undo());
        ed.rotate_ccw((50, 50));
        assert!(!ed.can_redo());
        assert_eq!(ed.state().rot, 3);
    }

    #[test]
    fn bake_scales_crop_to_full_res() {
        use image::{DynamicImage, RgbImage};
        let full = DynamicImage::ImageRgb8(RgbImage::new(200, 160));
        let st = EditorState {
            rot: 1, // CW: full 200x160 -> 160x200
            crop: Some(CropRect {
                x: 10,
                y: 20,
                w: 40,
                h: 30,
            }),
        };
        // display base 100x80 -> rotacionado 80x100; fator = 160/80 = 2
        let out = bake(&full, (100, 80), &st);
        assert_eq!((out.width(), out.height()), (80, 60));
    }

    #[test]
    fn apply_crop_only() {
        use image::{DynamicImage, RgbImage};
        let img = DynamicImage::ImageRgb8(RgbImage::new(100, 80));
        let st = EditorState {
            rot: 0,
            crop: Some(CropRect {
                x: 5,
                y: 5,
                w: 20,
                h: 10,
            }),
        };
        let out = apply_to_image(&img, &st);
        assert_eq!((out.width(), out.height()), (20, 10));
    }

    #[test]
    fn baked_image_saves_in_all_mvp_formats() {
        use image::{DynamicImage, RgbImage};
        let dir = tempfile::tempdir().expect("tempdir");
        let full = DynamicImage::ImageRgb8(RgbImage::from_fn(64, 48, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        }));
        let st = EditorState {
            rot: 1,
            crop: Some(CropRect {
                x: 4,
                y: 4,
                w: 20,
                h: 12,
            }),
        };
        let out = bake(&full, (64, 48), &st);
        for ext in ["jpg", "png", "bmp", "gif", "tiff", "webp"] {
            let path = dir.path().join(format!("out.{ext}"));
            out.save(&path).unwrap_or_else(|_| panic!("save {ext}"));
            assert!(path.exists());
        }
    }
}
