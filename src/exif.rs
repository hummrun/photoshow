//! Orientação EXIF: lê o tag Orientation e corrige a imagem.
//!
//! Sem isso, foto de celular aparece de lado. Mapeamento de correção
//! (para exibir corretamente a partir do valor armazenado):
//! 1=identidade, 2=flipH, 3=180°, 4=flipV, 5=rot90CW+flipH,
//! 6=rot90CW, 7=rot90CW+flipV, 8=rot270CW.

use std::path::Path;

/// Lê o Orientation (1..=8); qualquer falha retorna 1 (identidade).
#[must_use]
pub fn read_orientation(path: &Path) -> u16 {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return 1,
    };
    let mut reader = std::io::BufReader::new(file);
    let exif = match exif::Reader::new().read_from_container(&mut reader) {
        Ok(e) => e,
        Err(_) => return 1,
    };
    match exif
        .get_field(exif::Tag::Orientation, exif::In::PRIMARY)
        .and_then(|f| f.value.get_uint(0))
    {
        Some(v @ 1..=8) => v as u16,
        _ => 1,
    }
}

/// Aplica a correção de orientação, retornando a imagem exibível.
#[must_use]
pub fn apply_orientation(img: image::DynamicImage, orientation: u16) -> image::DynamicImage {
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate90().flipv(),
        8 => img.rotate270(),
        _ => img,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgb, RgbImage};

    /// Imagem 3x2 com R = rótulo 1..=6:
    /// Row0: 1 2 3 / Row1: 4 5 6
    fn labeled() -> DynamicImage {
        let img = RgbImage::from_fn(3, 2, |x, y| Rgb([(y * 3 + x + 1) as u8, 0, 0]));
        DynamicImage::ImageRgb8(img)
    }

    fn grid(img: &DynamicImage) -> (u32, u32, Vec<u8>) {
        let rgb = img.to_rgb8();
        let px = rgb.pixels().map(|p| p[0]).collect();
        (rgb.width(), rgb.height(), px)
    }

    #[test]
    fn orientations_map_correctly() {
        // (orientação, largura, altura, rótulos em ordem de varredura)
        let cases: &[(u16, u32, u32, &[u8])] = &[
            (1, 3, 2, &[1, 2, 3, 4, 5, 6]),
            (2, 3, 2, &[3, 2, 1, 6, 5, 4]),
            (3, 3, 2, &[6, 5, 4, 3, 2, 1]),
            (4, 3, 2, &[4, 5, 6, 1, 2, 3]),
            (5, 2, 3, &[1, 4, 2, 5, 3, 6]),
            (6, 2, 3, &[4, 1, 5, 2, 6, 3]),
            (7, 2, 3, &[6, 3, 5, 2, 4, 1]),
            (8, 2, 3, &[3, 6, 2, 5, 1, 4]),
            (0, 3, 2, &[1, 2, 3, 4, 5, 6]),
            (9, 3, 2, &[1, 2, 3, 4, 5, 6]),
        ];
        for (ori, w, h, labels) in cases {
            let (gw, gh, px) = grid(&apply_orientation(labeled(), *ori));
            assert_eq!((gw, gh), (*w, *h), "dims ori={ori}");
            assert_eq!(&px, labels, "pixels ori={ori}");
        }
    }

    #[test]
    fn missing_file_defaults_to_identity() {
        assert_eq!(read_orientation(Path::new("/nao/existe.jpg")), 1);
    }
}
