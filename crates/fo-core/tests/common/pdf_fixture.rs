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

/// One A4 page: a 2×2 grid stroked as a rectangle (x 100–300, y 500–600 from
/// the bottom) plus a middle horizontal and vertical line, a thin filled
/// rectangle at y 450 (a rule drawn as a fill), a large filled box (not a
/// rule), and the words "Red words" in red Times-Roman at y 700.
pub fn ruled_pdf() -> Vec<u8> {
    let mut pdf = Pdf::new();
    let (cat, tree, page, content, font) = (
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
    p.resources().fonts().pair(Name(b"FT"), font);
    p.finish();
    pdf.type1_font(font).base_font(Name(b"Times-Roman"));
    let mut c = Content::new();
    c.set_line_width(0.5)
        .rect(100.0, 500.0, 200.0, 100.0)
        .stroke()
        .move_to(100.0, 550.0)
        .line_to(300.0, 550.0)
        .move_to(200.0, 500.0)
        .line_to(200.0, 600.0)
        .stroke();
    c.rect(100.0, 449.6, 200.0, 0.8).fill_nonzero();
    c.rect(350.0, 300.0, 150.0, 100.0).fill_nonzero();
    c.set_fill_rgb(1.0, 0.0, 0.0)
        .begin_text()
        .set_font(Name(b"FT"), 12.0)
        .next_line(72.0, 700.0)
        .show(Str(b"Red words"))
        .end_text();
    pdf.stream(content, &c.finish());
    pdf.finish()
}

/// Two A4 pages in Helvetica, each with the running header "Survey notes"
/// and "Page N" at the bottom. Page 1: body text with a raised 7pt "1" after
/// "water.", a ruled 2-column × 3-row table (Month/Level, March/2.1,
/// April/2.9), and the 8pt footnote "1 Readings were taken twice." at the
/// bottom. Page 2: more body text.
pub fn structured_pdf() -> Vec<u8> {
    let mut pdf = Pdf::new();
    let (cat, tree, reg, bold) = (Ref::new(1), Ref::new(2), Ref::new(3), Ref::new(4));
    let pages = [(Ref::new(10), Ref::new(11)), (Ref::new(12), Ref::new(13))];
    pdf.catalog(cat).pages(tree);
    pdf.pages(tree).kids(pages.iter().map(|p| p.0)).count(2);
    pdf.type1_font(reg).base_font(Name(b"Helvetica"));
    pdf.type1_font(bold).base_font(Name(b"Helvetica-Bold"));
    let line = |c: &mut Content, font: &[u8], size: f32, x: f32, y: f32, t: &str| {
        c.begin_text()
            .set_font(Name(font), size)
            .next_line(x, y)
            .show(Str(t.as_bytes()))
            .end_text();
    };
    for (n, (page, content)) in pages.iter().enumerate() {
        let mut p = pdf.page(*page);
        p.media_box(Rect::new(0.0, 0.0, 595.0, 842.0))
            .parent(tree)
            .contents(*content);
        p.resources()
            .fonts()
            .pair(Name(b"R"), reg)
            .pair(Name(b"B"), bold);
        p.finish();
        let mut c = Content::new();
        line(&mut c, b"R", 9.0, 72.0, 815.0, "Survey notes");
        line(&mut c, b"R", 9.0, 280.0, 20.0, &format!("Page {}", n + 1));
        for i in 0..8 {
            let y = 760.0 - 15.0 * i as f32;
            line(
                &mut c,
                b"R",
                11.0,
                72.0,
                y,
                "The survey went down to the river at dawn each day.",
            );
        }
        if n == 0 {
            line(
                &mut c,
                b"R",
                11.0,
                72.0,
                630.0,
                "The fog was lying on the water.",
            );
            // The raised marker right after "water.".
            c.begin_text()
                .set_font(Name(b"R"), 7.0)
                .set_rise(4.0)
                .next_line(226.0, 630.0)
                .show(Str(b"1"))
                .end_text();
            line(
                &mut c,
                b"R",
                11.0,
                72.0,
                615.0,
                "Every marker was read twice.",
            );
            // Table: x 72|200|328, y 580|560|540|520.
            let (xs, ys) = ([72.0, 200.0, 328.0], [580.0, 560.0, 540.0, 520.0]);
            c.set_line_width(0.5);
            for y in ys {
                c.move_to(xs[0], y).line_to(xs[2], y);
            }
            for x in xs {
                c.move_to(x, ys[0]).line_to(x, ys[3]);
            }
            c.stroke();
            let cells = [["Month", "Level"], ["March", "2.1"], ["April", "2.9"]];
            for (r, row) in cells.iter().enumerate() {
                for (k, t) in row.iter().enumerate() {
                    let font: &[u8] = if r == 0 { b"B" } else { b"R" };
                    line(&mut c, font, 11.0, xs[k] + 5.0, ys[r] - 15.0, t);
                }
            }
            c.set_fill_rgb(0.0, 0.2, 0.6); // dark blue (0, 51, 153)
            line(
                &mut c,
                b"R",
                11.0,
                72.0,
                490.0,
                "The level fell slowly through the spring.",
            );
            c.set_fill_rgb(0.0, 0.0, 0.0);
            line(
                &mut c,
                b"R",
                8.0,
                72.0,
                80.0,
                "1 Readings were taken twice.",
            );
        }
        pdf.stream(*content, &c.finish());
    }
    pdf.finish()
}
