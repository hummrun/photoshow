//! Embedded image metadata capture and preservation policy.
//!
//! PhotoShow applies EXIF orientation to decoded pixels. Re-embedding a non-identity
//! orientation tag would make compatible viewers rotate the already-oriented pixels
//! again, so raw EXIF is preserved only when orientation is identity. ICC profiles
//! are preserved whenever the destination encoder supports them.

use std::path::Path;

use image::{ImageDecoder, ImageEncoder};

#[derive(Debug, Default, Clone)]
pub struct EmbeddedMetadata {
    pub exif: Option<Vec<u8>>,
    pub icc: Option<Vec<u8>>,
    pub orientation: u16,
    pub read_error: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MetadataReport {
    pub exif_present: bool,
    pub icc_present: bool,
    pub exif_preserved: bool,
    pub icc_preserved: bool,
    pub exif_skipped_for_orientation: bool,
    pub read_error: Option<String>,
}

impl MetadataReport {
    #[must_use]
    pub fn status_suffix(&self) -> String {
        if let Some(error) = &self.read_error {
            return format!(" · metadata não verificada: {error}");
        }
        let mut notes = Vec::new();
        if self.exif_present && !self.exif_preserved {
            if self.exif_skipped_for_orientation {
                notes.push("EXIF omitido para evitar rotação duplicada");
            } else {
                notes.push("EXIF não preservado pelo formato");
            }
        }
        if self.icc_present && !self.icc_preserved {
            notes.push("perfil ICC não preservado pelo formato");
        }
        if notes.is_empty() {
            String::new()
        } else {
            format!(" · {}", notes.join("; "))
        }
    }
}

#[must_use]
pub fn read_embedded_metadata(path: &Path) -> EmbeddedMetadata {
    let orientation = crate::exif::read_orientation(path);
    let reader = match image::ImageReader::open(path) {
        Ok(reader) => reader,
        Err(error) => {
            return EmbeddedMetadata {
                orientation,
                read_error: Some(error.to_string()),
                ..EmbeddedMetadata::default()
            };
        }
    };
    let reader = match reader.with_guessed_format() {
        Ok(reader) => reader,
        Err(error) => {
            return EmbeddedMetadata {
                orientation,
                read_error: Some(error.to_string()),
                ..EmbeddedMetadata::default()
            };
        }
    };
    let mut decoder = match reader.into_decoder() {
        Ok(decoder) => decoder,
        Err(error) => {
            return EmbeddedMetadata {
                orientation,
                read_error: Some(error.to_string()),
                ..EmbeddedMetadata::default()
            };
        }
    };

    let exif = decoder.exif_metadata().ok().flatten();
    let icc = decoder.icc_profile().ok().flatten();
    EmbeddedMetadata {
        exif,
        icc,
        orientation,
        read_error: None,
    }
}

pub fn apply_to_encoder(
    encoder: &mut impl ImageEncoder,
    metadata: &EmbeddedMetadata,
) -> MetadataReport {
    let mut report = MetadataReport {
        exif_present: metadata.exif.is_some(),
        icc_present: metadata.icc.is_some(),
        read_error: metadata.read_error.clone(),
        ..MetadataReport::default()
    };

    if let Some(icc) = &metadata.icc
        && encoder.set_icc_profile(icc.clone()).is_ok()
    {
        report.icc_preserved = true;
    }

    if let Some(exif) = &metadata.exif {
        if metadata.orientation <= 1 {
            if encoder.set_exif_metadata(exif.clone()).is_ok() {
                report.exif_preserved = true;
            }
        } else {
            report.exif_skipped_for_orientation = true;
        }
    }

    report
}

#[must_use]
pub fn unsupported_report(metadata: &EmbeddedMetadata) -> MetadataReport {
    MetadataReport {
        exif_present: metadata.exif.is_some(),
        icc_present: metadata.icc.is_some(),
        read_error: metadata.read_error.clone(),
        exif_skipped_for_orientation: metadata.exif.is_some() && metadata.orientation > 1,
        ..MetadataReport::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_explains_orientation_skip() {
        let report = MetadataReport {
            exif_present: true,
            exif_skipped_for_orientation: true,
            ..MetadataReport::default()
        };
        assert!(report.status_suffix().contains("rotação duplicada"));
    }
}
