//! Embedded image metadata capture and preservation.
//!
//! PhotoShow applies EXIF orientation to pixels during decode. Before re-encoding,
//! the raw EXIF chunk has its Orientation field normalized to NoTransforms using
//! image's metadata helper. That preserves the rest of EXIF without causing a
//! second rotation in standards-compliant viewers.

use std::path::Path;

use image::metadata::Orientation;
use image::{ImageDecoder, ImageEncoder};

#[derive(Debug, Default, Clone)]
pub struct EmbeddedMetadata {
    pub exif: Option<Vec<u8>>,
    pub icc: Option<Vec<u8>>,
    pub exif_orientation_normalized: bool,
    pub read_error: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MetadataReport {
    pub exif_present: bool,
    pub icc_present: bool,
    pub exif_preserved: bool,
    pub icc_preserved: bool,
    pub exif_orientation_normalized: bool,
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
            notes.push("EXIF não preservado pelo formato");
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
    let reader = match image::ImageReader::open(path) {
        Ok(reader) => reader,
        Err(error) => {
            return EmbeddedMetadata {
                read_error: Some(error.to_string()),
                ..EmbeddedMetadata::default()
            };
        }
    };
    let reader = match reader.with_guessed_format() {
        Ok(reader) => reader,
        Err(error) => {
            return EmbeddedMetadata {
                read_error: Some(error.to_string()),
                ..EmbeddedMetadata::default()
            };
        }
    };
    let mut decoder = match reader.into_decoder() {
        Ok(decoder) => decoder,
        Err(error) => {
            return EmbeddedMetadata {
                read_error: Some(error.to_string()),
                ..EmbeddedMetadata::default()
            };
        }
    };

    let mut exif = decoder.exif_metadata().ok().flatten();
    let icc = decoder.icc_profile().ok().flatten();
    let exif_orientation_normalized = exif
        .as_mut()
        .and_then(|chunk| Orientation::remove_from_exif_chunk(chunk))
        .is_some();

    EmbeddedMetadata {
        exif,
        icc,
        exif_orientation_normalized,
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
        exif_orientation_normalized: metadata.exif_orientation_normalized,
        read_error: metadata.read_error.clone(),
        ..MetadataReport::default()
    };

    if let Some(icc) = &metadata.icc
        && encoder.set_icc_profile(icc.clone()).is_ok()
    {
        report.icc_preserved = true;
    }

    if let Some(exif) = &metadata.exif
        && encoder.set_exif_metadata(exif.clone()).is_ok()
    {
        report.exif_preserved = true;
    }

    report
}

#[must_use]
pub fn unsupported_report(metadata: &EmbeddedMetadata) -> MetadataReport {
    MetadataReport {
        exif_present: metadata.exif.is_some(),
        icc_present: metadata.icc.is_some(),
        exif_orientation_normalized: metadata.exif_orientation_normalized,
        read_error: metadata.read_error.clone(),
        ..MetadataReport::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_reports_metadata_that_format_cannot_preserve() {
        let report = MetadataReport {
            exif_present: true,
            icc_present: true,
            ..MetadataReport::default()
        };
        let status = report.status_suffix();
        assert!(status.contains("EXIF"));
        assert!(status.contains("ICC"));
    }

    #[test]
    fn fully_preserved_metadata_has_no_warning_suffix() {
        let report = MetadataReport {
            exif_present: true,
            icc_present: true,
            exif_preserved: true,
            icc_preserved: true,
            exif_orientation_normalized: true,
            ..MetadataReport::default()
        };
        assert!(report.status_suffix().is_empty());
    }
}
