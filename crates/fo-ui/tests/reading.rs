use fo_types::{FileKind, ImageFormat, PageSize};
use fo_ui::reading::*;

#[test]
fn zoom_steps_up_and_down_and_stops_at_the_ends() {
    assert_eq!(zoom_in(1.0), 1.25);
    assert_eq!(zoom_out(1.0), 0.8);
    assert_eq!(zoom_in(3.0), 3.0);
    assert_eq!(zoom_out(0.5), 0.5);
    assert_eq!(zoom_in(1.1), 1.25, "between steps goes to the next one");
    assert_eq!(zoom_out(1.1), 1.0);
}

#[test]
fn bitmap_widths_are_bucketed_and_clamped() {
    assert_eq!(bitmap_width(100.0, 1.0), 256);
    assert_eq!(bitmap_width(700.0, 2.0), 1536);
    assert_eq!(bitmap_width(800.0, 1.0), 1024);
    assert_eq!(bitmap_width(5000.0, 2.0), 4096);
}

#[test]
fn page_layout_and_current_page() {
    let pages = [
        PageSize { w: 600.0, h: 800.0 },
        PageSize { w: 800.0, h: 600.0 },
        PageSize { w: 600.0, h: 800.0 },
    ];
    let l = layout(&pages, 600.0, 20.0);
    assert_eq!(l.tops, vec![0.0, 820.0, 1290.0]);
    assert_eq!(l.heights, vec![800.0, 450.0, 800.0]);
    assert_eq!(l.total, 2090.0);
    assert_eq!(current_page(&l, 0.0, 600.0), 0);
    assert_eq!(
        current_page(&l, 700.0, 600.0),
        1,
        "page 2 fills most of the view"
    );
    assert_eq!(current_page(&l, 5000.0, 600.0), 2);
    assert!((hit_mark(&l, 1, 0.5) - (820.0 + 225.0) / 2090.0).abs() < 1e-9);
}

#[test]
fn chapter_lists_skip_untitled_parts_and_label_next() {
    let titles: Vec<String> = ["One", "", "Two"].iter().map(|s| s.to_string()).collect();
    assert_eq!(
        chapter_list(&titles),
        vec![(0, "One".to_string()), (2, "Two".to_string())]
    );
    assert_eq!(next_label(&titles, 0).as_deref(), Some("Continue reading"));
    assert_eq!(next_label(&titles, 1).as_deref(), Some("Next: Two"));
    assert_eq!(next_label(&titles, 2), None);
    assert_eq!(
        chapter_title(&titles, 1),
        "One",
        "untitled parts belong to the chapter before"
    );
}

#[test]
fn readable_kinds() {
    assert!(readable(&FileKind::Pdf) && readable(&FileKind::Epub) && readable(&FileKind::Markdown));
    assert!(readable(&FileKind::Image(ImageFormat::Raw)));
    assert!(
        !readable(&FileKind::Image(ImageFormat::Heic)),
        "not decodable yet"
    );
    assert!(!readable(&FileKind::Docx) && !readable(&FileKind::Unknown));
}

#[test]
fn links_in_sanitised_documents() {
    assert_eq!(
        link_action("fo-ch:3#fo-ford"),
        LinkAction::Chapter(3, Some("fo-ford".into()))
    );
    assert_eq!(link_action("fo-ch:0"), LinkAction::Chapter(0, None));
    assert_eq!(link_action("#fo-top"), LinkAction::Anchor("fo-top".into()));
    assert_eq!(
        link_action("fo-ext:https://example.org/a?b"),
        LinkAction::External("https://example.org/a?b".into())
    );
    assert_eq!(link_action(""), LinkAction::Nothing);
    assert_eq!(link_action("fo-ch:x"), LinkAction::Nothing);
}

#[test]
fn line_ends_mark_where_text_wraps() {
    use fo_types::{Rect, TextSpan};
    let sp = |x: f32, y: f32| TextSpan {
        text: "w".into(),
        rect: Rect {
            x,
            y,
            w: 0.1,
            h: 0.02,
        },
    };
    let spans = [
        sp(0.1, 0.1),
        sp(0.3, 0.101),
        sp(0.1, 0.13),
        sp(0.5, 0.13),
        sp(0.1, 0.13),
    ];
    assert_eq!(line_ends(&spans), vec![false, true, false, true, true]);
}
