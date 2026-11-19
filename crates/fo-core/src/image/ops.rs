//! Resizing and the target-file-size search.

use super::decode::Decoded;
use super::encode::{encode, EncodeOpts};
use crate::tool::ToolError;
use fo_types::FidelityReport;
use image::imageops::FilterType;
use image::DynamicImage;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeSpec {
    Exact {
        w: u32,
        h: u32,
    },
    Percent(u32),
    /// Fit inside the box, keeping the aspect ratio. Never upscales.
    Fit {
        w: u32,
        h: u32,
    },
}

pub fn target_size(w: u32, h: u32, spec: &ResizeSpec) -> (u32, u32) {
    match *spec {
        ResizeSpec::Exact { w: tw, h: th } => (tw.max(1), th.max(1)),
        ResizeSpec::Percent(p) => (
            ((w as u64 * p as u64) / 100).max(1) as u32,
            ((h as u64 * p as u64) / 100).max(1) as u32,
        ),
        ResizeSpec::Fit { w: bw, h: bh } => {
            let scale = (bw as f64 / w as f64).min(bh as f64 / h as f64).min(1.0);
            (
                ((w as f64 * scale).round() as u32).max(1),
                ((h as f64 * scale).round() as u32).max(1),
            )
        }
    }
}

pub fn resize(img: &DynamicImage, spec: &ResizeSpec) -> DynamicImage {
    let (w, h) = target_size(img.width(), img.height(), spec);
    if (w, h) == (img.width(), img.height()) {
        return img.clone();
    }
    img.resize_exact(w, h, FilterType::Lanczos3)
}

const LOSSY: &[&str] = &["jpg", "webp", "avif"];

/// Encode `d` as `target` no larger than `max_bytes`, searching quality 10..=95.
pub fn encode_to_size(
    d: &Decoded,
    target: &str,
    max_bytes: u64,
    out: &Path,
) -> Result<FidelityReport, ToolError> {
    let size = |p: &Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(u64::MAX);
    let kb = |b: u64| b.div_ceil(1024);
    if !LOSSY.contains(&target) {
        let mut r = encode(
            d,
            target,
            &EncodeOpts {
                lossless: true,
                ..Default::default()
            },
            out,
        )?;
        if target == "png" {
            optimise_png(out)?;
        }
        if size(out) > max_bytes {
            r.warnings.push(format!(
                "Couldn't reach {} KB; smallest is {} KB",
                kb(max_bytes),
                kb(size(out))
            ));
        }
        return Ok(r);
    }
    let probe = out.with_extension(format!("probe.{target}"));
    let (mut lo, mut hi, mut best) = (10u8, 95u8, None);
    for _ in 0..7 {
        if lo > hi {
            break;
        }
        let mid = lo + (hi - lo) / 2;
        encode(
            d,
            target,
            &EncodeOpts {
                quality: mid,
                ..Default::default()
            },
            &probe,
        )?;
        if size(&probe) <= max_bytes {
            best = Some(mid);
            lo = mid + 1;
        } else {
            hi = mid.saturating_sub(1);
        }
    }
    let _ = std::fs::remove_file(&probe);
    let quality = best.unwrap_or(10);
    let mut r = encode(
        d,
        target,
        &EncodeOpts {
            quality,
            ..Default::default()
        },
        out,
    )?;
    if best.is_none() {
        r.warnings.push(format!(
            "Couldn't reach {} KB; smallest is {} KB",
            kb(max_bytes),
            kb(size(out))
        ));
    }
    Ok(r)
}

/// Lossless PNG optimisation in place.
pub fn optimise_png(path: &Path) -> Result<(), ToolError> {
    let opts = oxipng::Options::from_preset(4);
    oxipng::optimize(
        &oxipng::InFile::Path(path.to_path_buf()),
        &oxipng::OutFile::from_path(path.to_path_buf()),
        &opts,
    )
    .map(|_| ())
    .map_err(|e| ToolError::Failed(format!("Couldn't optimise the PNG: {e}")))
}
