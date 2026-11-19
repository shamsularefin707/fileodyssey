//! Image decoding: every read format → `Decoded` (frames + metadata).
//! Sizes are checked from headers before any pixels are allocated.

use crate::tool::ToolError;
use fo_types::{FileKind, ImageFormat};
use image::metadata::Orientation;
use image::{AnimationDecoder, DynamicImage, ImageDecoder, ImageReader, Limits};
use std::io::{BufReader, Cursor};
use std::path::Path;
use std::sync::Arc;

pub const MAX_PIXELS: u64 = 300_000_000;
pub const MAX_FRAMES: usize = 1000;

#[derive(Clone)]
pub struct Frame {
    pub image: DynamicImage,
    pub delay_ms: u32,
}

#[derive(Clone)]
pub struct Decoded {
    pub frames: Vec<Frame>,
    pub icc: Option<Vec<u8>>,
    /// EXIF with the orientation tag already applied and reset.
    pub exif: Option<Vec<u8>>,
    pub bit_depth: u8,
    pub has_alpha: bool,
    pub source: ImageFormat,
    /// Original file bytes, kept for JPEG sources so PDF output can embed
    /// them unchanged.
    pub source_bytes: Option<Arc<[u8]>>,
    /// Things lost while reading (e.g. extra TIFF pages); copied into the report.
    pub notes: Vec<String>,
}

impl Decoded {
    pub fn first(&self) -> &DynamicImage {
        &self.frames[0].image
    }
}

/// Formats M2 can read. AVIF/HEIC reading lands in M8 (see plan rulings).
pub fn decodable(kind: &FileKind) -> bool {
    matches!(kind, FileKind::Image(f) if !matches!(f, ImageFormat::Avif | ImageFormat::Heic))
}

fn unreadable(e: impl std::fmt::Display) -> ToolError {
    ToolError::Failed(format!("Couldn't read this image: {e}"))
}

pub fn check_size(w: u32, h: u32) -> Result<(), ToolError> {
    let px = w as u64 * h as u64;
    if px > MAX_PIXELS {
        return Err(ToolError::Failed(format!(
            "This image is too large to open safely ({} megapixels)",
            px / 1_000_000
        )));
    }
    Ok(())
}

fn limits() -> Limits {
    let mut l = Limits::default();
    // RGBA16 at the pixel cap, plus headroom for decoder buffers.
    l.max_alloc = Some(MAX_PIXELS * 8 + (256 << 20));
    l
}

fn depth_and_alpha(img: &DynamicImage) -> (u8, bool) {
    let c = img.color();
    let depth = (c.bits_per_pixel() / c.channel_count() as u16) as u8;
    (depth.max(8), c.has_alpha())
}

fn single(
    image: DynamicImage,
    source: ImageFormat,
    icc: Option<Vec<u8>>,
    exif: Option<Vec<u8>>,
) -> Decoded {
    let (bit_depth, has_alpha) = depth_and_alpha(&image);
    Decoded {
        frames: vec![Frame { image, delay_ms: 0 }],
        icc,
        exif,
        bit_depth,
        has_alpha,
        source,
        source_bytes: None,
        notes: vec![],
    }
}

pub fn decode(path: &Path, kind: &FileKind) -> Result<Decoded, ToolError> {
    let FileKind::Image(format) = *kind else {
        return Err(ToolError::Failed("This isn't an image".into()));
    };
    match format {
        ImageFormat::Avif | ImageFormat::Heic => Err(ToolError::Failed(
            "Reading this image format isn't supported yet".into(),
        )),
        ImageFormat::Svg => decode_svg(path),
        ImageFormat::Psd => decode_psd(path),
        ImageFormat::Raw => decode_raw(path),
        ImageFormat::Jxl => decode_jxl(path),
        ImageFormat::Gif => decode_animated(path, format, true),
        ImageFormat::Webp => decode_animated(path, format, false),
        _ => decode_still(path, format),
    }
}

fn reader(path: &Path) -> Result<ImageReader<BufReader<std::fs::File>>, ToolError> {
    let mut r = ImageReader::open(path)
        .map_err(unreadable)?
        .with_guessed_format()
        .map_err(unreadable)?;
    r.limits(limits());
    Ok(r)
}

fn decode_still(path: &Path, format: ImageFormat) -> Result<Decoded, ToolError> {
    let (w, h) = reader(path)?.into_dimensions().map_err(unreadable)?;
    check_size(w, h)?;
    let mut dec = reader(path)?.into_decoder().map_err(unreadable)?;
    let icc = dec.icc_profile().ok().flatten();
    let mut exif = dec.exif_metadata().ok().flatten();
    let orientation = dec.orientation().unwrap_or(Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(dec).map_err(unreadable)?;
    img.apply_orientation(orientation);
    if let Some(e) = exif.as_mut() {
        let _ = Orientation::remove_from_exif_chunk(e);
    }
    let mut d = single(img, format, icc, exif);
    if format == ImageFormat::Jpeg && orientation == Orientation::NoTransforms {
        // Only 1- or 3-component JPEGs can be embedded in a PDF as DeviceGray/RGB.
        let bytes = std::fs::read(path).ok();
        if bytes
            .as_deref()
            .and_then(jpeg_components)
            .is_some_and(|c| c == 1 || c == 3)
        {
            d.source_bytes = bytes.map(Arc::from);
        }
    }
    if format == ImageFormat::Tiff {
        let pages = tiff_pages(path);
        if pages > 1 {
            d.notes
                .push(format!("Only the first of {pages} pages was used"));
        }
    }
    Ok(d)
}

/// Number of colour components in a JPEG's frame header (SOFn).
fn jpeg_components(b: &[u8]) -> Option<u8> {
    let mut i = 2;
    while i + 9 < b.len() {
        if b[i] != 0xFF {
            return None;
        }
        let m = b[i + 1];
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        if matches!(m, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
            return b.get(i + 9).copied();
        }
        i += 2 + len;
    }
    None
}

fn tiff_pages(path: &Path) -> usize {
    let Ok(file) = std::fs::File::open(path) else {
        return 1;
    };
    let Ok(mut dec) = tiff::decoder::Decoder::new(BufReader::new(file)) else {
        return 1;
    };
    let mut n = 1;
    while dec.more_images() && n < 10_000 {
        if dec.next_image().is_err() {
            break;
        }
        n += 1;
    }
    n
}

fn decode_animated(path: &Path, format: ImageFormat, gif: bool) -> Result<Decoded, ToolError> {
    let (w, h) = reader(path)?.into_dimensions().map_err(unreadable)?;
    check_size(w, h)?;
    let bytes = std::fs::read(path).map_err(unreadable)?;
    let frames = if gif {
        let mut d = image::codecs::gif::GifDecoder::new(Cursor::new(&bytes)).map_err(unreadable)?;
        d.set_limits(limits()).map_err(unreadable)?;
        collect_frames(d.into_frames())?
    } else {
        let mut d =
            image::codecs::webp::WebPDecoder::new(Cursor::new(&bytes)).map_err(unreadable)?;
        d.set_limits(limits()).map_err(unreadable)?;
        if !d.has_animation() {
            return decode_still(path, format);
        }
        collect_frames(d.into_frames())?
    };
    let (bit_depth, has_alpha) = (8, true);
    Ok(Decoded {
        frames,
        icc: None,
        exif: None,
        bit_depth,
        has_alpha,
        source: format,
        source_bytes: None,
        notes: vec![],
    })
}

/// Every frame of an animation is held as a full-canvas image, so the total
/// across frames must stay within the same pixel budget as a single image.
pub fn check_frames(w: u32, h: u32, n: usize) -> Result<(), ToolError> {
    let total = w as u64 * h as u64 * n as u64;
    if total > MAX_PIXELS {
        return Err(ToolError::Failed(format!(
            "This animation is too large to open safely ({} megapixels across {n} frames)",
            total / 1_000_000
        )));
    }
    Ok(())
}

fn collect_frames(frames: image::Frames<'_>) -> Result<Vec<Frame>, ToolError> {
    let mut out = Vec::new();
    for f in frames {
        let f = f.map_err(unreadable)?;
        if out.len() >= MAX_FRAMES {
            return Err(ToolError::Failed(format!(
                "This animation has more than {MAX_FRAMES} frames, which is too many to open safely"
            )));
        }
        let (fw, fh) = f.buffer().dimensions();
        check_frames(fw, fh, out.len() + 1)?;
        let (n, d) = f.delay().numer_denom_ms();
        let delay_ms = n.checked_div(d).unwrap_or(0);
        out.push(Frame {
            image: DynamicImage::ImageRgba8(f.into_buffer()),
            delay_ms,
        });
    }
    if out.is_empty() {
        return Err(unreadable("no frames"));
    }
    Ok(out)
}

/// DejaVu Sans (Bitstream Vera licence, see assets/fonts/DejaVu-LICENSE.txt).
const BUNDLED_FONT: &[u8] = include_bytes!("../../assets/fonts/DejaVuSans.ttf");
const BUNDLED_FAMILY: &str = "DejaVu Sans";

fn decode_svg(path: &Path) -> Result<Decoded, ToolError> {
    use resvg::{tiny_skia, usvg};
    use usvg::fontdb;
    let data = std::fs::read(path).map_err(unreadable)?;
    let mut opts = usvg::Options {
        resources_dir: path.parent().map(|p| p.to_path_buf()),
        ..Default::default()
    };
    let fonts = opts.fontdb_mut();
    fonts.load_system_fonts();
    #[cfg(target_os = "android")]
    fonts.load_fonts_dir("/system/fonts");
    // A bundled font backs every generic family, so text renders even when the
    // drawing's default ("Times New Roman") or requested font isn't installed.
    fonts.load_font_data(BUNDLED_FONT.to_vec());
    for set in [
        fontdb::Database::set_serif_family,
        fontdb::Database::set_sans_serif_family,
        fontdb::Database::set_cursive_family,
        fontdb::Database::set_fantasy_family,
        fontdb::Database::set_monospace_family,
    ] {
        set(fonts, BUNDLED_FAMILY);
    }
    opts.font_family = BUNDLED_FAMILY.to_owned();
    let no_fonts = opts.fontdb.is_empty();
    let tree = usvg::Tree::from_data(&data, &opts).map_err(unreadable)?;
    let size = tree.size().to_int_size();
    check_size(size.width(), size.height())?;
    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height())
        .ok_or_else(|| unreadable("empty drawing"))?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    // tiny-skia stores premultiplied RGBA; convert to straight alpha.
    let mut rgba = Vec::with_capacity(pixmap.data().len());
    for p in pixmap.pixels() {
        let c = p.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    let img = image::RgbaImage::from_raw(size.width(), size.height(), rgba)
        .ok_or_else(|| unreadable("bad size"))?;
    let mut d = single(DynamicImage::ImageRgba8(img), ImageFormat::Svg, None, None);
    if no_fonts && String::from_utf8_lossy(&data).contains("<text") {
        d.notes
            .push("Text in this drawing couldn't be drawn: no fonts were found".into());
    }
    Ok(d)
}

fn decode_psd(path: &Path) -> Result<Decoded, ToolError> {
    let data = std::fs::read(path).map_err(unreadable)?;
    let psd = psd::Psd::from_bytes(&data).map_err(unreadable)?;
    check_size(psd.width(), psd.height())?;
    let img = image::RgbaImage::from_raw(psd.width(), psd.height(), psd.rgba())
        .ok_or_else(|| unreadable("bad size"))?;
    Ok(single(
        DynamicImage::ImageRgba8(img),
        ImageFormat::Psd,
        None,
        None,
    ))
}

fn decode_raw(path: &Path) -> Result<Decoded, ToolError> {
    // Read the dimensions without decoding the sensor data, then decode.
    let src = rawler::rawsource::RawSource::new(path).map_err(unreadable)?;
    let decoder = rawler::get_decoder(&src).map_err(unreadable)?;
    let params = rawler::decoders::RawDecodeParams::default();
    let header = decoder.raw_image(&src, &params, true).map_err(unreadable)?;
    check_size(header.width as u32, header.height as u32)?;
    let raw = decoder
        .raw_image(&src, &params, false)
        .map_err(unreadable)?;
    let img = rawler::imgop::develop::RawDevelop::default()
        .develop_intermediate(&raw)
        .map_err(unreadable)?
        .to_dynamic_image()
        .ok_or_else(|| unreadable("couldn't develop the RAW image"))?;
    Ok(single(img, ImageFormat::Raw, None, None))
}

fn decode_jxl(path: &Path) -> Result<Decoded, ToolError> {
    let file = std::fs::File::open(path).map_err(unreadable)?;
    let mut dec =
        jxl_oxide::integration::JxlDecoder::new(BufReader::new(file)).map_err(unreadable)?;
    let (w, h) = dec.dimensions();
    check_size(w, h)?;
    let icc = dec.icc_profile().ok().flatten();
    let img = DynamicImage::from_decoder(dec).map_err(unreadable)?;
    Ok(single(img, ImageFormat::Jxl, icc, None))
}
