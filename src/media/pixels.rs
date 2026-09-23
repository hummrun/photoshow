//! UI-neutral decoded pixel buffers.

/// RGBA8 frame ready for upload by a UI adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaFrame {
    width: u32,
    height: u32,
    bytes: Vec<u8>,
}

impl RgbaFrame {
    #[must_use]
    pub fn from_dynamic(image: &image::DynamicImage) -> Self {
        let rgba = image.to_rgba8();
        Self {
            width: rgba.width(),
            height: rgba.height(),
            bytes: rgba.into_raw(),
        }
    }

    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_image_becomes_rgba8_frame() {
        let image = image::DynamicImage::ImageRgb8(image::RgbImage::new(3, 2));
        let frame = RgbaFrame::from_dynamic(&image);

        assert_eq!(frame.width(), 3);
        assert_eq!(frame.height(), 2);
        assert_eq!(frame.byte_len(), 3 * 2 * 4);
    }
}
