use fo_core::reader::epub::Book;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const CONTAINER: &str = r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;

fn epub(dir: &Path, name: &str, files: &[(&str, &[u8])]) -> PathBuf {
    let path = dir.join(name);
    let mut z = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let opts = zip::write::SimpleFileOptions::default();
    for (n, data) in files {
        z.start_file(*n, opts).unwrap();
        z.write_all(data).unwrap();
    }
    z.finish().unwrap();
    path
}

const CH1: &str = r#"<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>c1</title><link rel="stylesheet" href="s.css"/></head><body><h1 id="top">Morning</h1><p>Go <a href="text/ch2.xhtml#end">on</a>.</p><img src="img/c.png" alt="c"/><script>alert(1)</script></body></html>"#;
const CH2: &str = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><h1>Evening</h1><p id="end">Back to <a href="../ch1.xhtml">start</a>.</p></body></html>"#;

fn epub3(dir: &Path) -> PathBuf {
    let opf = r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>The Long Walk</dc:title></metadata>
<manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="c2" href="text/ch2.xhtml" media-type="application/xhtml+xml"/><item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/><item id="split" href="ch1b.xhtml" media-type="application/xhtml+xml"/><item id="img" href="img/c.png" media-type="image/png"/></manifest>
<spine><itemref idref="c1"/><itemref idref="split"/><itemref idref="c2"/></spine></package>"#;
    let nav = r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="landmarks"><ol><li><a href="text/ch2.xhtml">Wrong</a></li></ol></nav><nav epub:type="toc"><ol><li><a href="ch1.xhtml#top">One: <b>Morning</b></a></li><li><a href="text/ch2.xhtml">Two: Evening</a></li></ol></nav></body></html>"#;
    epub(
        dir,
        "walk.epub",
        &[
            ("mimetype", b"application/epub+zip"),
            ("META-INF/container.xml", CONTAINER.as_bytes()),
            ("OEBPS/content.opf", opf.as_bytes()),
            ("OEBPS/nav.xhtml", nav.as_bytes()),
            ("OEBPS/ch1.xhtml", CH1.as_bytes()),
            ("OEBPS/ch1b.xhtml", b"<html><body><p>more</p></body></html>"),
            ("OEBPS/text/ch2.xhtml", CH2.as_bytes()),
            ("OEBPS/img/c.png", b"\x89PNG fake"),
        ],
    )
}

#[test]
fn epub3_titles_spine_and_chapters() {
    let d = TempDir::new().unwrap();
    let mut b = Book::open(&epub3(d.path())).unwrap();
    let info = b.info();
    assert_eq!(info.title.as_deref(), Some("The Long Walk"));
    assert_eq!(
        info.chapters,
        vec![
            "One: Morning".to_string(),
            String::new(),
            "Two: Evening".to_string()
        ]
    );
    let c1 = b.chapter(0).unwrap();
    assert!(c1.contains(r#"href="fo-ch:2#fo-end""#), "{c1}");
    assert!(c1.contains(r#"data-fo-src="OEBPS/img/c.png""#), "{c1}");
    assert!(!c1.contains("alert") && !c1.contains("<title"), "{c1}");
    let c2 = b.chapter(2).unwrap();
    assert!(c2.contains(r#"href="fo-ch:0""#), "{c2}");
    assert!(b.chapter(3).is_err());
}

#[test]
fn resources_are_served_only_from_inside_the_book() {
    let d = TempDir::new().unwrap();
    let mut b = Book::open(&epub3(d.path())).unwrap();
    let (bytes, mime) = b.resource("OEBPS/img/c.png").unwrap();
    assert_eq!(
        (bytes.as_slice(), mime),
        (&b"\x89PNG fake"[..], "image/png")
    );
    assert!(b.resource("../../etc/passwd").is_err());
    assert!(b.resource("OEBPS/../../x").is_err());
    assert!(b.resource("OEBPS/missing.png").is_err());
}

#[test]
fn epub2_ncx_titles() {
    let d = TempDir::new().unwrap();
    let opf = r#"<package xmlns="http://www.idpf.org/2007/opf" version="2.0"><metadata/><manifest><item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/><item id="a" href="a.html" media-type="application/xhtml+xml"/><item id="b" href="b%20two.html" media-type="application/xhtml+xml"/></manifest><spine toc="ncx"><itemref idref="a"/><itemref idref="b"/></spine></package>"#;
    let ncx = r#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/"><navMap><navPoint id="p1"><navLabel><text>Alpha</text></navLabel><content src="a.html"/></navPoint><navPoint id="p2"><navLabel><text>Beta</text></navLabel><content src="b%20two.html#x"/></navPoint></navMap></ncx>"#;
    let p = epub(
        d.path(),
        "old.epub",
        &[
            ("mimetype", b"application/epub+zip"),
            ("META-INF/container.xml", CONTAINER.as_bytes()),
            ("OEBPS/content.opf", opf.as_bytes()),
            ("OEBPS/toc.ncx", ncx.as_bytes()),
            ("OEBPS/a.html", b"<p>a</p>"),
            ("OEBPS/b two.html", b"<p>b</p>"),
        ],
    );
    let mut b = Book::open(&p).unwrap();
    assert_eq!(b.info().title, None);
    assert_eq!(
        b.info().chapters,
        vec!["Alpha".to_string(), "Beta".to_string()]
    );
    assert!(b.chapter(1).unwrap().contains("<p>b</p>"));
}

#[test]
fn drm_and_broken_books_fail_clearly() {
    let d = TempDir::new().unwrap();
    let enc = r#"<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container" xmlns:enc="http://www.w3.org/2001/04/xmlenc#"><enc:EncryptedData><enc:EncryptionMethod Algorithm="http://www.w3.org/2001/04/xmlenc#aes128-cbc"/></enc:EncryptedData></encryption>"#;
    let p = epub(
        d.path(),
        "drm.epub",
        &[
            ("mimetype", b"application/epub+zip"),
            ("META-INF/container.xml", CONTAINER.as_bytes()),
            ("META-INF/encryption.xml", enc.as_bytes()),
        ],
    );
    let e = Book::open(&p).err().unwrap();
    assert!(e.contains("DRM"), "{e}");
    // Font obfuscation alone is fine (not DRM) — only the missing OPF fails here.
    let font = r#"<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container" xmlns:enc="http://www.w3.org/2001/04/xmlenc#"><enc:EncryptedData><enc:EncryptionMethod Algorithm="http://www.idpf.org/2008/embedding"/></enc:EncryptedData></encryption>"#;
    let p = epub(
        d.path(),
        "font.epub",
        &[
            ("META-INF/container.xml", CONTAINER.as_bytes()),
            ("META-INF/encryption.xml", font.as_bytes()),
        ],
    );
    let e = Book::open(&p).err().unwrap();
    assert!(!e.contains("DRM"), "{e}");
    let p = epub(
        d.path(),
        "empty.epub",
        &[("mimetype", b"application/epub+zip")],
    );
    let e = Book::open(&p).err().unwrap();
    assert!(e.contains("damaged"), "{e}");
}
