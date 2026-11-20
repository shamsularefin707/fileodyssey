//! Image encoding to every write format, reporting anything that couldn't be
//! carried over (spec §7: nothing is lost silently).

use super::decode::Decoded;
use super::pdf::{write_pdf, PageSize, PdfPage};
use crate::tool::ToolError;
use fo_types::{FidelityReport, Metric};
use image::codecs::gif::{GifEncoder, Repeat};
use image::{DynamicImage, ImageEncoder, Rgba, RgbaImage};
use std::io::BufWriter;
use std::path::Path;

pub const WRITE_TARGETS: &[&str] = &[
    "png", "jpg", "webp", "avif", "jxl", "gif", "bmp", "ico", "tiff", "tga", "qoi", "pnm", "pdf",
];

#[derive(Debug, Clone, Copy)]
pub struct EncodeOpts {
    pub quality: u8,
    pub keep_metadata: bool,
    pub lossless: bool,
}

impl Default for EncodeOpts {
    fn default() -> Self {
        Self {
            quality: 90,
            keep_metadata: true,
            lossless: false,
        }
    }
}

/// Collects metrics and warnings while encoding.
struct Notes<'a> {
    d: &'a Decoded,
    warnings: Vec<String>,
    frames_out: usize,
    depth_out: u8,
    size_out: (u32, u32),
}

impl<'a> Notes<'a> {
    fn new(d: &'a Decoded) -> Self {
        let f = d.first();
        Self {
            d,
            warnings: d.notes.clone(),
            frames_out: 1,
            depth_out: 8,
            size_out: (f.width(), f.height()),
        }
    }
    fn warn(&mut self, w: impl Into<String>) {
        self.warnings.push(w.into());
    }
    fn single_frame(&mut self) {
        let n = self.d.frames.len();
        if n > 1 {
            self.warn(format!("Animation dropped: kept the first of {n} frames"));
        }
        self.frames_out = 1;
    }
    fn depth8(&mut self) {
        if self.d.bit_depth > 8 {
            self.warn(format!(
                "Colour depth reduced from {} to 8 bits",
                self.d.bit_depth
            ));
        }
        self.depth_out = 8;
    }
    fn no_metadata(&mut self, ext: &str) {
        if self.d.icc.is_some() || self.d.exif.is_some() {
            self.warn(format!("Metadata (EXIF/colour profile) not kept in .{ext}"));
        }
    }
    fn lossy(&mut self, q: u8) {
        self.warn(format!("Saved with lossy compression at quality {q}"));
    }
    fn finish(self) -> FidelityReport {
        let f = self.d.first();
        let m = |name: &str, before: u64, after: u64| Metric {
            name: name.into(),
            before,
            after,
            informational: false,
        };
        FidelityReport {
            metrics: vec![
                m("Width", f.width() as u64, self.size_out.0 as u64),
                m("Height", f.height() as u64, self.size_out.1 as u64),
                m("Frames", self.d.frames.len() as u64, self.frames_out as u64),
                m("Bit depth", self.d.bit_depth as u64, self.depth_out as u64),
            ],
            text_similarity: None,
            warnings: self.warnings,
        }
    }
}

fn failed(e: impl std::fmt::Display) -> ToolError {
    ToolError::Failed(format!("Couldn't save the image: {e}"))
}

/// Composite onto white so transparent areas don't turn black.
fn flatten_white(img: &DynamicImage) -> image::RgbImage {
    let rgba = img.to_rgba8();
    let mut out = image::RgbImage::new(rgba.width(), rgba.height());
    for (o, p) in out.pixels_mut().zip(rgba.pixels()) {
        let a = p.0[3] as u32;
        let blend = |c: u8| ((c as u32 * a + 255 * (255 - a)) / 255) as u8;
        *o = image::Rgb([blend(p.0[0]), blend(p.0[1]), blend(p.0[2])]);
    }
    out
}

fn has_transparency(img: &DynamicImage) -> bool {
    img.color().has_alpha() && img.to_rgba8().pixels().any(|p| p.0[3] < 255)
}

/// 8-bit image keeping alpha only when it exists.
fn eight_bit(img: &DynamicImage) -> DynamicImage {
    if img.color().has_alpha() {
        DynamicImage::ImageRgba8(img.to_rgba8())
    } else {
        DynamicImage::ImageRgb8(img.to_rgb8())
    }
}

fn exif_app1(exif: &[u8]) -> Vec<u8> {
    [b"Exif\0\0".as_slice(), exif].concat()
}

pub fn encode(
    d: &Decoded,
    target: &str,
    opts: &EncodeOpts,
    out: &Path,
) -> Result<FidelityReport, ToolError> {
    let mut n = Notes::new(d);
    let img = d.first();
    let (w, h) = (img.width(), img.height());
    let create = || {
        std::fs::File::create(out)
            .map(BufWriter::new)
            .map_err(failed)
    };

    match target {
        "png" => {
            n.single_frame();
            let deep = d.bit_depth > 8;
            let im = if deep {
                n.depth_out = 16;
                if img.color().has_alpha() {
                    DynamicImage::ImageRgba16(img.to_rgba16())
                } else {
                    DynamicImage::ImageRgb16(img.to_rgb16())
                }
            } else {
                eight_bit(img)
            };
            let mut enc = image::codecs::png::PngEncoder::new(create()?);
            if opts.keep_metadata {
                if let Some(icc) = &d.icc {
                    let _ = enc.set_icc_profile(icc.clone());
                }
                if let Some(exif) = &d.exif {
                    let _ = enc.set_exif_metadata(exif.clone());
                }
            }
            enc.write_image(im.as_bytes(), w, h, im.color().into())
                .map_err(failed)?;
        }
        "jpg" | "jpeg" => {
            n.single_frame();
            n.depth8();
            if has_transparency(img) {
                n.warn("Transparency replaced with white");
            }
            let rgb = flatten_white(img);
            let mut enc = jpeg_encoder::Encoder::new_file(out, opts.quality).map_err(failed)?;
            if opts.keep_metadata {
                if let Some(icc) = &d.icc {
                    enc.add_icc_profile(icc).map_err(failed)?;
                }
                if let Some(exif) = &d.exif {
                    enc.add_app_segment(1, exif_app1(exif)).map_err(failed)?;
                }
            }
            enc.encode(
                rgb.as_raw(),
                w as u16,
                h as u16,
                jpeg_encoder::ColorType::Rgb,
            )
            .map_err(failed)?;
            n.lossy(opts.quality);
        }
        "webp" => {
            n.depth8();
            n.no_metadata("webp");
            let mut config =
                webp::WebPConfig::new().map_err(|_| failed("WebP encoder unavailable"))?;
            config.lossless = opts.lossless as i32;
            config.quality = opts.quality as f32;
            let bytes: Vec<u8> = if d.frames.len() > 1 {
                n.frames_out = d.frames.len();
                let rgba: Vec<RgbaImage> = d.frames.iter().map(|f| f.image.to_rgba8()).collect();
                let mut enc = webp::AnimEncoder::new(w, h, &config);
                enc.set_loop_count(0);
                let mut t = 0i32;
                for (f, im) in d.frames.iter().zip(&rgba) {
                    enc.add_frame(webp::AnimFrame::from_rgba(im.as_raw(), w, h, t));
                    t += f.delay_ms as i32;
                }
                enc.try_encode()
                    .map_err(|e| failed(format!("{e:?}")))?
                    .to_vec()
            } else {
                let rgba = img.to_rgba8();
                let enc = webp::Encoder::from_rgba(rgba.as_raw(), w, h);
                enc.encode_advanced(&config)
                    .map_err(|e| failed(format!("{e:?}")))?
                    .to_vec()
            };
            std::fs::write(out, bytes).map_err(failed)?;
            if !opts.lossless {
                n.lossy(opts.quality);
            }
        }
        "avif" => {
            n.single_frame();
            n.depth8();
            n.no_metadata("avif");
            let rgba = img.to_rgba8();
            let px: Vec<rgb::RGBA8> = rgba
                .pixels()
                .map(|p| rgb::RGBA8::new(p[0], p[1], p[2], p[3]))
                .collect();
            let res = ravif::Encoder::new()
                .with_quality(opts.quality as f32)
                .with_alpha_quality(opts.quality as f32)
                .with_speed(6)
                .encode_rgba(ravif::Img::new(px.as_slice(), w as usize, h as usize))
                .map_err(failed)?;
            std::fs::write(out, res.avif_file).map_err(failed)?;
            n.lossy(opts.quality);
        }
        "jxl" => {
            use zune_core::bit_depth::BitDepth;
            use zune_core::colorspace::ColorSpace;
            use zune_core::options::EncoderOptions;
            n.single_frame();
            n.depth8();
            n.no_metadata("jxl");
            let (data, cs) = if img.color().has_alpha() {
                (img.to_rgba8().into_raw(), ColorSpace::RGBA)
            } else {
                (img.to_rgb8().into_raw(), ColorSpace::RGB)
            };
            let options = EncoderOptions::new(w as usize, h as usize, cs, BitDepth::Eight);
            let mut buf = Vec::new();
            zune_jpegxl::JxlSimpleEncoder::new(&data, options)
                .encode(&mut buf)
                .map_err(|e| failed(format!("{e:?}")))?;
            std::fs::write(out, buf).map_err(failed)?;
        }
        "gif" => {
            n.depth8();
            n.no_metadata("gif");
            n.frames_out = d.frames.len();
            if count_colours(img) > 256 {
                n.warn("Colours reduced to a 256-colour palette");
            }
            let mut enc = GifEncoder::new_with_speed(create()?, 10);
            enc.set_repeat(Repeat::Infinite).map_err(failed)?;
            let frames = d.frames.iter().map(|f| {
                image::Frame::from_parts(
                    f.image.to_rgba8(),
                    0,
                    0,
                    image::Delay::from_numer_denom_ms(f.delay_ms, 1),
                )
            });
            enc.encode_frames(frames).map_err(failed)?;
        }
        "ico" => {
            n.single_frame();
            n.depth8();
            n.no_metadata("ico");
            let largest = w.max(h);
            let sizes: Vec<u32> = [16u32, 32, 48, 256]
                .into_iter()
                .filter(|s| *s <= largest.max(16))
                .collect();
            let images: Vec<RgbaImage> = sizes.iter().map(|&s| square(img, s)).collect();
            let frames: Vec<image::codecs::ico::IcoFrame> = images
                .iter()
                .map(|im| {
                    image::codecs::ico::IcoFrame::as_png(
                        im.as_raw(),
                        im.width(),
                        im.height(),
                        image::ExtendedColorType::Rgba8,
                    )
                })
                .collect::<Result<_, _>>()
                .map_err(failed)?;
            image::codecs::ico::IcoEncoder::new(create()?)
                .encode_images(&frames)
                .map_err(failed)?;
            let top = sizes.last().copied().unwrap_or(16);
            n.size_out = (top, top);
            if sizes.len() < 4 || w != h {
                n.warn(format!(
                    "Icon sizes made: {}",
                    sizes
                        .iter()
                        .map(|s| format!("{s}×{s}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        "tiff" => {
            n.single_frame();
            n.no_metadata("tiff");
            let im = if d.bit_depth > 8 {
                n.depth_out = 16;
                if img.color().has_alpha() {
                    DynamicImage::ImageRgba16(img.to_rgba16())
                } else {
                    DynamicImage::ImageRgb16(img.to_rgb16())
                }
            } else {
                eight_bit(img)
            };
            im.write_to(&mut create()?, image::ImageFormat::Tiff)
                .map_err(failed)?;
        }
        "bmp" | "tga" | "qoi" => {
            n.single_frame();
            n.depth8();
            n.no_metadata(target);
            let fmt = match target {
                "bmp" => image::ImageFormat::Bmp,
                "tga" => image::ImageFormat::Tga,
                _ => image::ImageFormat::Qoi,
            };
            eight_bit(img)
                .write_to(&mut create()?, fmt)
                .map_err(failed)?;
        }
        "pnm" => {
            n.single_frame();
            n.depth8();
            n.no_metadata("pnm");
            if has_transparency(img) {
                n.warn("Transparency replaced with white");
            }
            DynamicImage::ImageRgb8(flatten_white(img))
                .write_to(&mut create()?, image::ImageFormat::Pnm)
                .map_err(failed)?;
        }
        "pdf" => {
            let jpeg = d.source_bytes.clone();
            if jpeg.is_none() {
                n.depth8();
                n.no_metadata("pdf");
            }
            n.frames_out = d.frames.len();
            write_pdf(d.frames.len(), PageSize::Image, out, |i| {
                Ok(PdfPage {
                    image: d.frames[i].image.clone(),
                    jpeg: jpeg.clone(),
                })
            })?;
        }
        other => return Err(ToolError::Failed(format!("Can't save images as .{other}"))),
    }
    Ok(n.finish())
}

/// Fit `img` into an `s`×`s` transparent square, keeping its aspect ratio.
fn square(img: &DynamicImage, s: u32) -> RgbaImage {
    let fitted = img
        .resize(s, s, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    let mut canvas = RgbaImage::from_pixel(s, s, Rgba([0, 0, 0, 0]));
    let (x, y) = ((s - fitted.width()) / 2, (s - fitted.height()) / 2);
    image::imageops::overlay(&mut canvas, &fitted, x as i64, y as i64);
    canvas
}

fn count_colours(img: &DynamicImage) -> usize {
    let mut seen = std::collections::HashSet::new();
    for p in img.to_rgba8().pixels() {
        seen.insert(p.0);
        if seen.len() > 256 {
            break;
        }
    }
    seen.len()
}
