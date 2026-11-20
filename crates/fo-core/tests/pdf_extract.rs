#[path = "common/pdf_fixture.rs"]
#[allow(dead_code)] // each test binary uses part of the shared fixtures
mod fixture;

use fo_core::reader::pdf::engine;

#[test]
fn extracts_glyphs_images_and_links() {
    let d = tempfile::TempDir::new().unwrap();
    let p = d.path().join("sample.pdf");
    std::fs::write(&p, fixture::sample_pdf()).unwrap();
    let pages = engine().unwrap().extract(&p).unwrap();
    assert_eq!(pages.len(), 1);
    let pg = &pages[0];
    assert_eq!((pg.width, pg.height), (595.0, 842.0));
    let text: String = pg.glyphs.iter().map(|g| g.text.as_str()).collect();
    assert!(
        text.contains("Report title") && text.contains("Plain body text here"),
        "{text}"
    );
    let r = pg.glyphs.iter().find(|g| g.text == "R").unwrap();
    assert!(r.bold && (r.size - 24.0).abs() < 0.5, "{r:?}");
    assert!(
        (r.y0 - 62.0).abs() < 30.0 && r.y1 > r.y0,
        "top-left origin: {r:?}"
    );
    let s = pg.glyphs.iter().find(|g| g.text == "S").unwrap();
    assert!(s.italic && !s.bold, "{s:?}");
    let pl = pg.glyphs.iter().find(|g| g.text == "P").unwrap();
    assert!(!pl.bold && (pl.size - 11.0).abs() < 0.5);
    assert_eq!(pg.images.len(), 1);
    let im = &pg.images[0];
    assert!(
        (im.x0 - 100.0).abs() < 1.0 && (im.x1 - 300.0).abs() < 1.0,
        "{:?}",
        (im.x0, im.x1)
    );
    assert!(
        (im.y0 - 242.0).abs() < 1.0 && (im.y1 - 342.0).abs() < 1.0,
        "{:?}",
        (im.y0, im.y1)
    );
    let decoded = image::load_from_memory(&im.png).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (40, 20));
    assert_eq!(pg.links.len(), 1);
    assert_eq!(pg.links[0].uri, "https://example.org/report");
    assert!(
        pg.links[0].y0 < 110.0 && pg.links[0].y1 > 90.0,
        "{:?}",
        pg.links[0]
    );
}
