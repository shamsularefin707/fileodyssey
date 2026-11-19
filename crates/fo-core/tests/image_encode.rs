use fo_core::detect::detect_kind;
use fo_core::image::decode::decode;
use fo_core::image::encode::{encode, EncodeOpts, WRITE_TARGETS};
use fo_core::tool::ToolError;
use fo_types::{FileKind, ImageFormat, Verdict};
use image::{DynamicImage, ImageBuffer, Rgb, Rgba, RgbaImage};
use std::path::PathBuf;
use tempfile::TempDir;

fn save(dir: &TempDir, name: &str, im: &DynamicImage) -> PathBuf {
    let p = dir.path().join(name);
    im.save(&p).unwrap();
    p
}

fn decoded(path: &PathBuf) -> fo_core::image::decode::Decoded {
    decode(path, &detect_kind(path).unwrap()).unwrap()
}

fn gradient(w: u32, h: u32) -> DynamicImage {
    DynamicImage::ImageRgb8(ImageBuffer::from_fn(w, h, |x, y| {
        Rgb([(x * 7) as u8, (y * 5) as u8, 90])
    }))
}

fn has_warning(r: &fo_types::FidelityReport, needle: &str) -> bool {
    r.warnings.iter().any(|w| w.contains(needle))
}

fn animated_gif(dir: &TempDir) -> PathBuf {
    use image::codecs::gif::GifEncoder;
    use image::{Delay, Frame};
    let p = dir.path().join("anim.gif");
    let mut enc = GifEncoder::new(std::fs::File::create(&p).unwrap());
    let frames = (0..3u8).map(|i| {
        Frame::from_parts(
            RgbaImage::from_pixel(6, 4, Rgba([i * 80, 40, 0, 255])),
            0,
            0,
            Delay::from_numer_denom_ms(120, 1),
        )
    });
    enc.encode_frames(frames).unwrap();
    p
}

#[test]
fn png_to_png_is_exact() {
    let d = TempDir::new().unwrap();
    let src = save(&d, "a.png", &gradient(20, 10));
    let out = d.path().join("out.png");
    let r = encode(&decoded(&src), "png", &EncodeOpts::default(), &out).unwrap();
    assert_eq!(r.verdict(), Verdict::Exact, "{r:?}");
    assert_eq!(
        image::open(&out).unwrap().to_rgb8(),
        gradient(20, 10).to_rgb8()
    );
}

#[test]
fn transparent_png_to_jpg_flattens_to_white() {
    let d = TempDir::new().unwrap();
    let mut im = RgbaImage::from_pixel(16, 16, Rgba([0, 0, 0, 0]));
    im.put_pixel(8, 8, Rgba([255, 0, 0, 255]));
    let src = save(&d, "t.png", &DynamicImage::ImageRgba8(im));
    let out = d.path().join("t.jpg");
    let r = encode(&decoded(&src), "jpg", &EncodeOpts::default(), &out).unwrap();
    assert!(has_warning(&r, "Transparency replaced with white"), "{r:?}");
    let px = image::open(&out).unwrap().to_rgb8();
    assert!(
        px.get_pixel(0, 0).0.iter().all(|&c| c > 240),
        "corner {:?}",
        px.get_pixel(0, 0)
    );
}

#[test]
fn gif_to_png_keeps_first_frame_and_warns() {
    let d = TempDir::new().unwrap();
    let src = animated_gif(&d);
    let out = d.path().join("f.png");
    let r = encode(&decoded(&src), "png", &EncodeOpts::default(), &out).unwrap();
    assert!(
        has_warning(&r, "Animation dropped: kept the first of 3 frames"),
        "{r:?}"
    );
    let frames = r.metrics.iter().find(|m| m.name == "Frames").unwrap();
    assert_eq!((frames.before, frames.after), (3, 1));
}

#[test]
fn gif_to_webp_stays_animated() {
    let d = TempDir::new().unwrap();
    let src = animated_gif(&d);
    let out = d.path().join("a.webp");
    let r = encode(&decoded(&src), "webp", &EncodeOpts::default(), &out).unwrap();
    assert!(!has_warning(&r, "Animation dropped"), "{r:?}");
    assert_eq!(
        decode(&out, &FileKind::Image(ImageFormat::Webp))
            .unwrap()
            .frames
            .len(),
        3
    );
}

#[test]
fn gif_to_gif_stays_animated() {
    let d = TempDir::new().unwrap();
    let src = animated_gif(&d);
    let out = d.path().join("b.gif");
    encode(&decoded(&src), "gif", &EncodeOpts::default(), &out).unwrap();
    let back = decode(&out, &FileKind::Image(ImageFormat::Gif)).unwrap();
    assert_eq!(back.frames.len(), 3);
    assert_eq!(back.frames[1].delay_ms, 120);
}

fn deep(d: &TempDir) -> PathBuf {
    let im: ImageBuffer<Rgb<u16>, Vec<u16>> =
        ImageBuffer::from_fn(4, 4, |x, _| Rgb([x as u16 * 9000, 300, 65535]));
    save(d, "deep.png", &DynamicImage::ImageRgb16(im))
}

#[test]
fn sixteen_bit_png_to_tiff_keeps_16() {
    let d = TempDir::new().unwrap();
    let out = d.path().join("deep.tiff");
    let r = encode(&decoded(&deep(&d)), "tiff", &EncodeOpts::default(), &out).unwrap();
    assert!(!has_warning(&r, "Colour depth"), "{r:?}");
    assert_eq!(decoded(&out).bit_depth, 16);
}

#[test]
fn sixteen_bit_png_to_jpg_warns_depth() {
    let d = TempDir::new().unwrap();
    let out = d.path().join("deep.jpg");
    let r = encode(&decoded(&deep(&d)), "jpg", &EncodeOpts::default(), &out).unwrap();
    assert!(
        has_warning(&r, "Colour depth reduced from 16 to 8 bits"),
        "{r:?}"
    );
}

#[test]
fn jpeg_exif_kept_in_jpg_output() {
    let d = TempDir::new().unwrap();
    let p = save(&d, "raw.jpg", &gradient(8, 8));
    let bytes = std::fs::read(&p).unwrap();
    // APP1 Exif with Orientation=1 (no rotation) so EXIF survives decode.
    let tiff: Vec<u8> = [
        b"II*\0\x08\0\0\0".as_slice(),
        &[1, 0, 0x12, 0x01, 3, 0, 1, 0, 0, 0, 1, 0, 0, 0],
        &[0, 0, 0, 0],
    ]
    .concat();
    let body = [b"Exif\0\0".as_slice(), &tiff].concat();
    let mut out = bytes[..2].to_vec();
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&((body.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(&body);
    out.extend_from_slice(&bytes[2..]);
    let src = d.path().join("exif.jpg");
    std::fs::write(&src, out).unwrap();
    assert!(decoded(&src).exif.is_some());
    let dest = d.path().join("again.jpg");
    let r = encode(&decoded(&src), "jpg", &EncodeOpts::default(), &dest).unwrap();
    assert!(!has_warning(&r, "Metadata"), "{r:?}");
    assert!(decoded(&dest).exif.is_some());
}

#[test]
fn lossy_targets_say_so() {
    let d = TempDir::new().unwrap();
    let src = save(&d, "a.png", &gradient(10, 10));
    let r = encode(
        &decoded(&src),
        "avif",
        &EncodeOpts {
            quality: 70,
            ..Default::default()
        },
        &d.path().join("a.avif"),
    )
    .unwrap();
    assert!(
        has_warning(&r, "Saved with lossy compression at quality 70"),
        "{r:?}"
    );
}

#[test]
fn every_target_produces_a_decodable_file() {
    let d = TempDir::new().unwrap();
    let src = save(&d, "s.png", &gradient(33, 21));
    let dec = decoded(&src);
    for t in WRITE_TARGETS
        .iter()
        .filter(|t| !matches!(**t, "pdf" | "avif"))
    {
        let out = d.path().join(format!("o.{t}"));
        encode(&dec, t, &EncodeOpts::default(), &out).unwrap_or_else(|e| panic!("{t}: {e:?}"));
        let back = decoded(&out);
        let (w, h) = (back.first().width(), back.first().height());
        if *t == "ico" {
            assert!(w <= 33 && h <= 33, "{t}");
        } else {
            assert_eq!((w, h), (33, 21), "{t}");
        }
    }
    // AVIF can't be read back until M8; check the container instead.
    let avif = d.path().join("o.avif");
    encode(&dec, "avif", &EncodeOpts::default(), &avif).unwrap();
    assert_eq!(
        detect_kind(&avif).unwrap(),
        FileKind::Image(ImageFormat::Avif)
    );
}

#[test]
fn pdf_output_starts_with_pdf_header_and_has_n_pages() {
    let d = TempDir::new().unwrap();
    let src = animated_gif(&d);
    let out = d.path().join("a.pdf");
    encode(&decoded(&src), "pdf", &EncodeOpts::default(), &out).unwrap();
    let bytes = std::fs::read(&out).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    let text = String::from_utf8_lossy(&bytes);
    assert_eq!(
        text.matches("/Type /Page\n").count()
            + text.matches("/Type /Page ").count()
            + text.matches("/Type /Page>").count(),
        3,
        "pages"
    );
}

#[test]
fn jpeg_embedded_unchanged_in_pdf() {
    let d = TempDir::new().unwrap();
    let src = save(&d, "photo.jpg", &gradient(40, 30));
    let jpeg = std::fs::read(&src).unwrap();
    let out = d.path().join("photo.pdf");
    let r = encode(&decoded(&src), "pdf", &EncodeOpts::default(), &out).unwrap();
    let pdf = std::fs::read(&out).unwrap();
    assert!(
        pdf.windows(jpeg.len()).any(|w| w == jpeg.as_slice()),
        "JPEG bytes not embedded as-is"
    );
    assert_eq!(r.verdict(), Verdict::Exact, "{r:?}");
}

#[test]
fn unknown_target_fails_clearly() {
    let d = TempDir::new().unwrap();
    let src = save(&d, "a.png", &gradient(4, 4));
    let err = encode(
        &decoded(&src),
        "xyz",
        &EncodeOpts::default(),
        &d.path().join("a.xyz"),
    )
    .err()
    .unwrap();
    assert_eq!(err, ToolError::Failed("Can't save images as .xyz".into()));
}
