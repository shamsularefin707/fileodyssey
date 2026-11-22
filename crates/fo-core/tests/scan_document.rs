//! Saving reviewed scan pages as a PDF and/or JPEG copies.
use fo_core::scan::document::save_scan;
use std::path::{Path, PathBuf};

/// A JPEG of `w`×`h` with a solid shade.
fn jpeg(dir: &Path, name: &str, w: u32, h: u32, shade: u8) -> PathBuf {
    let img = image::RgbImage::from_pixel(w, h, image::Rgb([shade, shade, shade]));
    let p = dir.join(name);
    img.save(&p).unwrap();
    p
}

fn pages() -> (tempfile::TempDir, Vec<PathBuf>) {
    let d = tempfile::TempDir::new().unwrap();
    let v = vec![
        jpeg(d.path(), "p0.jpg", 210, 297, 40),  // A4 shape
        jpeg(d.path(), "p1.jpg", 300, 300, 120), // square
        jpeg(d.path(), "p2.jpg", 297, 210, 200), // landscape
    ];
    (d, v)
}

#[test]
fn pdf_follows_the_order_shown_with_jpegs_unchanged() {
    let (d, p) = pages();
    let out = d.path().join("out");
    std::fs::create_dir(&out).unwrap();
    let files = save_scan(&p, &[2, 0, 1], &out, "Scan 2026-11-22 14.05", true, false).unwrap();
    assert_eq!(files, vec![out.join("Scan 2026-11-22 14.05.pdf")]);
    let bytes = std::fs::read(&files[0]).unwrap();
    // JPEG data embedded as is.
    for f in &p {
        let j = std::fs::read(f).unwrap();
        assert!(
            bytes.windows(j.len()).any(|w| w == j),
            "{f:?} embedded unchanged"
        );
    }
    let pdfium = fo_core::reader::pdf::engine().unwrap();
    let pages = pdfium.extract(&files[0]).unwrap();
    let sizes: Vec<(i32, i32)> = pages
        .iter()
        .map(|p| (p.width.round() as i32, p.height.round() as i32))
        .collect();
    // A4 width; height follows each page's shape: landscape, A4, square.
    assert_eq!(sizes, vec![(595, 421), (595, 842), (595, 595)]);
}

#[test]
fn jpeg_copies_and_no_overwrite() {
    let (d, p) = pages();
    let out = d.path().join("out");
    std::fs::create_dir(&out).unwrap();
    std::fs::write(out.join("Scan.pdf"), b"older").unwrap();
    let files = save_scan(&p, &[1, 0], &out, "Scan", true, true).unwrap();
    assert_eq!(
        files,
        vec![
            out.join("Scan (2).pdf"),
            out.join("Scan (page 1).jpg"),
            out.join("Scan (page 2).jpg")
        ]
    );
    assert_eq!(std::fs::read(out.join("Scan.pdf")).unwrap(), b"older");
    assert_eq!(
        std::fs::read(&files[1]).unwrap(),
        std::fs::read(&p[1]).unwrap()
    );
}

#[test]
fn nothing_to_save_is_an_error() {
    let (d, p) = pages();
    assert!(save_scan(&p, &[], d.path(), "Scan", true, true).is_err());
    assert!(save_scan(&p, &[3], d.path(), "Scan", true, false).is_err());
    assert!(save_scan(&p, &[0], d.path(), "Scan", false, false).is_err());
}
