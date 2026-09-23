//! Desktop integration boundary.

use std::path::Path;

pub fn copy_text_to_clipboard(text: &str) -> Result<(), String> {
    arboard::Clipboard::new()
        .map_err(|error| format!("clipboard: {error}"))?
        .set_text(text.to_owned())
        .map_err(|error| format!("clipboard: {error}"))
}

pub fn copy_image_to_clipboard(image: &image::DynamicImage) -> Result<(), String> {
    let rgba = image.to_rgba8();
    let data = arboard::ImageData {
        width: rgba.width() as usize,
        height: rgba.height() as usize,
        bytes: rgba.into_raw().into(),
    };
    arboard::Clipboard::new()
        .map_err(|error| format!("clipboard: {error}"))?
        .set_image(data)
        .map_err(|error| format!("clipboard: {error}"))
}

pub fn open_default(path: &Path) -> Result<(), String> {
    open::that(path).map_err(|error| format!("Falha ao abrir: {error}"))
}

pub fn reveal_in_folder(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .status()
            .map_err(|error| error.to_string())?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        let ok = std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .status()
            .map_err(|error| error.to_string())?;
        ok.success()
            .then_some(())
            .ok_or_else(|| String::from("open -R falhou"))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let parent = path
            .parent()
            .ok_or_else(|| String::from("pasta inválida"))?;
        let ok = std::process::Command::new("xdg-open")
            .arg(parent)
            .status()
            .map_err(|error| error.to_string())?;
        ok.success()
            .then_some(())
            .ok_or_else(|| String::from("xdg-open falhou"))
    }
}
