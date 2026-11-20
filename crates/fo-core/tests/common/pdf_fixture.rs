//! A small PDF with known content, for extraction tests.
use pdf_writer::types::{ActionType, AnnotationType};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref, Str};

/// One A4 page: a bold 24pt title at y=780 (from bottom), an 11pt body line
/// at 740 with a link over it, an oblique line at 720, and a 40×20 px image
/// drawn at x 100–300, y 500–600.
pub fn sample_pdf() -> Vec<u8> {
    let mut pdf = Pdf::new();
    let (cat, tree, page, content) = (Ref::new(1), Ref::new(2), Ref::new(3), Ref::new(4));
    let (bold, reg, ital, img, link) = (
        Ref::new(5),
        Ref::new(6),
        Ref::new(7),
        Ref::new(8),
        Ref::new(9),
    );
    pdf.catalog(cat).pages(tree);
    pdf.pages(tree).kids([page]).count(1);
    let mut p = pdf.page(page);
    p.media_box(Rect::new(0.0, 0.0, 595.0, 842.0))
        .parent(tree)
        .contents(content);
    p.annotations([link]);
    let mut res = p.resources();
    res.fonts()
        .pair(Name(b"FB"), bold)
        .pair(Name(b"FR"), reg)
        .pair(Name(b"FI"), ital);
    res.x_objects().pair(Name(b"Im1"), img);
    res.finish();
    p.finish();
    pdf.type1_font(bold).base_font(Name(b"Helvetica-Bold"));
    pdf.type1_font(reg).base_font(Name(b"Helvetica"));
    pdf.type1_font(ital).base_font(Name(b"Helvetica-Oblique"));
    let pixels: Vec<u8> = (0..40 * 20)
        .flat_map(|i| [(i % 40 * 6) as u8, 120, 200])
        .collect();
    let data = miniz_oxide::deflate::compress_to_vec_zlib(&pixels, 6);
    let mut x = pdf.image_xobject(img, &data);
    x.filter(Filter::FlateDecode);
    x.width(40).height(20).bits_per_component(8);
    x.color_space().device_rgb();
    x.finish();
    let mut a = pdf.annotation(link);
    a.subtype(AnnotationType::Link)
        .rect(Rect::new(70.0, 735.0, 200.0, 752.0));
    a.action()
        .action_type(ActionType::Uri)
        .uri(Str(b"https://example.org/report"));
    a.finish();
    let mut c = Content::new();
    c.begin_text()
        .set_font(Name(b"FB"), 24.0)
        .next_line(72.0, 780.0)
        .show(Str(b"Report title"))
        .end_text();
    c.begin_text()
        .set_font(Name(b"FR"), 11.0)
        .next_line(72.0, 740.0)
        .show(Str(b"Plain body text here"))
        .end_text();
    c.begin_text()
        .set_font(Name(b"FI"), 11.0)
        .next_line(72.0, 720.0)
        .show(Str(b"Slanted words"))
        .end_text();
    c.save_state()
        .transform([200.0, 0.0, 0.0, 100.0, 100.0, 500.0])
        .x_object(Name(b"Im1"))
        .restore_state();
    pdf.stream(content, &c.finish());
    pdf.finish()
}

/// One page that is only a picture (like a scan): no text at all.
pub fn scanned_pdf() -> Vec<u8> {
    let mut pdf = Pdf::new();
    let (cat, tree, page, content, img) = (
        Ref::new(1),
        Ref::new(2),
        Ref::new(3),
        Ref::new(4),
        Ref::new(5),
    );
    pdf.catalog(cat).pages(tree);
    pdf.pages(tree).kids([page]).count(1);
    let mut p = pdf.page(page);
    p.media_box(Rect::new(0.0, 0.0, 300.0, 400.0))
        .parent(tree)
        .contents(content);
    p.resources().x_objects().pair(Name(b"Scan"), img);
    p.finish();
    let pixels: Vec<u8> = (0..60 * 80)
        .flat_map(|i| [(i % 60 * 4) as u8, 200, (i / 60 * 3) as u8])
        .collect();
    let data = miniz_oxide::deflate::compress_to_vec_zlib(&pixels, 6);
    let mut x = pdf.image_xobject(img, &data);
    x.filter(Filter::FlateDecode);
    x.width(60).height(80).bits_per_component(8);
    x.color_space().device_rgb();
    x.finish();
    let mut c = Content::new();
    c.save_state()
        .transform([300.0, 0.0, 0.0, 400.0, 0.0, 0.0])
        .x_object(Name(b"Scan"))
        .restore_state();
    pdf.stream(content, &c.finish());
    pdf.finish()
}
