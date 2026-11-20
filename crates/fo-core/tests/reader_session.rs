use fo_core::reader::pdf::find_pdfium_dirs;
use fo_core::reader::session::Sessions;
use fo_types::ReaderBody;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn sessions() -> Sessions {
    Sessions::new(find_pdfium_dirs(&[]))
}

fn one_page_pdf(dir: &Path) -> PathBuf {
    use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str};
    let mut pdf = Pdf::new();
    pdf.catalog(Ref::new(1)).pages(Ref::new(2));
    pdf.pages(Ref::new(2)).kids([Ref::new(3)]).count(1);
    let mut p = pdf.page(Ref::new(3));
    p.media_box(Rect::new(0.0, 0.0, 200.0, 100.0))
        .parent(Ref::new(2))
        .contents(Ref::new(5));
    p.resources().fonts().pair(Name(b"F1"), Ref::new(4));
    p.finish();
    pdf.type1_font(Ref::new(4)).base_font(Name(b"Helvetica"));
    let mut c = Content::new();
    c.begin_text()
        .set_font(Name(b"F1"), 12.0)
        .next_line(10.0, 50.0);
    c.show(Str(b"Quiet river"));
    c.end_text();
    pdf.stream(Ref::new(5), &c.finish());
    let path = dir.join("one.pdf");
    std::fs::write(&path, pdf.finish()).unwrap();
    path
}

#[test]
fn pdf_doc_serves_pages_and_rejects_bad_requests() {
    let d = TempDir::new().unwrap();
    let s = sessions();
    let v = s.open(&one_page_pdf(d.path())).unwrap();
    assert_eq!(v.name, "one.pdf");
    let ReaderBody::Pdf(info) = &v.body else {
        panic!("{:?}", v.body)
    };
    assert_eq!(info.pages.len(), 1);
    let (png, mime) = s.serve(&format!("/{}/page/0?w=100", v.doc)).unwrap();
    assert_eq!(mime, "image/png");
    assert_eq!(image::load_from_memory(&png).unwrap().width(), 100);
    // Encoded the way the webview's convertFileSrc does it.
    assert!(s.serve(&format!("/{}%2Fpage%2F0%3Fw%3D50", v.doc)).is_ok());
    let (png, _) = s.serve(&format!("/{}/page/0?w=99999", v.doc)).unwrap();
    assert_eq!(image::load_from_memory(&png).unwrap().width(), 4096);
    assert_eq!(
        s.serve(&format!("/{}/page/7?w=100", v.doc)).unwrap_err().0,
        404
    );
    assert_eq!(s.serve("/999/page/0?w=100").unwrap_err().0, 404);
    assert_eq!(s.serve(&format!("/{}/nonsense", v.doc)).unwrap_err().0, 400);
    assert!(s.search(v.doc, "river").unwrap().len() == 1);
    assert!(s
        .text_spans(v.doc, 0)
        .unwrap()
        .iter()
        .any(|t| t.text.contains("Quiet")));
    s.close(v.doc);
    assert_eq!(
        s.serve(&format!("/{}/page/0?w=100", v.doc)).unwrap_err().0,
        404
    );
}

#[test]
fn markdown_doc_serves_only_images_from_its_folder() {
    let d = TempDir::new().unwrap();
    let docs = d.path().join("docs");
    std::fs::create_dir_all(docs.join("pics")).unwrap();
    std::fs::write(d.path().join("secret.png"), b"\x89PNG secret").unwrap();
    std::fs::write(docs.join("pics/p.png"), b"\x89PNG ok").unwrap();
    std::fs::write(docs.join("notes.txt"), b"private").unwrap();
    let md = docs.join("a.md");
    std::fs::write(&md, "# Notes\n![p](pics/p.png)").unwrap();
    let s = sessions();
    let v = s.open(&md).unwrap();
    let ReaderBody::Page(html) = &v.body else {
        panic!()
    };
    assert!(html.contains(r#"data-fo-src="pics/p.png""#));
    assert_eq!(
        s.serve(&format!("/{}/res/pics/p.png", v.doc)).unwrap().0,
        b"\x89PNG ok"
    );
    assert_eq!(
        s.serve(&format!("/{}/res/../secret.png", v.doc))
            .unwrap_err()
            .0,
        403
    );
    assert_eq!(
        s.serve(&format!("/{}/res/%2E%2E/secret.png", v.doc))
            .unwrap_err()
            .0,
        403
    );
    assert_eq!(
        s.serve(&format!("/{}/res/notes.txt", v.doc)).unwrap_err().0,
        403,
        "images only"
    );
}

#[test]
fn image_and_text_docs() {
    let d = TempDir::new().unwrap();
    let s = sessions();
    let png = d.path().join("i.png");
    image::RgbImage::from_pixel(6, 3, image::Rgb([9, 9, 9]))
        .save(&png)
        .unwrap();
    let v = s.open(&png).unwrap();
    assert!(matches!(&v.body, ReaderBody::Image(i) if i.width == 6));
    assert_eq!(
        s.serve(&format!("/{}/image", v.doc)).unwrap().1,
        "image/png"
    );
    let csv = d.path().join("t.csv");
    std::fs::write(&csv, "a,b\n1,2\n").unwrap();
    assert!(matches!(s.open(&csv).unwrap().body, ReaderBody::Page(h) if h.contains("a,b")));
}

#[test]
fn unreadable_kinds_say_why() {
    let d = TempDir::new().unwrap();
    let s = sessions();
    let docx = d.path().join("w.docx");
    let mut z = zip::ZipWriter::new(std::fs::File::create(&docx).unwrap());
    z.start_file(
        "[Content_Types].xml",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    std::io::Write::write_all(&mut z, b"<Types/>").unwrap();
    z.start_file(
        "word/document.xml",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    std::io::Write::write_all(&mut z, b"<w:document/>").unwrap();
    z.finish().unwrap();
    let e = s.open(&docx).unwrap_err();
    assert!(e.contains("Word"), "{e}");
    let bin = d.path().join("x.bin");
    std::fs::write(&bin, [0u8, 1, 2, 3, 0xFF, 0xFE, 7, 0]).unwrap();
    assert!(s.open(&bin).unwrap_err().contains("can't read"));
}
