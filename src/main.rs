#![forbid(unsafe_code)]

//! PhotoShow: visualizador de fotos nativo, rápido e intencionalmente pequeno.
//!
//! A aplicação mantém decode/media separado do adapter egui, limita trabalho
//! assíncrono e trata escrita em disco como operação transacional.

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
