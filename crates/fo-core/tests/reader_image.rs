use fo_core::reader::image_view::{display_bytes, image_info};
use fo_types::{FileKind, ImageFormat};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// A 40×20 JPEG with Make, Model, DateTimeOriginal, FNumber and ExposureTime.
fn jpeg_with_exif(dir: &Path) -> PathBuf {
    use exif::experimental::Writer;
    use exif::{Field, In, Rational, Tag, Value};
    let img = image::RgbImage::from_pixel(40, 20, image::Rgb([200, 120, 40]));
    let mut jpg = Vec::new();
    image::DynamicImage::ImageRgb8(img)
        .write_to(
            &mut std::io::Cursor::new(&mut jpg),
            image::ImageFormat::Jpeg,
        )
        .unwrap();
    let fields = [
        Field {
            tag: Tag::Make,
            ifd_num: In::PRIMARY,
            value: Value::Ascii(vec![b"Fujifilm".to_vec()]),
        },
        Field {
            tag: Tag::Model,
            ifd_num: In::PRIMARY,
            value: Value::Ascii(vec![b"X-T30".to_vec()]),
        },
        Field {
            tag: Tag::DateTimeOriginal,
            ifd_num: In::PRIMARY,
            value: Value::Ascii(vec![b"2026:03:14 07:05:00".to_vec()]),
        },
        Field {
            tag: Tag::FNumber,
            ifd_num: In::PRIMARY,
            value: Value::Rational(vec![Rational { num: 28, denom: 10 }]),
        },
        Field {
            tag: Tag::ExposureTime,
            ifd_num: In::PRIMARY,
            value: Value::Rational(vec![Rational { num: 1, denom: 250 }]),
        },
    ];
    let mut w = Writer::new();
    for f in &fields {
        w.push_field(f);
    }
    let mut tiff = std::io::Cursor::new(Vec::new());
    w.write(&mut tiff, false).unwrap();
    let body = [b"Exif\0\0".as_slice(), tiff.get_ref()].concat();
    let mut out = jpg[..2].to_vec();
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&((body.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(&body);
    out.extend_from_slice(&jpg[2..]);
    let p = dir.join("photo.jpg");
    std::fs::write(&p, out).unwrap();
    p
}

#[test]
fn jpeg_exif_is_listed_in_plain_words() {
    let d = TempDir::new().unwrap();
    let info = image_info(
        &jpeg_with_exif(d.path()),
        FileKind::Image(ImageFormat::Jpeg),
    )
    .unwrap();
    assert_eq!((info.width, info.height), (40, 20));
    let get = |k: &str| {
        info.exif
            .iter()
            .find(|(l, _)| l == k)
            .map(|(_, v)| v.clone())
    };
    assert_eq!(get("Camera").as_deref(), Some("Fujifilm X-T30"));
    assert_eq!(get("Taken").as_deref(), Some("2026-03-14 07:05:00"));
    assert_eq!(get("Aperture").as_deref(), Some("f/2.8"));
    assert_eq!(get("Exposure").as_deref(), Some("1/250 s"));
    assert_eq!(info.exif[0].0, "Camera", "most useful first");
}

#[test]
fn browser_formats_pass_through_others_become_png() {
    let d = TempDir::new().unwrap();
    let png = d.path().join("a.png");
    image::RgbImage::from_pixel(8, 4, image::Rgb([1, 2, 3]))
        .save(&png)
        .unwrap();
    let (bytes, mime) = display_bytes(&png, FileKind::Image(ImageFormat::Png)).unwrap();
    assert_eq!((bytes, mime), (std::fs::read(&png).unwrap(), "image/png"));

    let tif = d.path().join("a.tif");
    image::RgbImage::from_pixel(8, 4, image::Rgb([1, 2, 3]))
        .save(&tif)
        .unwrap();
    let (bytes, mime) = display_bytes(&tif, FileKind::Image(ImageFormat::Tiff)).unwrap();
    assert_eq!(mime, "image/png");
    assert_eq!(image::load_from_memory(&bytes).unwrap().width(), 8);
    let info = image_info(&tif, FileKind::Image(ImageFormat::Tiff)).unwrap();
    assert_eq!((info.width, info.height), (8, 4));
    assert!(info.exif.is_empty());
}
