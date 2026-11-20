use fo_core::image::pdf::{write_pdf, PageSize, PdfPage};
use fo_core::pdfopt::{compress, Preset};
use fo_types::Verdict;
use image::{DynamicImage, ImageBuffer, Rgb, Rgba};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn noise(w: u32, h: u32) -> DynamicImage {
    let mut s = 7u32;
    DynamicImage::ImageRgb8(ImageBuffer::from_fn(w, h, |x, y| {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        // Smooth gradient plus a little noise: photo-like, so JPEG wins.
        Rgb([
            (x / 12) as u8 ^ (s as u8 & 7),
            (y / 8) as u8,
            ((x + y) / 20) as u8,
        ])
    }))
}

fn image_pdf(dir: &TempDir, name: &str, img: DynamicImage) -> PathBuf {
    let p = dir.path().join(name);
    write_pdf(1, PageSize::Image, &p, |_| {
        Ok(PdfPage {
            image: img.clone(),
            jpeg: None,
        })
    })
    .unwrap();
    p
}

/// A one-page PDF whose content stream is uncompressed and full of text.
fn text_pdf(dir: &TempDir) -> PathBuf {
    use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str};
    let mut pdf = Pdf::new();
    let (cat, tree, page, font, content) = (
        Ref::new(1),
        Ref::new(2),
        Ref::new(3),
        Ref::new(4),
        Ref::new(5),
    );
    pdf.catalog(cat).pages(tree);
    pdf.pages(tree).kids([page]).count(1);
    let mut p = pdf.page(page);
    p.media_box(Rect::new(0.0, 0.0, 595.0, 842.0))
        .parent(tree)
        .contents(content);
    p.resources().fonts().pair(Name(b"F1"), font);
    p.finish();
    pdf.type1_font(font).base_font(Name(b"Helvetica"));
    let mut c = Content::new();
    for i in 0..400 {
        c.begin_text()
            .set_font(Name(b"F1"), 9.0)
            .next_line(20.0, 820.0 - (i % 90) as f32 * 9.0);
        c.show(Str(
            b"Hello FileOdyssey, this line repeats to make the stream compressible.",
        ));
        c.end_text();
    }
    pdf.stream(content, &c.finish());
    let path = dir.path().join("text.pdf");
    std::fs::write(&path, pdf.finish()).unwrap();
    path
}

/// lopdf can write files it reads back but other readers reject; when qpdf is
/// installed, every output must also pass `qpdf --check`.
fn page_count(p: &Path) -> usize {
    if let Ok(out) = std::process::Command::new("qpdf")
        .arg("--check")
        .arg(p)
        .output()
    {
        assert!(
            out.status.success(),
            "qpdf rejects {p:?}: {}",
            String::from_utf8_lossy(&out.stdout)
        );
    }
    lopdf::Document::load(p).unwrap().get_pages().len()
}

fn all_content(p: &Path) -> String {
    let doc = lopdf::Document::load(p).unwrap();
    doc.get_pages()
        .values()
        .map(|id| String::from_utf8_lossy(&doc.get_page_content(*id)).into_owned())
        .collect()
}

#[test]
fn balanced_shrinks_big_flate_image_keeps_pages() {
    let d = TempDir::new().unwrap();
    let src = image_pdf(&d, "big.pdf", noise(3000, 2000));
    let out = d.path().join("out.pdf");
    let r = compress(&src, Preset::Balanced, &out).unwrap();
    let (before, after) = (
        std::fs::metadata(&src).unwrap().len(),
        std::fs::metadata(&out).unwrap().len(),
    );
    assert!(after * 2 < before, "{before} → {after}");
    assert_eq!(page_count(&out), 1);
    assert!(
        r.warnings
            .iter()
            .any(|w| w == "1 images re-encoded as JPEG at quality 80"),
        "{r:?}"
    );
    assert!(
        r.warnings
            .iter()
            .any(|w| w == "1 images scaled down to at most 2000 px"),
        "{r:?}"
    );
}

#[test]
fn lossless_keeps_images_and_text_exact() {
    let d = TempDir::new().unwrap();
    let src = text_pdf(&d);
    let out = d.path().join("out.pdf");
    let r = compress(&src, Preset::Lossless, &out).unwrap();
    assert!(std::fs::metadata(&out).unwrap().len() < std::fs::metadata(&src).unwrap().len());
    assert!(all_content(&out).contains("Hello FileOdyssey"));
    assert_eq!(page_count(&out), 1);
    assert_eq!(r.verdict(), Verdict::Exact, "{r:?}");
}

#[test]
fn masks_are_not_jpeg_encoded() {
    let d = TempDir::new().unwrap();
    let img = DynamicImage::ImageRgba8(ImageBuffer::from_fn(2400, 1600, |x, y| {
        Rgba([
            (x / 10) as u8,
            (y / 7) as u8,
            60,
            if x < 1200 { 255 } else { 90 },
        ])
    }));
    let src = image_pdf(&d, "alpha.pdf", img);
    let out = d.path().join("out.pdf");
    compress(&src, Preset::Strong, &out).unwrap();
    assert_eq!(page_count(&out), 1);
    let doc = lopdf::Document::load(&out).unwrap();
    let mut saw_mask = false;
    for obj in doc.objects.values() {
        if let Ok(st) = obj.as_stream() {
            if let Ok(mask) = st.dict.get(b"SMask").and_then(|m| m.as_reference()) {
                let m = doc.get_object(mask).unwrap().as_stream().unwrap();
                let filter = m.dict.get(b"Filter").unwrap().as_name().unwrap();
                assert_eq!(filter, b"FlateDecode");
                saw_mask = true;
            }
        }
    }
    assert!(saw_mask);
}

#[test]
fn not_smaller_saves_original() {
    let d = TempDir::new().unwrap();
    let src = d.path().join("tiny.pdf");
    // Already compact: compress once, then compress the result again.
    compress(&text_pdf(&d), Preset::Lossless, &src).unwrap();
    let out = d.path().join("again.pdf");
    let r = compress(&src, Preset::Lossless, &out).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), std::fs::read(&src).unwrap());
    assert!(
        r.warnings
            .iter()
            .any(|w| w == "Already compact; saved unchanged"),
        "{r:?}"
    );
}

#[test]
fn not_a_pdf_fails_clearly() {
    let d = TempDir::new().unwrap();
    let p = d.path().join("x.pdf");
    std::fs::write(&p, b"%PDF-1.4 garbage").unwrap();
    match compress(&p, Preset::Balanced, &d.path().join("o.pdf")) {
        Err(fo_core::tool::ToolError::Failed(m)) => {
            assert!(m.starts_with("Couldn't read this PDF"), "{m}")
        }
        other => panic!("{:?}", other.map(|r| r.warnings)),
    }
}
