use fo_core::detect::{detect_file, detect_kind};
use fo_types::{ArchiveFormat, FileKind, ImageFormat};
use std::io::Write;
use std::path::PathBuf;
use tempfile::TempDir;

const PNG: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R',
];

fn write(dir: &TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.path().join(name);
    std::fs::write(&p, bytes).unwrap();
    p
}

/// Build a zip with the given (name, contents) entries; `mimetype` stored first like EPUB/ODF.
fn zip_with(dir: &TempDir, name: &str, entries: &[(&str, &str)]) -> PathBuf {
    let p = dir.path().join(name);
    let f = std::fs::File::create(&p).unwrap();
    let mut z = zip::ZipWriter::new(f);
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (n, c) in entries {
        z.start_file(*n, opts).unwrap();
        z.write_all(c.as_bytes()).unwrap();
    }
    z.finish().unwrap();
    p
}

#[test]
fn png_bytes_named_txt_is_png() {
    let d = TempDir::new().unwrap();
    let p = write(&d, "photo.txt", PNG);
    assert_eq!(detect_kind(&p).unwrap(), FileKind::Image(ImageFormat::Png));
}

#[test]
fn docx_renamed_zip_is_docx() {
    let d = TempDir::new().unwrap();
    let p = zip_with(
        &d,
        "report.zip",
        &[
            ("[Content_Types].xml", "<Types/>"),
            ("word/document.xml", "<w:document/>"),
        ],
    );
    assert_eq!(detect_kind(&p).unwrap(), FileKind::Docx);
}

#[test]
fn pptx_and_xlsx_by_entries() {
    let d = TempDir::new().unwrap();
    let p = zip_with(
        &d,
        "a.bin",
        &[("[Content_Types].xml", "x"), ("ppt/presentation.xml", "x")],
    );
    assert_eq!(detect_kind(&p).unwrap(), FileKind::Pptx);
    let x = zip_with(
        &d,
        "b.bin",
        &[("[Content_Types].xml", "x"), ("xl/workbook.xml", "x")],
    );
    assert_eq!(detect_kind(&x).unwrap(), FileKind::Xlsx);
}

#[test]
fn epub_detected_by_mimetype_entry() {
    let d = TempDir::new().unwrap();
    let p = zip_with(
        &d,
        "book.zip",
        &[
            ("mimetype", "application/epub+zip"),
            ("META-INF/container.xml", "x"),
        ],
    );
    assert_eq!(detect_kind(&p).unwrap(), FileKind::Epub);
}

#[test]
fn odt_detected_by_mimetype_entry() {
    let d = TempDir::new().unwrap();
    let p = zip_with(
        &d,
        "doc",
        &[("mimetype", "application/vnd.oasis.opendocument.text")],
    );
    assert_eq!(detect_kind(&p).unwrap(), FileKind::Odt);
}

#[test]
fn plain_zip_is_archive() {
    let d = TempDir::new().unwrap();
    let p = zip_with(&d, "stuff.zip", &[("a.txt", "hello")]);
    assert_eq!(
        detect_kind(&p).unwrap(),
        FileKind::Archive(ArchiveFormat::Zip)
    );
}

#[test]
fn pdf_magic() {
    let d = TempDir::new().unwrap();
    let p = write(&d, "noext", b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n1 0 obj\n");
    assert_eq!(detect_kind(&p).unwrap(), FileKind::Pdf);
}

#[test]
fn markdown_by_extension() {
    let d = TempDir::new().unwrap();
    let p = write(&d, "notes.md", b"# Title\n\nSome text.\n");
    assert_eq!(detect_kind(&p).unwrap(), FileKind::Markdown);
}

#[test]
fn other_text_extensions() {
    let d = TempDir::new().unwrap();
    assert_eq!(
        detect_kind(&write(&d, "a.csv", b"a,b\n1,2\n")).unwrap(),
        FileKind::Csv
    );
    assert_eq!(
        detect_kind(&write(&d, "a.json", b"{\"a\":1}")).unwrap(),
        FileKind::Json
    );
    assert_eq!(
        detect_kind(&write(&d, "a.HTML", b"<p>x</p>")).unwrap(),
        FileKind::Html
    );
    assert_eq!(
        detect_kind(&write(&d, "letter", b"{\\rtf1\\ansi hi}")).unwrap(),
        FileKind::Rtf
    );
    assert_eq!(
        detect_kind(&write(&d, "readme", "নমস্কার\n".as_bytes())).unwrap(),
        FileKind::Text
    );
}

#[test]
fn svg_by_content() {
    let d = TempDir::new().unwrap();
    let p = write(
        &d,
        "drawing.xml",
        b"<?xml version=\"1.0\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>",
    );
    assert_eq!(detect_kind(&p).unwrap(), FileKind::Image(ImageFormat::Svg));
}

#[test]
fn tar_by_ustar_magic() {
    let d = TempDir::new().unwrap();
    let mut buf = vec![0u8; 512];
    buf[..5].copy_from_slice(b"a.txt");
    buf[257..262].copy_from_slice(b"ustar");
    let p = write(&d, "x.bin", &buf);
    assert_eq!(
        detect_kind(&p).unwrap(),
        FileKind::Archive(ArchiveFormat::Tar)
    );
}

#[test]
fn zero_byte_is_unknown() {
    let d = TempDir::new().unwrap();
    let p = write(&d, "empty.pdf", b"");
    assert_eq!(detect_kind(&p).unwrap(), FileKind::Unknown);
}

#[test]
fn binary_garbage_is_unknown() {
    let d = TempDir::new().unwrap();
    let p = write(
        &d,
        "blob.dat",
        &[0xff, 0xfe, 0x00, 0x13, 0x80, 0x81, 0x00, 0xc3],
    );
    assert_eq!(detect_kind(&p).unwrap(), FileKind::Unknown);
}

#[test]
fn bengali_name_with_spaces() {
    let d = TempDir::new().unwrap();
    let p = write(&d, "রিপোর্ট ফাইল.pdf", b"%PDF-1.4\n");
    let f = detect_file(&p).unwrap();
    assert_eq!(f.name, "রিপোর্ট ফাইল.pdf");
    assert_eq!(f.kind, FileKind::Pdf);
    assert_eq!(f.size, 9);
    assert_eq!(f.path, p.to_string_lossy());
}

#[test]
fn missing_file_is_io_error() {
    let d = TempDir::new().unwrap();
    let err = detect_kind(&d.path().join("nope.pdf")).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn markdown_with_inline_svg_stays_markdown() {
    let d = TempDir::new().unwrap();
    let p = write(&d, "README.md", b"# Logo\n\n<svg width=\"10\"></svg>\n");
    assert_eq!(detect_kind(&p).unwrap(), FileKind::Markdown);
}

#[test]
fn html_with_inline_svg_stays_html() {
    let d = TempDir::new().unwrap();
    let p = write(
        &d,
        "page",
        b"<!doctype html><html><body><svg></svg></body></html>",
    );
    assert_eq!(detect_kind(&p).unwrap(), FileKind::Html);
}

#[test]
fn svg_with_comment_and_doctype_before_root() {
    let d = TempDir::new().unwrap();
    let p = write(
        &d,
        "x",
        b"<?xml version=\"1.0\"?>\n<!-- made by hand -->\n<!DOCTYPE svg>\n<svg/>",
    );
    assert_eq!(detect_kind(&p).unwrap(), FileKind::Image(ImageFormat::Svg));
}
