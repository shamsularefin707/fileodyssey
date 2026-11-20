use fo_core::pdfconv::layout::{layout, BlockKind, Script};
use fo_core::pdfconv::{Glyph, PageContent, PageImage, PageLink};

/// Glyphs for `text` starting at (x, y-top), 0.5em per char.
fn run(out: &mut Vec<Glyph>, text: &str, x: f32, y: f32, size: f32, bold: bool) {
    let mut cx = x;
    for c in text.chars() {
        let w = size * 0.5;
        out.push(Glyph {
            text: c.to_string(),
            x0: cx,
            y0: y,
            x1: cx + w,
            y1: y + size,
            size,
            bold,
            italic: false,
            mono: false,
            generated: false,
            hyphen: false,
            font: String::new(),
            color: [0, 0, 0],
        });
        cx += w;
    }
}

fn page(glyphs: Vec<Glyph>) -> PageContent {
    PageContent {
        width: 600.0,
        height: 800.0,
        glyphs,
        images: vec![],
        links: vec![],
        spaces: vec![],
        rules: vec![],
    }
}

fn texts(blocks: &[fo_core::pdfconv::layout::Block]) -> Vec<String> {
    blocks.iter().map(|b| b.text()).collect()
}

const L: [&str; 4] = [
    "left column first line here",
    "left column second line",
    "right column first line here",
    "right column second line",
];

#[test]
fn two_columns_under_a_full_width_title() {
    let mut g = Vec::new();
    run(
        &mut g,
        "A title across both columns",
        50.0,
        40.0,
        20.0,
        true,
    );
    // Columns at x 50–250 and 320–520; the right column is drawn FIRST in the
    // content stream to prove order comes from geometry, not the stream.
    run(&mut g, L[2], 320.0, 100.0, 10.0, false);
    run(&mut g, L[3], 320.0, 114.0, 10.0, false);
    run(&mut g, L[0], 50.0, 100.0, 10.0, false);
    run(&mut g, L[1], 50.0, 114.0, 10.0, false);
    // Enough body text elsewhere for 10pt to be the body size.
    for i in 0..6 {
        run(
            &mut g,
            "more body text in the left column",
            50.0,
            200.0 + 14.0 * i as f32,
            10.0,
            false,
        );
    }
    let blocks = layout(&[page(g)]);
    let t = texts(&blocks);
    assert_eq!(blocks[0].kind, BlockKind::Heading(1), "{t:?}");
    let pos = |s: &str| {
        t.iter()
            .position(|x| x.contains(s))
            .unwrap_or_else(|| panic!("{s} missing in {t:?}"))
    };
    assert!(
        pos("left column first") < pos("right column first"),
        "{t:?}"
    );
    assert!(
        pos("more body text") < pos("right column first"),
        "left column ends before the right begins: {t:?}"
    );
    assert!(
        t[pos("left column first")].contains("left column second line"),
        "lines join into a paragraph: {t:?}"
    );
}

#[test]
fn paragraphs_headings_and_lists() {
    let mut g = Vec::new();
    run(&mut g, "Main heading", 50.0, 40.0, 18.0, true);
    run(&mut g, "First paragraph line one", 50.0, 80.0, 10.0, false);
    run(&mut g, "first paragraph line two.", 50.0, 93.0, 10.0, false);
    run(
        &mut g,
        "Second paragraph after a gap",
        50.0,
        125.0,
        10.0,
        false,
    );
    run(&mut g, "Sub heading", 50.0, 160.0, 13.0, true);
    run(&mut g, "• Apples are red", 50.0, 185.0, 10.0, false);
    run(&mut g, "• Pears are green", 50.0, 198.0, 10.0, false);
    run(&mut g, "1. Step one", 50.0, 225.0, 10.0, false);
    run(&mut g, "2. Step two", 50.0, 238.0, 10.0, false);
    run(&mut g, "Bold lead-in line", 50.0, 270.0, 10.0, true);
    run(
        &mut g,
        "A normal closing paragraph with several words in it.",
        50.0,
        290.0,
        10.0,
        false,
    );
    let blocks = layout(&[page(g)]);
    let kinds: Vec<_> = blocks.iter().map(|b| (b.kind.clone(), b.text())).collect();
    assert_eq!(
        kinds[0],
        (BlockKind::Heading(1), "Main heading".into()),
        "{kinds:#?}"
    );
    assert_eq!(
        kinds[1],
        (
            BlockKind::Para,
            "First paragraph line one first paragraph line two.".into()
        )
    );
    assert_eq!(
        kinds[2],
        (BlockKind::Para, "Second paragraph after a gap".into())
    );
    assert_eq!(kinds[3], (BlockKind::Heading(2), "Sub heading".into()));
    assert_eq!(
        kinds[4],
        (
            BlockKind::ListItem {
                ordered: false,
                level: 0
            },
            "Apples are red".into()
        )
    );
    assert_eq!(
        kinds[5],
        (
            BlockKind::ListItem {
                ordered: false,
                level: 0
            },
            "Pears are green".into()
        )
    );
    assert_eq!(
        kinds[6],
        (
            BlockKind::ListItem {
                ordered: true,
                level: 0
            },
            "Step one".into()
        )
    );
    assert_eq!(
        kinds[8].0,
        BlockKind::Heading(3),
        "bold body-size line is a lower heading: {kinds:#?}"
    );
}

#[test]
fn hyphenation_styles_links_and_images() {
    let mut g = Vec::new();
    run(&mut g, "A sentence with a hyphen", 50.0, 40.0, 10.0, false);
    let last = g.len();
    run(&mut g, "-", 170.0, 40.0, 10.0, false);
    g[last].hyphen = true;
    run(
        &mut g,
        "ated word and a well-known one.",
        50.0,
        53.0,
        10.0,
        false,
    );
    run(&mut g, "Some ", 50.0, 100.0, 10.0, false);
    run(&mut g, "bold", 75.0, 100.0, 10.0, true);
    run(&mut g, " and linked text here.", 95.0, 100.0, 10.0, false);
    run(&mut g, "Below the picture.", 50.0, 300.0, 10.0, false);
    let mut p = page(g);
    p.links.push(PageLink {
        x0: 118.0,
        y0: 98.0,
        x1: 150.0,
        y1: 112.0,
        uri: "https://x.org".into(),
    });
    p.images.push(PageImage {
        x0: 50.0,
        y0: 150.0,
        x1: 250.0,
        y1: 260.0,
        png: vec![1],
    });
    let blocks = layout(&[p]);
    assert_eq!(
        blocks[0].text(),
        "A sentence with a hyphenated word and a well-known one."
    );
    let spans = &blocks[1].lines[0].spans;
    assert!(
        spans.iter().any(|s| s.bold && s.text.trim() == "bold"),
        "{spans:?}"
    );
    assert!(
        spans
            .iter()
            .any(|s| s.link.as_deref() == Some("https://x.org") && s.text.contains("linked")),
        "{spans:?}"
    );
    assert_eq!(blocks[2].kind, BlockKind::Image(0));
    assert_eq!(blocks[3].text(), "Below the picture.");
}

#[test]
fn stream_order_inside_a_line_is_kept_and_page_numbers_drop() {
    // Bengali "কি" is drawn with the vowel sign (U+09BF) LEFT of the consonant
    // but extracted after it; sorting by x would scramble it.
    let mut pages = Vec::new();
    for n in 1..=3 {
        let mut g = Vec::new();
        run(
            &mut g,
            "Body text on this page goes here",
            50.0,
            60.0,
            10.0,
            false,
        );
        if n == 1 {
            g.push(Glyph {
                text: "ক".into(),
                x0: 205.0,
                y0: 60.0,
                x1: 211.0,
                y1: 70.0,
                size: 10.0,
                bold: false,
                italic: false,
                mono: false,
                generated: false,
                hyphen: false,
                font: String::new(),
                color: [0, 0, 0],
            });
            g.push(Glyph {
                text: "ি".into(),
                x0: 200.0,
                y0: 60.0,
                x1: 205.0,
                y1: 70.0,
                size: 10.0,
                bold: false,
                italic: false,
                mono: false,
                generated: false,
                hyphen: false,
                font: String::new(),
                color: [0, 0, 0],
            });
        }
        run(&mut g, &n.to_string(), 295.0, 770.0, 9.0, false);
        run(&mut g, "Running header text", 50.0, 15.0, 8.0, false);
        pages.push(page(g));
    }
    let t = texts(&layout(&pages));
    assert!(t[0].ends_with("কি"), "{t:?}");
    assert!(
        t.iter()
            .all(|x| x != "1" && x != "2" && !x.contains("Running header")),
        "{t:?}"
    );
    assert_eq!(
        t.len(),
        3,
        "one paragraph per page, no merging across pages: {t:?}"
    );
}

fn glyph(text: &str, x0: f32, x1: f32, y0: f32, y1: f32, generated: bool) -> Glyph {
    Glyph {
        text: text.into(),
        x0,
        y0,
        x1,
        y1,
        size: 20.0,
        bold: false,
        italic: false,
        mono: false,
        generated,
        hyphen: false,
        font: String::new(),
        color: [0, 0, 0],
    }
}

/// Chrome draws each word gap of complex scripts as its own (empty) space
/// object while PDFium generates a space between every cluster: the drawn
/// spaces decide.
#[test]
fn drawn_spaces_decide_word_boundaries() {
    let g = vec![
        glyph("ab", 50.0, 70.0, 40.0, 60.0, false),
        glyph(" ", 70.0, 70.0, 60.0, 60.0, true),
        glyph("cd", 74.0, 94.0, 40.0, 60.0, false),
        glyph(" ", 94.0, 94.0, 60.0, 60.0, true),
        glyph("ef", 98.0, 118.0, 40.0, 60.0, false),
    ];
    let mut p = page(g);
    // Both gaps are 0.2em; one drawn space, between "cd" and "ef", on the baseline.
    p.spaces = vec![[96.0, 60.5, 96.0, 60.5]];
    assert_eq!(texts(&layout(&[p])), vec!["abcd ef"]);
}

/// A tiny mark glyph whose text map repeats its cluster, and the full-size
/// glyphs after it that repeat the rest of the cluster, are dropped once.
#[test]
fn mark_copies_of_a_cluster_are_dropped() {
    let g = vec![
        glyph("p", 50.0, 55.0, 40.0, 60.0, false),
        glyph("u", 55.0, 60.0, 40.0, 60.0, false),
        glyph("r", 60.0, 65.0, 40.0, 60.0, false),
        glyph("e", 65.0, 70.0, 40.0, 60.0, false),
        // Mark below "pu": tiny, repeats "p", "u"; then "r", "e" repeat too.
        glyph("p", 56.0, 58.0, 58.0, 63.0, false),
        glyph("u", 58.0, 60.0, 58.0, 63.0, false),
        glyph("r", 70.0, 73.0, 40.0, 60.0, false),
        glyph("e", 73.0, 76.0, 40.0, 60.0, false),
        glyph("x", 76.0, 81.0, 40.0, 60.0, false),
    ];
    assert_eq!(texts(&layout(&[page(g)])), vec!["purex"]);
}

/// A page whose text carries real space characters isn't one of those
/// producers: a stray space object must not override its word gaps.
#[test]
fn drawn_spaces_ignored_when_text_has_real_spaces() {
    let g = vec![
        glyph("ab", 50.0, 70.0, 40.0, 60.0, false),
        glyph(" ", 70.0, 74.0, 40.0, 60.0, false),
        glyph("cd", 74.0, 94.0, 40.0, 60.0, false),
        glyph(" ", 94.0, 94.0, 60.0, 60.0, true),
        glyph("ef", 98.0, 118.0, 40.0, 60.0, false),
    ];
    let mut p = page(g);
    p.spaces = vec![[300.0, 60.5, 300.0, 60.5]];
    assert_eq!(texts(&layout(&[p])), vec!["ab cd ef"]);
}

/// A raised, smaller "1" after "water." is a superscript in its place, and
/// the words around it keep their order.
#[test]
fn superscript_marker_stays_in_place() {
    let mut g = Vec::new();
    run(&mut g, "lying on the water.", 50.0, 100.0, 10.0, false);
    let x = g.last().unwrap().x1;
    // 7pt, its bottom 4pt above the line's bottom.
    run(&mut g, "1", x, 99.0, 7.0, false);
    let x = g.last().unwrap().x1 + 3.0;
    run(&mut g, " The next sentence", x, 100.0, 10.0, false);
    for i in 0..4 {
        run(
            &mut g,
            "filler text to set the body size",
            50.0,
            140.0 + 14.0 * i as f32,
            10.0,
            false,
        );
    }
    let blocks = layout(&[page(g)]);
    let spans = &blocks[0].lines[0].spans;
    let sup: Vec<_> = spans.iter().filter(|s| s.script == Script::Super).collect();
    assert_eq!(sup.len(), 1, "{spans:?}");
    assert_eq!(sup[0].text.trim(), "1");
    assert!(
        blocks[0]
            .text()
            .starts_with("lying on the water.1 The next"),
        "{}",
        blocks[0].text()
    );
}

#[test]
fn spans_carry_font_size_and_colour() {
    let mut g = Vec::new();
    run(&mut g, "plain ", 50.0, 100.0, 10.0, false);
    let x = g.last().unwrap().x1;
    let start = g.len();
    run(&mut g, "red", x, 100.0, 10.0, false);
    for c in &mut g[start..] {
        c.color = [200, 0, 0];
        c.font = "Times New Roman".into();
    }
    let blocks = layout(&[page(g)]);
    let spans = &blocks[0].lines[0].spans;
    assert_eq!(spans.len(), 2, "{spans:?}");
    assert_eq!(
        (spans[1].color, spans[1].font.as_str()),
        ([200, 0, 0], "Times New Roman")
    );
    assert!((spans[0].size - 10.0).abs() < 0.01 && spans[0].script == Script::Normal);
}

#[test]
fn ast_writes_superscripts() {
    use fo_core::pdfconv::ast::inlines;
    use fo_core::pdfconv::layout::{Line, Span};
    let l = Line {
        spans: vec![
            Span {
                text: "water.".into(),
                size: 10.0,
                ..Span::default()
            },
            Span {
                text: "1".into(),
                size: 7.0,
                script: Script::Super,
                ..Span::default()
            },
            Span {
                text: " Next".into(),
                size: 10.0,
                ..Span::default()
            },
        ],
        ..Line::default()
    };
    let v = serde_json::to_string(&inlines(&[l])).unwrap();
    assert_eq!(
        v,
        r#"[{"c":"water.","t":"Str"},{"c":[{"c":"1","t":"Str"}],"t":"Superscript"},{"t":"Space"},{"c":"Next","t":"Str"}]"#
    );
}

/// "R. Hossain and T. Alam" is a byline, not a one-item lettered list;
/// "a. first" followed by "b. second" is a list.
#[test]
fn lone_letter_marker_is_not_a_list() {
    let mut g = Vec::new();
    run(&mut g, "R. Hossain and T. Alam", 50.0, 100.0, 10.0, false);
    run(&mut g, "a. first item", 50.0, 140.0, 10.0, false);
    run(&mut g, "b. second item", 50.0, 154.0, 10.0, false);
    for i in 0..4 {
        run(
            &mut g,
            "filler text to set the body size",
            50.0,
            200.0 + 14.0 * i as f32,
            10.0,
            false,
        );
    }
    let blocks = layout(&[page(g)]);
    assert_eq!(blocks[0].kind, BlockKind::Para);
    assert_eq!(blocks[0].text(), "R. Hossain and T. Alam");
    assert!(matches!(
        blocks[1].kind,
        BlockKind::ListItem { ordered: true, .. }
    ));
    assert!(matches!(
        blocks[2].kind,
        BlockKind::ListItem { ordered: true, .. }
    ));
}

fn sized(g: &mut Vec<Glyph>, text: &str, x: f32, y: f32, size: f32) {
    run(g, text, x, y, size, false);
}

/// Body with a raised "1" after "water."; at the bottom of the page a small
/// line "1 Readings were taken twice." — a footnote, linked at the marker.
fn footnote_page(marker_in_body: bool) -> PageContent {
    let mut g = Vec::new();
    for i in 0..6 {
        sized(
            &mut g,
            "ordinary body text of the page",
            50.0,
            100.0 + 14.0 * i as f32,
            10.0,
        );
    }
    sized(&mut g, "lying on the water.", 50.0, 200.0, 10.0);
    if marker_in_body {
        let x = g.last().unwrap().x1;
        sized(&mut g, "1", x, 199.0, 7.0);
    }
    sized(&mut g, "1 Readings were taken twice.", 50.0, 700.0, 8.0);
    sized(&mut g, "at every marker.", 50.0, 710.0, 8.0);
    page(g)
}

#[test]
fn footnote_at_page_bottom_links_to_its_marker() {
    use fo_core::pdfconv::layout::reconstruct;
    let doc = reconstruct(&[footnote_page(true)]);
    assert_eq!(doc.notes.len(), 1);
    let note: Vec<String> = doc.notes[0].iter().map(|l| l.text()).collect();
    assert_eq!(
        note.join(" "),
        "Readings were taken twice. at every marker."
    );
    let marker = doc
        .blocks
        .iter()
        .flat_map(|b| &b.lines)
        .flat_map(|l| &l.spans)
        .find(|s| s.note.is_some())
        .expect("marker span");
    assert_eq!((marker.note, marker.text.trim()), (Some(0), "1"));
    assert!(!doc.blocks.iter().any(|b| b.text().contains("Readings")));
}

#[test]
fn small_bottom_line_without_a_marker_stays_text() {
    use fo_core::pdfconv::layout::reconstruct;
    let doc = reconstruct(&[footnote_page(false)]);
    assert!(doc.notes.is_empty());
    assert!(doc
        .blocks
        .iter()
        .any(|b| b.text().contains("Readings were taken twice.")));
}

#[test]
fn running_header_and_page_numbers_are_kept_apart() {
    use fo_core::pdfconv::layout::reconstruct;
    let pages: Vec<PageContent> = (1..=2)
        .map(|n| {
            let mut g = Vec::new();
            sized(&mut g, "Field notes, spring survey", 50.0, 30.0, 9.0);
            for i in 0..6 {
                sized(
                    &mut g,
                    "ordinary body text of the page",
                    50.0,
                    100.0 + 14.0 * i as f32,
                    10.0,
                );
            }
            sized(&mut g, &format!("Page {n}"), 280.0, 770.0, 9.0);
            page(g)
        })
        .collect();
    let doc = reconstruct(&pages);
    assert_eq!(doc.header.as_deref(), Some("Field notes, spring survey"));
    assert!(doc.page_numbers);
    assert!(!doc
        .blocks
        .iter()
        .any(|b| b.text().contains("Field notes") || b.text().contains("Page")));
}
