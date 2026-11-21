//! Office (DOCX/PPTX/XLSX/ODT) and EPUB compression. These are zip
//! packages: entries are rewritten in order with maximum Deflate, PNGs are
//! optimised losslessly, and JPEGs are re-encoded only for lossy presets.
//! Every other entry's bytes are copied unchanged.

use crate::archive::guard::{Budget, MAX_ENTRIES};
use crate::detect::detect_kind;
use crate::pdfopt::Preset;
use crate::tool::{Ctx, Tool, ToolError, ToolOutput};
use fo_types::*;
use image::metadata::Orientation;
use image::ImageDecoder;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::sync::Arc;

fn unreadable(e: impl std::fmt::Display) -> ToolError {
    ToolError::Failed(format!("Couldn't read this file: {e}"))
}

/// A smaller JPEG, or `None` to keep the original bytes.
fn smaller_jpeg(bytes: &[u8], max_px: u32, quality: u8) -> Option<(Vec<u8>, bool)> {
    let mut dec = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_decoder()
        .ok()?;
    // Re-encoding drops EXIF, so a rotated photo would turn sideways: skip it.
    if dec
        .orientation()
        .is_ok_and(|o| o != Orientation::NoTransforms)
        || dec.icc_profile().ok().flatten().is_some()
    {
        return None;
    }
    let img = image::DynamicImage::from_decoder(dec).ok()?;
    let long = img.width().max(img.height());
    let (img, scaled) = if long > max_px {
        (
            img.resize(max_px, max_px, image::imageops::FilterType::Lanczos3),
            true,
        )
    } else {
        (img, false)
    };
    let rgb = img.to_rgb8();
    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, quality)
        .encode(
            rgb.as_raw(),
            rgb.width() as u16,
            rgb.height() as u16,
            jpeg_encoder::ColorType::Rgb,
        )
        .ok()?;
    (out.len() < bytes.len()).then_some((out, scaled))
}

pub fn compress(input: &Path, preset: Preset, out: &Path) -> Result<FidelityReport, ToolError> {
    let original_len = std::fs::metadata(input)?.len();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(input)?).map_err(unreadable)?;
    if zip.len() as u64 > MAX_ENTRIES {
        return Err(ToolError::Failed(format!(
            "This file has more than {MAX_ENTRIES} parts, which looks unsafe"
        )));
    }
    let mut budget = Budget::new(original_len);
    let mut w = zip::ZipWriter::new(std::fs::File::create(out)?);
    let deflate = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(9));
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let png_opts = oxipng::Options::from_preset(4);
    let (mut reencoded, mut scaled, mut written) = (0u64, 0u64, 0u64);

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(unreadable)?;
        let name = entry.name().to_owned();
        if entry.is_dir() {
            w.add_directory(name.as_str(), deflate)
                .map_err(unreadable)?;
            written += 1;
            continue;
        }
        let mut bytes = Vec::new();
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = entry.read(&mut buf).map_err(unreadable)?;
            if n == 0 {
                break;
            }
            budget.charge(n as u64)?;
            bytes.extend_from_slice(&buf[..n]);
        }
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".png") {
            if let Ok(opt) = oxipng::optimize_from_memory(&bytes, &png_opts) {
                if opt.len() < bytes.len() {
                    bytes = opt;
                }
            }
        } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
            if let Some((max_px, quality)) = preset.image_params() {
                if let Some((jpeg, was_scaled)) = smaller_jpeg(&bytes, max_px, quality) {
                    bytes = jpeg;
                    reencoded += 1;
                    scaled += was_scaled as u64;
                }
            }
        }
        // EPUB/ODF readers require "mimetype" first and uncompressed.
        let opts = if name == "mimetype" { stored } else { deflate };
        w.start_file(name.as_str(), opts).map_err(unreadable)?;
        w.write_all(&bytes)?;
        written += 1;
    }
    w.finish().map_err(unreadable)?;

    let mut warnings = vec![];
    if let Some((max_px, quality)) = preset.image_params() {
        if reencoded > 0 {
            warnings.push(format!(
                "{reencoded} images re-encoded as JPEG at quality {quality}"
            ));
        }
        if scaled > 0 {
            warnings.push(format!(
                "{scaled} images scaled down to at most {max_px} px"
            ));
        }
    }
    let mut new_len = std::fs::metadata(out)?.len();
    if new_len >= original_len {
        std::fs::copy(input, out)?;
        new_len = original_len;
        warnings = vec!["Already compact; saved unchanged".into()];
        reencoded = 0;
    }
    let kb = |b: u64| b.div_ceil(1024);
    Ok(FidelityReport {
        metrics: vec![
            Metric {
                name: "Files inside".into(),
                before: zip.len() as u64,
                after: written,
                informational: false,
            },
            Metric {
                name: "File size (KB)".into(),
                before: kb(original_len),
                after: kb(new_len),
                informational: true,
            },
            Metric {
                name: "Images re-encoded".into(),
                before: 0,
                after: reencoded,
                informational: true,
            },
        ],
        text_similarity: None,
        warnings,
    })
}

/// `office.compress`: Compress tab, Office documents and EPUB books.
pub struct OfficeCompress;

pub fn tool() -> Arc<dyn Tool> {
    Arc::new(OfficeCompress)
}

impl Tool for OfficeCompress {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            id: "office.compress".into(),
            label: "Compress document".into(),
            category: Category::Compress,
            targets: vec!["same".into()],
            best_effort: false,
            platforms: vec![Platform::Desktop, Platform::Android],
            options: vec![Preset::option_spec()],
            combines: false,
            needs: None,
        }
    }

    fn accepts(&self, kind: &FileKind) -> bool {
        matches!(
            kind,
            FileKind::Docx
                | FileKind::Pptx
                | FileKind::Xlsx
                | FileKind::Odt
                | FileKind::Ods
                | FileKind::Odp
                | FileKind::Epub
        )
    }

    fn output_label(&self) -> &'static str {
        "compressed"
    }

    fn run(&self, input: &Path, ctx: &Ctx) -> Result<ToolOutput, ToolError> {
        let ext = match detect_kind(input)? {
            FileKind::Docx => "docx",
            FileKind::Pptx => "pptx",
            FileKind::Xlsx => "xlsx",
            FileKind::Odt => "odt",
            FileKind::Ods => "ods",
            FileKind::Odp => "odp",
            FileKind::Epub => "epub",
            _ => {
                return Err(ToolError::Failed(
                    "This isn't an Office document or EPUB".into(),
                ))
            }
        };
        (ctx.progress)(0.1);
        let preset = Preset::from_option(ctx.options.get("level").and_then(|v| v.as_str()));
        let out = ctx.work_dir.join(format!("out.{ext}"));
        let report = compress(input, preset, &out)?;
        Ok(ToolOutput {
            files: vec![(out, ext.into())],
            report,
        })
    }
}
