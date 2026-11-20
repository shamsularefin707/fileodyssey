use fo_core::office::compress;
use fo_core::pdfopt::Preset;
use image::{DynamicImage, ImageBuffer, Rgb};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn photo(w: u32, h: u32) -> DynamicImage {
    DynamicImage::ImageRgb8(ImageBuffer::from_fn(w, h, |x, y| {
        Rgb([(x / 9) as u8, (y / 6) as u8, ((x * y) / 5000) as u8])
    }))
}

fn encoded(img: &DynamicImage, fmt: image::ImageFormat) -> Vec<u8> {
    let mut v = Vec::new();
    if fmt == image::ImageFormat::Jpeg {
        let rgb = img.to_rgb8();
        jpeg_encoder::Encoder::new(&mut v, 100)
            .encode(
                rgb.as_raw(),
                rgb.width() as u16,
                rgb.height() as u16,
                jpeg_encoder::ColorType::Rgb,
            )
            .unwrap();
    } else {
        img.write_to(&mut std::io::Cursor::new(&mut v), fmt)
            .unwrap();
    }
    v
}

fn package(dir: &TempDir, name: &str, entries: &[(&str, Vec<u8>, bool)]) -> PathBuf {
    let p = dir.path().join(name);
    let mut z = zip::ZipWriter::new(std::fs::File::create(&p).unwrap());
    for (n, data, stored) in entries {
        let opts = if *stored {
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored)
        } else {
            zip::write::SimpleFileOptions::default().compression_level(Some(1))
        };
        z.start_file(*n, opts).unwrap();
        z.write_all(data).unwrap();
    }
    z.finish().unwrap();
    p
}

fn read_all(p: &Path) -> Vec<(String, Vec<u8>, zip::CompressionMethod)> {
    let mut z = zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
    (0..z.len())
        .map(|i| {
            let mut e = z.by_index(i).unwrap();
            let mut b = Vec::new();
            e.read_to_end(&mut b).unwrap();
            (e.name().to_owned(), b, e.compression())
        })
        .collect()
}

fn docx(d: &TempDir) -> PathBuf {
    let xml = b"<w:document>"
        .iter()
        .chain(b"<w:p>Hello</w:p>".repeat(500).iter())
        .chain(b"</w:document>".iter())
        .copied()
        .collect();
    package(
        d,
        "report.docx",
        &[
            ("[Content_Types].xml", b"<Types/>".to_vec(), false),
            ("word/document.xml", xml, true),
            (
                "word/media/image1.png",
                encoded(&photo(800, 600), image::ImageFormat::Png),
                false,
            ),
            (
                "word/media/image2.jpeg",
                encoded(&photo(2600, 1800), image::ImageFormat::Jpeg),
                false,
            ),
        ],
    )
}

#[test]
fn docx_images_recompressed_entries_preserved() {
    let d = TempDir::new().unwrap();
    let src = docx(&d);
    let out = d.path().join("out.docx");
    let r = compress(&src, Preset::Balanced, &out).unwrap();
    let (a, b) = (read_all(&src), read_all(&out));
    assert_eq!(
        a.iter().map(|e| &e.0).collect::<Vec<_>>(),
        b.iter().map(|e| &e.0).collect::<Vec<_>>()
    );
    for (x, y) in a.iter().zip(&b) {
        if x.0.ends_with(".xml") {
            assert_eq!(x.1, y.1, "{}", x.0);
        }
    }
    // The PNG is optimised losslessly: same pixels.
    assert_eq!(
        image::load_from_memory(&a[2].1).unwrap().to_rgb8(),
        image::load_from_memory(&b[2].1).unwrap().to_rgb8()
    );
    assert!(std::fs::metadata(&out).unwrap().len() < std::fs::metadata(&src).unwrap().len());
    assert!(
        r.warnings
            .iter()
            .any(|w| w == "1 images re-encoded as JPEG at quality 80"),
        "{r:?}"
    );
    let inside = r.metrics.iter().find(|m| m.name == "Files inside").unwrap();
    assert_eq!((inside.before, inside.after), (4, 4));
}

#[test]
fn epub_mimetype_first_and_stored() {
    let d = TempDir::new().unwrap();
    let src = package(
        &d,
        "book.epub",
        &[
            ("mimetype", b"application/epub+zip".to_vec(), true),
            ("META-INF/container.xml", b"<container/>".repeat(50), false),
            (
                "OEBPS/cover.png",
                encoded(&photo(500, 700), image::ImageFormat::Png),
                false,
            ),
        ],
    );
    let out = d.path().join("out.epub");
    compress(&src, Preset::Strong, &out).unwrap();
    let raw = std::fs::read(&out).unwrap();
    // The OCF spec requires "mimetype" as the first local file, uncompressed.
    assert_eq!(&raw[30..38], b"mimetype");
    let entries = read_all(&out);
    assert_eq!(entries[0].0, "mimetype");
    assert_eq!(entries[0].2, zip::CompressionMethod::Stored);
    assert_eq!(entries[0].1, b"application/epub+zip");
}

#[test]
fn lossless_never_touches_jpeg() {
    let d = TempDir::new().unwrap();
    let src = docx(&d);
    let out = d.path().join("out.docx");
    let r = compress(&src, Preset::Lossless, &out).unwrap();
    assert_eq!(read_all(&src)[3].1, read_all(&out)[3].1);
    assert!(!r.warnings.iter().any(|w| w.contains("JPEG")), "{r:?}");
}
