//! Saving reviewed scan pages: a PDF (JPEGs embedded unchanged, A4 wide)
//! and/or the page images, never overwriting a file already there.

use crate::image::pdf::{write_pdf, PageSize, PdfPage};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Claims `stem.ext`, else `stem (2).ext`, … in `dir` (create_new, so two
/// saves can't take the same name) and writes `bytes` there.
pub(crate) fn write_new(
    dir: &Path,
    stem: &str,
    ext: &str,
    bytes: &[u8],
) -> Result<PathBuf, String> {
    for n in 1.. {
        let name = if n == 1 {
            format!("{stem}.{ext}")
        } else {
            format!("{stem} ({n}).{ext}")
        };
        let p = dir.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&p) {
            Ok(mut f) => {
                return f.write_all(bytes).map(|_| p.clone()).map_err(|e| {
                    let _ = std::fs::remove_file(&p);
                    format!("Couldn't save the scan: {e}")
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("Couldn't save the scan: {e}")),
        }
    }
    unreachable!()
}

/// Saves `pages` in the order `order` (indexes into `pages`) to `out_dir`
/// as `stem.pdf` and/or `stem (page N).jpg`; returns the files written.
pub fn save_scan(
    pages: &[PathBuf],
    order: &[usize],
    out_dir: &Path,
    stem: &str,
    pdf: bool,
    jpegs: bool,
) -> Result<Vec<PathBuf>, String> {
    if order.is_empty() {
        return Err("There are no pages to save".into());
    }
    if !pdf && !jpegs {
        return Err("Choose PDF, images or both".into());
    }
    if let Some(&bad) = order.iter().find(|&&i| i >= pages.len()) {
        return Err(format!("Page {} isn't in this scan", bad + 1));
    }
    let chosen: Vec<&PathBuf> = order.iter().map(|&i| &pages[i]).collect();
    let mut out = Vec::new();
    if pdf {
        let tmp = tempfile::NamedTempFile::new_in(out_dir)
            .map_err(|e| format!("Couldn't save the scan: {e}"))?;
        write_pdf(chosen.len(), PageSize::ScanA4Width, tmp.path(), |i| {
            let bytes = std::fs::read(chosen[i])?;
            let image = image::load_from_memory(&bytes).map_err(|e| {
                crate::tool::ToolError::Failed(format!("Page {} can't be read: {e}", i + 1))
            })?;
            let is_jpeg = bytes.starts_with(&[0xFF, 0xD8]);
            Ok(PdfPage {
                image,
                jpeg: is_jpeg.then(|| Arc::<[u8]>::from(bytes)),
            })
        })
        .map_err(|e| match e {
            crate::tool::ToolError::Failed(m) => m,
            crate::tool::ToolError::Cancelled => "Cancelled".into(),
        })?;
        let bytes = std::fs::read(tmp.path()).map_err(|e| e.to_string())?;
        out.push(write_new(out_dir, stem, "pdf", &bytes)?);
    }
    if jpegs {
        for (n, p) in chosen.iter().enumerate() {
            let bytes =
                std::fs::read(p).map_err(|e| format!("Page {} can't be read: {e}", n + 1))?;
            out.push(write_new(
                out_dir,
                &format!("{stem} (page {})", n + 1),
                "jpg",
                &bytes,
            )?);
        }
    }
    Ok(out)
}

/// A page's thumbnail, at most `max` pixels on its longer side, as a JPEG
/// `data:` URL the webview can show.
pub fn thumbnail(path: &Path, max: u32) -> Result<String, String> {
    use base64::Engine;
    let img = image::open(path).map_err(|e| format!("Couldn't read the page: {e}"))?;
    let small = img.thumbnail(max, max).to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80)
        .encode_image(&small)
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(out)
    ))
}
