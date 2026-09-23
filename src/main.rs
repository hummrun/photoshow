//! photoshow: visualizador de fotos rápido com egui.
//!
//! M1: janela eframe 0.36 + abrir pasta/arquivos via rfd + lista lateral.
//! Fases seguintes: render da imagem (image_store), EXIF e thumbnails,
//! edição não-destrutiva rotate/crop (editor).

mod app;
mod atomic_file;
mod config;
mod editor;
mod fs_browser;
mod icons;
mod image_store;
mod media;
mod metadata;
mod platform;
mod thumbs;
mod ui;

use anyhow::Context;

fn main() -> anyhow::Result<()> {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "photoshow",
        native_options,
        Box::new(|cc| Ok(Box::new(app::PhotoShowApp::new(cc)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe saiu com erro: {e}"))
    .context("falha ao iniciar o photoshow")
}
