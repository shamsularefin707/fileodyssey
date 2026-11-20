//! The image viewer: size, EXIF in plain words, and bytes the webview can show.

use crate::image::decode::decode;
use fo_types::{FileKind, ImageFormat, ImageInfo};
use std::io::Cursor;
use std::path::Path;

/// Longest edge of a converted (non-browser-format) image.
pub const MAX_DISPLAY_EDGE: u32 = 8192;

/// Formats every webview we target shows natively, and their MIME types.
fn native_mime(kind: FileKind) -> Option<&'static str> {
    let FileKind::Image(f) = kind else {
        return None;
    };
    Some(match f {
        ImageFormat::Png => "image/png",
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Gif => "image/gif",
        ImageFormat::Webp => "image/webp",
        ImageFormat::Bmp => "image/bmp",
        ImageFormat::Ico => "image/x-icon",
        ImageFormat::Svg => "image/svg+xml",
        _ => return None,
    })
}

fn tool_err(e: crate::tool::ToolError) -> String {
    match e {
        crate::tool::ToolError::Failed(m) => m,
        crate::tool::ToolError::Cancelled => "Cancelled".into(),
    }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub fn image_info(path: &Path, kind: FileKind) -> Result<ImageInfo, String> {
    let raw = std::fs::read(path).map_err(|e| format!("Couldn't read the file: {e}"))?;
    let mut exif = exif::Reader::new()
        .read_from_container(&mut Cursor::new(&raw))
        .ok();
    let (mut width, mut height) = match native_mime(kind) {
        Some(_) if kind != FileKind::Image(ImageFormat::Svg) => {
            image::ImageReader::new(Cursor::new(&raw))
                .with_guessed_format()
                .map_err(err)?
                .into_dimensions()
                .map_err(|_| "This image is damaged, so it can't be shown.".to_string())?
        }
        _ => {
            let d = decode(path, &kind).map_err(tool_err)?;
            if exif.is_none() {
                exif = d
                    .exif
                    .clone()
                    .and_then(|e| exif::Reader::new().read_raw(e).ok());
            }
            // Decoding already applied any rotation.
            let img = d.first();
            return Ok(ImageInfo {
                width: img.width(),
                height: img.height(),
                exif: exif.map(|e| fields(&e)).unwrap_or_default(),
            });
        }
    };
    if let Some(e) = &exif {
        // Orientations 5–8 turn the picture on its side; webviews honour them.
        let o = e
            .get_field(exif::Tag::Orientation, exif::In::PRIMARY)
            .and_then(|f| f.value.get_uint(0));
        if matches!(o, Some(5..=8)) {
            std::mem::swap(&mut width, &mut height);
        }
    }
    Ok(ImageInfo {
        width,
        height,
        exif: exif.map(|e| fields(&e)).unwrap_or_default(),
    })
}

/// Bytes and MIME type to show `path` in the webview.
pub fn display_bytes(path: &Path, kind: FileKind) -> Result<(Vec<u8>, &'static str), String> {
    if let Some(mime) = native_mime(kind) {
        let bytes = std::fs::read(path).map_err(|e| format!("Couldn't read the file: {e}"))?;
        return Ok((bytes, mime));
    }
    let d = decode(path, &kind).map_err(tool_err)?;
    let mut img = d.first().clone();
    if img.width().max(img.height()) > MAX_DISPLAY_EDGE {
        img = img.resize(
            MAX_DISPLAY_EDGE,
            MAX_DISPLAY_EDGE,
            image::imageops::FilterType::Triangle,
        );
    }
    let mut out = Cursor::new(Vec::new());
    // 8-bit RGBA keeps HDR/16-bit sources displayable everywhere.
    image::DynamicImage::ImageRgba8(img.to_rgba8())
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(err)?;
    Ok((out.into_inner(), "image/png"))
}

fn text(e: &exif::Exif, tag: exif::Tag) -> Option<String> {
    let f = e.get_field(tag, exif::In::PRIMARY)?;
    let s = match &f.value {
        exif::Value::Ascii(v) => v
            .iter()
            .map(|b| {
                String::from_utf8_lossy(b)
                    .trim_matches(char::from(0))
                    .trim()
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join(" "),
        _ => f.display_value().with_unit(e).to_string(),
    };
    let s = s.trim().to_owned();
    (!s.is_empty()).then_some(s)
}

fn gps(e: &exif::Exif) -> Option<String> {
    use exif::Tag;
    let coord = |t: Tag, r: Tag| -> Option<f64> {
        let f = e.get_field(t, exif::In::PRIMARY)?;
        let exif::Value::Rational(v) = &f.value else {
            return None;
        };
        let dms: Vec<f64> = v.iter().map(|r| r.to_f64()).collect();
        let deg =
            dms.first()? + dms.get(1).unwrap_or(&0.0) / 60.0 + dms.get(2).unwrap_or(&0.0) / 3600.0;
        let neg = text(e, r).is_some_and(|s| s.starts_with('S') || s.starts_with('W'));
        Some(if neg { -deg } else { deg })
    };
    let lat = coord(Tag::GPSLatitude, Tag::GPSLatitudeRef)?;
    let lon = coord(Tag::GPSLongitude, Tag::GPSLongitudeRef)?;
    Some(format!("{lat:.5}, {lon:.5}"))
}

/// The fields people look for, in the order they look for them.
fn fields(e: &exif::Exif) -> Vec<(String, String)> {
    use exif::Tag;
    let mut out = Vec::new();
    let mut push = |label: &str, v: Option<String>| {
        if let Some(v) = v {
            out.push((label.to_owned(), v));
        }
    };
    let camera = match (text(e, Tag::Make), text(e, Tag::Model)) {
        (Some(make), Some(model)) if model.starts_with(&make) => Some(model),
        (Some(make), Some(model)) => Some(format!("{make} {model}")),
        (a, b) => a.or(b),
    };
    push("Camera", camera);
    push("Lens", text(e, Tag::LensModel));
    // EXIF writes dates as "2026:03:14 07:05:00".
    let date = |d: String| d.replacen(':', "-", 2);
    push(
        "Taken",
        text(e, Tag::DateTimeOriginal)
            .or_else(|| text(e, Tag::DateTime))
            .map(date),
    );
    push("Exposure", text(e, Tag::ExposureTime));
    push("Aperture", text(e, Tag::FNumber));
    push("ISO", text(e, Tag::PhotographicSensitivity));
    push("Focal length", text(e, Tag::FocalLength));
    push("Flash", text(e, Tag::Flash));
    push("Location", gps(e));
    push("Description", text(e, Tag::ImageDescription));
    push("Artist", text(e, Tag::Artist));
    push("Copyright", text(e, Tag::Copyright));
    push("Software", text(e, Tag::Software));
    out
}
