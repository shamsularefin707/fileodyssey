//! PDF compression: lossless stream/object packing, plus optional image
//! downscaling and JPEG re-encoding (Balanced/Strong). Text and vector
//! content are never changed.

use crate::detect::detect_kind;
use crate::tool::{Ctx, Tool, ToolError, ToolOutput};
use fo_types::*;
use lopdf::{Document, Object, ObjectId, Stream};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Lossless,
    Balanced,
    Strong,
}

impl Preset {
    pub fn from_option(v: Option<&str>) -> Self {
        match v {
            Some("lossless") => Preset::Lossless,
            Some("strong") => Preset::Strong,
            _ => Preset::Balanced,
        }
    }

    /// (longest side in pixels, JPEG quality) for lossy presets.
    pub fn image_params(self) -> Option<(u32, u8)> {
        match self {
            Preset::Lossless => None,
            Preset::Balanced => Some((2000, 80)),
            Preset::Strong => Some((1400, 60)),
        }
    }

    pub fn option_spec() -> OptionSpec {
        OptionSpec {
            key: "level".into(),
            label: "Compression".into(),
            kind: OptionKind::Choice {
                choices: vec![
                    ("lossless".into(), "Lossless (nothing changes)".into()),
                    ("balanced".into(), "Balanced (smaller images)".into()),
                    ("strong".into(), "Strong (smallest)".into()),
                ],
                default: "balanced".into(),
            },
            targets: vec![],
            show_if: None,
        }
    }
}

fn unreadable(e: impl std::fmt::Display) -> ToolError {
    ToolError::Failed(format!("Couldn't read this PDF: {e}"))
}

fn name_is(st: &Stream, key: &[u8], want: &[u8]) -> bool {
    st.dict
        .get(key)
        .and_then(|o| o.as_name())
        .is_ok_and(|n| n == want)
}

fn int(st: &Stream, key: &[u8]) -> Option<i64> {
    st.dict.get(key).ok().and_then(|o| o.as_i64().ok())
}

/// The image's pixels, if it is a plain 8-bit RGB/Gray image we can safely re-encode.
fn plain_image(st: &Stream) -> Option<image::DynamicImage> {
    if !name_is(st, b"Subtype", b"Image") || int(st, b"BitsPerComponent") != Some(8) {
        return None;
    }
    if st
        .dict
        .get(b"ImageMask")
        .and_then(|o| o.as_bool())
        .unwrap_or(false)
        || st.dict.has(b"Decode")
        || st.dict.has(b"Mask")
    {
        return None;
    }
    let gray = name_is(st, b"ColorSpace", b"DeviceGray");
    if !gray && !name_is(st, b"ColorSpace", b"DeviceRGB") {
        return None;
    }
    let (w, h) = (int(st, b"Width")? as u32, int(st, b"Height")? as u32);
    if w == 0 || h == 0 || w as u64 * h as u64 > crate::image::decode::MAX_PIXELS {
        return None;
    }
    let filter = st.dict.get(b"Filter").ok();
    let single = |name: &[u8]| match filter {
        Some(Object::Name(n)) => n == name,
        Some(Object::Array(a)) => a.len() == 1 && a[0].as_name().is_ok_and(|n| n == name),
        _ => false,
    };
    if single(b"DCTDecode") {
        // e.g. /ColorTransform 0 would decode with the wrong colours.
        if st.dict.has(b"DecodeParms") {
            return None;
        }
        let img =
            image::load_from_memory_with_format(&st.content, image::ImageFormat::Jpeg).ok()?;
        return (img.width() == w && img.height() == h).then_some(img);
    }
    if filter.is_some() && !single(b"FlateDecode") {
        return None;
    }
    let comps = if gray { 1 } else { 3 };
    let expected = (w * h) as usize * comps;
    // Never inflate beyond what the declared size can hold (bomb guard).
    let raw = if filter.is_some() {
        st.decompressed_content_with_limit(expected + 1).ok()?
    } else {
        st.content.clone()
    };
    if raw.len() != expected {
        return None;
    }
    if gray {
        image::GrayImage::from_raw(w, h, raw).map(image::DynamicImage::ImageLuma8)
    } else {
        image::RgbImage::from_raw(w, h, raw).map(image::DynamicImage::ImageRgb8)
    }
}

struct Recompressed {
    reencoded: u64,
    scaled: u64,
}

fn recompress_images(doc: &mut Document, max_px: u32, quality: u8) -> Recompressed {
    // Soft masks keep their exact alpha; never JPEG them.
    let masks: HashSet<ObjectId> = doc
        .objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .filter_map(|st| st.dict.get(b"SMask").and_then(|m| m.as_reference()).ok())
        .collect();
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut out = Recompressed {
        reencoded: 0,
        scaled: 0,
    };
    for id in ids {
        if masks.contains(&id) {
            continue;
        }
        let Some(Object::Stream(st)) = doc.objects.get_mut(&id) else {
            continue;
        };
        let Some(img) = plain_image(st) else { continue };
        let gray = matches!(img, image::DynamicImage::ImageLuma8(_));
        let long = img.width().max(img.height());
        let (img, scaled) = if long > max_px {
            let r = img.resize(max_px, max_px, image::imageops::FilterType::Lanczos3);
            (r, true)
        } else {
            (img, false)
        };
        let mut jpeg = Vec::new();
        let encoded = if gray {
            let g = img.to_luma8();
            jpeg_encoder::Encoder::new(&mut jpeg, quality).encode(
                g.as_raw(),
                g.width() as u16,
                g.height() as u16,
                jpeg_encoder::ColorType::Luma,
            )
        } else {
            let c = img.to_rgb8();
            jpeg_encoder::Encoder::new(&mut jpeg, quality).encode(
                c.as_raw(),
                c.width() as u16,
                c.height() as u16,
                jpeg_encoder::ColorType::Rgb,
            )
        };
        if encoded.is_err() || jpeg.len() >= st.content.len() {
            continue; // only swap in a smaller result
        }
        st.dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
        st.dict.remove(b"DecodeParms");
        st.dict.set("Width", img.width() as i64);
        st.dict.set("Height", img.height() as i64);
        st.dict.set(
            "ColorSpace",
            Object::Name(if gray {
                b"DeviceGray".to_vec()
            } else {
                b"DeviceRGB".to_vec()
            }),
        );
        st.set_content(jpeg);
        st.allows_compression = false;
        out.reencoded += 1;
        out.scaled += scaled as u64;
    }
    out
}

pub fn compress(input: &Path, preset: Preset, out: &Path) -> Result<FidelityReport, ToolError> {
    let original = std::fs::read(input)?;
    let mut doc = Document::load_mem(&original).map_err(unreadable)?;
    if doc.is_encrypted() || doc.trailer.has(b"Encrypt") {
        return Err(ToolError::Failed(
            "Password-protected PDFs aren't supported yet".into(),
        ));
    }
    let pages_before = doc.get_pages().len() as u64;
    let mut warnings = vec![];
    let mut reencoded = 0;
    if let Some((max_px, quality)) = preset.image_params() {
        let r = recompress_images(&mut doc, max_px, quality);
        reencoded = r.reencoded;
        if r.reencoded > 0 {
            warnings.push(format!(
                "{} images re-encoded as JPEG at quality {quality}",
                r.reencoded
            ));
        }
        if r.scaled > 0 {
            warnings.push(format!(
                "{} images scaled down to at most {max_px} px",
                r.scaled
            ));
        }
    }
    // Empty streams are kept: they can be referenced (e.g. an empty widget
    // appearance), and deleting them leaves dangling references.
    // XMP metadata stays unfiltered so tools (and PDF/A) can read it.
    for obj in doc.objects.values_mut() {
        if let Object::Stream(st) = obj {
            if name_is(st, b"Type", b"Metadata") {
                st.allows_compression = false;
            }
        }
    }
    doc.prune_objects();
    doc.renumber_objects();
    doc.compress();
    let mut packed = Vec::new();
    // lopdf's object-stream writer (0.47) produces files that poppler and
    // qpdf reject, so streams are compressed but objects are written plainly.
    doc.save_to(&mut packed)
        .map_err(|e| ToolError::Failed(format!("Couldn't save the PDF: {e}")))?;

    let size_after = if packed.len() < original.len() {
        std::fs::write(out, &packed)?;
        packed.len()
    } else {
        std::fs::write(out, &original)?;
        warnings = vec!["Already compact; saved unchanged".into()];
        reencoded = 0;
        original.len()
    };
    let pages_after = Document::load(out)
        .map(|d| d.get_pages().len() as u64)
        .unwrap_or(0);
    let kb = |b: usize| (b as u64).div_ceil(1024);
    Ok(FidelityReport {
        metrics: vec![
            Metric {
                name: "Pages".into(),
                before: pages_before,
                after: pages_after,
                informational: false,
            },
            Metric {
                name: "File size (KB)".into(),
                before: kb(original.len()),
                after: kb(size_after),
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

/// `pdf.compress`: Compress tab, PDFs only.
pub struct PdfCompress;

pub fn tool() -> Arc<dyn Tool> {
    Arc::new(PdfCompress)
}

impl Tool for PdfCompress {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            id: "pdf.compress".into(),
            label: "Compress PDF".into(),
            category: Category::Compress,
            targets: vec!["pdf".into()],
            best_effort: false,
            platforms: vec![Platform::Desktop, Platform::Android],
            options: vec![Preset::option_spec()],
            combines: false,
            needs: None,
        }
    }

    fn accepts(&self, kind: &FileKind) -> bool {
        *kind == FileKind::Pdf
    }

    fn output_label(&self) -> &'static str {
        "compressed"
    }

    fn run(&self, input: &Path, ctx: &Ctx) -> Result<ToolOutput, ToolError> {
        if detect_kind(input)? != FileKind::Pdf {
            return Err(ToolError::Failed("This isn't a PDF".into()));
        }
        (ctx.progress)(0.1);
        let preset = Preset::from_option(ctx.options.get("level").and_then(|v| v.as_str()));
        let out = ctx.work_dir.join("out.pdf");
        let report = compress(input, preset, &out)?;
        Ok(ToolOutput {
            files: vec![(out, "pdf".into())],
            report,
        })
    }
}
