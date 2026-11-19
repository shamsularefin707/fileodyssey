use fo_core::image::decode::{decodable, decode};
use fo_core::tool::ToolError;
use fo_types::{FileKind, ImageFormat};
use image::{DynamicImage, ImageBuffer, Rgb, Rgba, RgbaImage};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn img(kind: ImageFormat) -> FileKind {
    FileKind::Image(kind)
}

fn save(dir: &TempDir, name: &str, im: &DynamicImage) -> PathBuf {
    let p = dir.path().join(name);
    im.save(&p).unwrap();
    p
}

/// APP1 Exif segment with a single Orientation tag.
fn exif_orientation(value: u16) -> Vec<u8> {
    let mut tiff = b"II*\0\x08\0\0\0".to_vec();
    tiff.extend_from_slice(&1u16.to_le_bytes()); // 1 entry
    tiff.extend_from_slice(&0x0112u16.to_le_bytes());
    tiff.extend_from_slice(&3u16.to_le_bytes()); // SHORT
    tiff.extend_from_slice(&1u32.to_le_bytes());
    tiff.extend_from_slice(&value.to_le_bytes());
    tiff.extend_from_slice(&[0, 0]);
    tiff.extend_from_slice(&0u32.to_le_bytes());
    let mut seg = vec![0xFF, 0xE1];
    let body: Vec<u8> = [b"Exif\0\0".as_slice(), &tiff].concat();
    seg.extend_from_slice(&((body.len() + 2) as u16).to_be_bytes());
    seg.extend_from_slice(&body);
    seg
}

#[test]
fn png_roundtrip_keeps_pixels_and_alpha() {
    let d = TempDir::new().unwrap();
    let mut im = RgbaImage::new(3, 2);
    im.put_pixel(1, 1, Rgba([10, 20, 30, 40]));
    let p = save(&d, "a.png", &DynamicImage::ImageRgba8(im));
    let dec = decode(&p, &img(ImageFormat::Png)).unwrap();
    assert_eq!(dec.frames.len(), 1);
    assert!(dec.has_alpha);
    assert_eq!(
        dec.frames[0].image.to_rgba8().get_pixel(1, 1),
        &Rgba([10, 20, 30, 40])
    );
}

#[test]
fn jpeg_with_orientation_6_is_rotated() {
    let d = TempDir::new().unwrap();
    let p = save(
        &d,
        "raw.jpg",
        &DynamicImage::ImageRgb8(ImageBuffer::from_pixel(4, 2, Rgb([200, 100, 50]))),
    );
    let bytes = std::fs::read(&p).unwrap();
    let mut out = bytes[..2].to_vec();
    out.extend(exif_orientation(6));
    out.extend_from_slice(&bytes[2..]);
    let q = d.path().join("rot.jpg");
    std::fs::write(&q, out).unwrap();
    let dec = decode(&q, &img(ImageFormat::Jpeg)).unwrap();
    assert_eq!(
        (dec.frames[0].image.width(), dec.frames[0].image.height()),
        (2, 4)
    );
}

#[test]
fn gif_all_frames_with_delays() {
    use image::codecs::gif::GifEncoder;
    use image::{Delay, Frame};
    let d = TempDir::new().unwrap();
    let p = d.path().join("anim.gif");
    let mut enc = GifEncoder::new(std::fs::File::create(&p).unwrap());
    let frames = (0..3u8).map(|i| {
        Frame::from_parts(
            RgbaImage::from_pixel(4, 4, Rgba([i * 80, 0, 0, 255])),
            0,
            0,
            Delay::from_numer_denom_ms(100, 1),
        )
    });
    enc.encode_frames(frames).unwrap();
    drop(enc);
    let dec = decode(&p, &img(ImageFormat::Gif)).unwrap();
    assert_eq!(dec.frames.len(), 3);
    assert!(dec.frames.iter().all(|f| f.delay_ms == 100));
}

#[test]
fn sixteen_bit_png_reports_bit_depth_16() {
    let d = TempDir::new().unwrap();
    let im: ImageBuffer<Rgb<u16>, Vec<u16>> = ImageBuffer::from_pixel(2, 2, Rgb([60000, 1, 2]));
    let p = save(&d, "deep.png", &DynamicImage::ImageRgb16(im));
    assert_eq!(decode(&p, &img(ImageFormat::Png)).unwrap().bit_depth, 16);
}

#[test]
fn svg_rasterises_at_intrinsic_size() {
    let d = TempDir::new().unwrap();
    let p = d.path().join("s.svg");
    std::fs::write(&p, r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20" fill="#5F8794"/></svg>"##).unwrap();
    let dec = decode(&p, &img(ImageFormat::Svg)).unwrap();
    assert_eq!(
        (dec.frames[0].image.width(), dec.frames[0].image.height()),
        (40, 20)
    );
}

fn png_header_only(path: &Path, w: u32, h: u32) {
    let mut ihdr = b"IHDR".to_vec();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let crc = crc32fast::hash(&ihdr);
    let mut f = b"\x89PNG\r\n\x1a\n".to_vec();
    f.extend_from_slice(&13u32.to_be_bytes());
    f.extend_from_slice(&ihdr);
    f.extend_from_slice(&crc.to_be_bytes());
    // A real file always has image data; an empty zlib stream is enough to
    // get past the header without allocating anything.
    for (ty, data) in [
        (
            &b"IDAT"[..],
            &b"\x78\x01\x01\x00\x00\xff\xff\x00\x00\x00\x01"[..],
        ),
        (&b"IEND"[..], &b""[..]),
    ] {
        f.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let body = [ty, data].concat();
        f.extend_from_slice(&body);
        f.extend_from_slice(&crc32fast::hash(&body).to_be_bytes());
    }
    std::fs::write(path, f).unwrap();
}

#[test]
fn oversized_dimensions_refused() {
    let d = TempDir::new().unwrap();
    let p = d.path().join("bomb.png");
    png_header_only(&p, 30000, 30000);
    let err = decode(&p, &img(ImageFormat::Png)).err().unwrap();
    assert_eq!(
        err,
        ToolError::Failed("This image is too large to open safely (900 megapixels)".into())
    );
}

#[test]
fn truncated_png_fails_clearly() {
    let d = TempDir::new().unwrap();
    let p = save(
        &d,
        "ok.png",
        &DynamicImage::ImageRgb8(ImageBuffer::from_pixel(64, 64, Rgb([1, 2, 3]))),
    );
    let bytes = std::fs::read(&p).unwrap();
    std::fs::write(&p, &bytes[..bytes.len() / 2]).unwrap();
    match decode(&p, &img(ImageFormat::Png)) {
        Err(ToolError::Failed(m)) => assert!(m.starts_with("Couldn't read this image"), "{m}"),
        Err(other) => panic!("{other:?}"),
        Ok(_) => panic!("decoded a truncated png"),
    }
}

#[test]
fn avif_not_decodable() {
    assert!(!decodable(&img(ImageFormat::Avif)));
    assert!(!decodable(&img(ImageFormat::Heic)));
    assert!(!decodable(&FileKind::Pdf));
    assert!(decodable(&img(ImageFormat::Png)));
}
