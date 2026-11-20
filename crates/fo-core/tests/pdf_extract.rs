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

#[test]
fn extracts_rules_fonts_and_colours() {
    let d = tempfile::TempDir::new().unwrap();
    let p = d.path().join("ruled.pdf");
    std::fs::write(&p, fixture::ruled_pdf()).unwrap();
    let pg = &engine().unwrap().extract(&p).unwrap()[0];
    let horiz: Vec<_> = pg
        .rules
        .iter()
        .filter(|r| r.x1 - r.x0 > r.y1 - r.y0)
        .collect();
    let vert: Vec<_> = pg
        .rules
        .iter()
        .filter(|r| r.x1 - r.x0 <= r.y1 - r.y0)
        .collect();
    assert_eq!((horiz.len(), vert.len()), (4, 3), "{:?}", pg.rules);
    // Top-left origin: the grid's top edge (y 600 from the bottom) is at 242.
    assert!(horiz
        .iter()
        .any(|r| (r.y0 + r.y1) / 2.0 - 242.0 < 1.0 && r.x0 < 101.0 && r.x1 > 299.0));
    assert!(
        horiz
            .iter()
            .any(|r| ((r.y0 + r.y1) / 2.0 - 392.0).abs() < 1.0),
        "filled rule at y 450"
    );
    assert!(vert
        .iter()
        .any(|r| ((r.x0 + r.x1) / 2.0 - 200.0).abs() < 1.0 && r.y1 - r.y0 > 99.0));
    let r = pg.glyphs.iter().find(|g| g.text == "R").unwrap();
    assert_eq!(r.color, [255, 0, 0], "{r:?}");
    assert!(r.font.contains("Times"), "{r:?}");
}

#[test]
fn font_families_from_pdf_names() {
    use fo_core::pdfconv::family_of;
    assert_eq!(
        family_of("ABCDEF+TimesNewRomanPS-BoldMT"),
        "Times New Roman"
    );
    assert_eq!(family_of("ArialMT"), "Arial");
    assert_eq!(family_of("QWERTY+DejaVuSans-Bold"), "DejaVu Sans");
    assert_eq!(family_of("NotoSansBengali-Regular"), "Noto Sans Bengali");
    assert_eq!(family_of("FreeSerif"), "FreeSerif");
    assert_eq!(family_of("Helvetica-Oblique"), "Helvetica");
    assert_eq!(family_of("Liberation Serif"), "Liberation Serif");
}
