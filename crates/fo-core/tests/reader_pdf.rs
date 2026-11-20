use fo_core::reader::pdf::{find_pdfium_dirs, PdfEngine};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tempfile::TempDir;

fn engine() -> &'static PdfEngine {
    static E: OnceLock<PdfEngine> = OnceLock::new();
    E.get_or_init(|| {
        PdfEngine::start(&find_pdfium_dirs(&[])).expect("PDFium (run scripts/fetch-pdfium.sh)")
    })
}

/// Two pages: A4 portrait with "Hello reader", then A4 landscape with "Second page".
fn two_pages(dir: &Path) -> PathBuf {
    use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str};
    let mut pdf = Pdf::new();
    let (cat, tree, font) = (Ref::new(1), Ref::new(2), Ref::new(3));
    let pages = [(Ref::new(4), Ref::new(5)), (Ref::new(6), Ref::new(7))];
    pdf.catalog(cat).pages(tree);
    pdf.pages(tree).kids(pages.iter().map(|p| p.0)).count(2);
    pdf.type1_font(font).base_font(Name(b"Helvetica"));
    for (i, (page, content)) in pages.iter().enumerate() {
        let (w, h, text): (f32, f32, &[u8]) = if i == 0 {
            (595.0, 842.0, b"Hello reader")
        } else {
            (842.0, 595.0, b"Second page")
        };
        let mut p = pdf.page(*page);
        p.media_box(Rect::new(0.0, 0.0, w, h))
            .parent(tree)
            .contents(*content);
        p.resources().fonts().pair(Name(b"F1"), font);
        p.finish();
        let mut c = Content::new();
        c.begin_text()
            .set_font(Name(b"F1"), 24.0)
            .next_line(72.0, h - 100.0);
        c.show(Str(text));
        c.end_text();
        pdf.stream(*content, &c.finish());
    }
    let path = dir.join("two.pdf");
    std::fs::write(&path, pdf.finish()).unwrap();
    path
}

#[test]
fn open_reports_pages_and_sizes() {
    let d = TempDir::new().unwrap();
    let (_, info) = engine().open(&two_pages(d.path())).unwrap();
    assert_eq!(info.pages.len(), 2);
    assert!((info.pages[0].w - 595.0).abs() < 1.0 && (info.pages[0].h - 842.0).abs() < 1.0);
    assert!(
        info.pages[1].w > info.pages[1].h,
        "second page is landscape"
    );
}

#[test]
fn render_returns_png_of_requested_width() {
    let d = TempDir::new().unwrap();
    let (id, _) = engine().open(&two_pages(d.path())).unwrap();
    let png = engine().render(id, 0, 300).unwrap();
    let img = image::load_from_memory(&png).unwrap().to_rgb8();
    assert_eq!(img.width(), 300);
    assert!((img.height() as i32 - 424).abs() <= 1);
    let first = img.get_pixel(0, 0);
    assert!(img.pixels().any(|p| p != first), "page has visible text");
}

#[test]
fn text_spans_and_search() {
    let d = TempDir::new().unwrap();
    let (id, _) = engine().open(&two_pages(d.path())).unwrap();
    let spans = engine().text_spans(id, 0).unwrap();
    let all: String = spans.iter().map(|s| s.text.as_str()).collect();
    assert!(all.contains("Hello reader"), "{all:?}");
    for s in &spans {
        assert!(
            s.rect.x >= 0.0
                && s.rect.x + s.rect.w <= 1.01
                && s.rect.y >= 0.0
                && s.rect.y + s.rect.h <= 1.01
        );
    }
    // Near the top of the page (y=100pt of 842).
    assert!(spans[0].rect.y < 0.2);
    let hits = engine().search(id, "second").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].page, 1);
    assert!(!hits[0].rects.is_empty());
    assert!(engine().search(id, "").unwrap().is_empty());
}

#[test]
fn page_out_of_range_is_an_error() {
    let d = TempDir::new().unwrap();
    let (id, _) = engine().open(&two_pages(d.path())).unwrap();
    assert!(engine().render(id, 9, 300).is_err());
    engine().close(id);
    assert!(engine().render(id, 0, 300).is_err(), "closed doc");
}

#[test]
fn broken_and_encrypted_pdfs_fail_clearly_and_engine_survives() {
    let d = TempDir::new().unwrap();
    let junk = d.path().join("junk.pdf");
    std::fs::write(&junk, b"%PDF-1.7\nthis is not really a pdf").unwrap();
    let e = engine().open(&junk).unwrap_err();
    assert!(e.contains("damaged") || e.contains("isn't a PDF"), "{e}");
    let src = two_pages(d.path());
    let locked = d.path().join("locked.pdf");
    let status = std::process::Command::new("qpdf")
        .args(["--encrypt", "secret", "owner", "256", "--"])
        .arg(&src)
        .arg(&locked)
        .status();
    if matches!(status, Ok(s) if s.success()) {
        let e = engine().open(&locked).unwrap_err();
        assert!(e.contains("password"), "{e}");
    } else if std::env::var_os("CI").is_some() {
        panic!("qpdf is required in CI");
    }
    assert!(engine().open(&src).is_ok());
}
