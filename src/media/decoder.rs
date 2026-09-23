//! UI-neutral image decoding.
//!
//! This module knows image formats and EXIF orientation, but nothing about egui,
//! textures, widgets or application state.

use std::path::Path;

use thiserror::Error;

use image::ImageDecoder as _;

pub const DISPLAY_MAX_DIM: u32 = 2048;

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

#[derive(Debug)]
pub struct DecodedPhoto {
    pub display: image::DynamicImage,
    pub full_size: (u32, u32),
}

pub fn decode_full_photo(path: &Path) -> Result<image::DynamicImage, LoadError> {
    let reader =
        image::ImageReader::open(path).map_err(|error| LoadError::Io(error.to_string()))?;
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut image = image::DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    Ok(image)
}

pub fn decode_photo(path: &Path) -> Result<DecodedPhoto, LoadError> {
    let full = decode_full_photo(path)?;
    let full_size = (full.width(), full.height());
    let display = if full.width().max(full.height()) > DISPLAY_MAX_DIM {
        full.thumbnail(DISPLAY_MAX_DIM, DISPLAY_MAX_DIM)
    } else {
        full
    };
    Ok(DecodedPhoto { display, full_size })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn write_test_png(dir: &Path, name: &str, width: u32, height: u32) -> PathBuf {
        let image = image::RgbImage::from_fn(width, height, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        });
        let path = dir.join(name);
        image.save(&path).expect("save png");
        path
    }

    #[test]
    fn decode_small_image_keeps_display_dimensions() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_test_png(dir.path(), "s.png", 32, 24);
        let decoded = decode_photo(&path).expect("decode");

        assert_eq!(decoded.full_size, (32, 24));
        assert_eq!(
            (decoded.display.width(), decoded.display.height()),
            (32, 24)
        );
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
        let error = decode_photo(Path::new("/nao/existe/foto.png")).expect_err("deveria falhar");
        assert!(matches!(error, LoadError::Io(_)));
    }
}
