#[path = "common/pdf_fixture.rs"]
#[allow(dead_code)] // each test binary uses part of the shared fixtures
mod fixture;

use fo_core::pandoc::NativePandoc;
use fo_core::pdfconv::tool::PdfConvert;
use fo_core::tool::{CancelToken, Ctx, Tool, ToolOutput};
use fo_types::FileKind;
use std::path::Path;
use std::sync::Arc;

fn run(input: &Path, target: &str) -> ToolOutput {
    let t = PdfConvert::new(Arc::new(NativePandoc::find().expect("native pandoc")));
    let work = input.parent().unwrap().join(format!("w-{target}"));
    std::fs::create_dir_all(&work).unwrap();
    let cancel = CancelToken::new();
    let opts = serde_json::json!({});
    t.run(
        input,
        &Ctx {
            work_dir: &work,
            target: Some(target),
            options: &opts,
            cancel: &cancel,
            progress: &|_| {},
        },
    )
    .unwrap()
}

#[test]
fn sample_pdf_becomes_structured_markdown() {
    let d = tempfile::TempDir::new().unwrap();
    let src = d.path().join("sample.pdf");
    std::fs::write(&src, fixture::sample_pdf()).unwrap();
    let out = run(&src, "md");
    let (dir, ext) = &out.files[0];
    assert_eq!(ext, "", "image → folder");
    let md = std::fs::read_to_string(dir.join("sample.md")).unwrap();
    assert!(md.contains("# Report title"), "{md}");
    assert!(
        md.contains("[Plain body text here](https://example.org/report)"),
        "{md}"
    );
    assert!(md.contains("*Slanted words*"), "{md}");
    assert!(md.contains("media/"), "{md}");
    let get = |n: &str| {
        out.report
            .metrics
            .iter()
            .find(|m| m.name == n)
            .map(|m| (m.before, m.after))
    };
    assert_eq!(get("Images"), Some((1, 1)), "{:#?}", out.report);
    assert_eq!(get("Links"), Some((1, 1)));
    assert!(
        out.report.text_similarity.unwrap() >= 0.99,
        "{:#?}",
        out.report
    );
    assert!(
        get("Headings").is_none(),
        "a PDF has no heading count of its own: {:#?}",
        out.report
    );
}

#[test]
fn docx_and_scanned_pages() {
    let d = tempfile::TempDir::new().unwrap();
    let src = d.path().join("sample.pdf");
    std::fs::write(&src, fixture::sample_pdf()).unwrap();
    let docx = run(&src, "docx");
    assert!(std::fs::read(&docx.files[0].0).unwrap().starts_with(b"PK"));
    let m = |n: &str| {
        docx.report
            .metrics
            .iter()
            .find(|m| m.name == n)
            .map(|m| (m.before, m.after))
    };
    assert_eq!(m("Images"), Some((1, 1)), "{:#?}", docx.report);
    assert_eq!(m("Links"), Some((1, 1)), "{:#?}", docx.report);
    let body = zip_part(&docx.files[0].0, "word/document.xml");
    assert!(
        body.contains(r#"<w:pStyle w:val="Heading1" />"#),
        "title is a Word heading"
    );
    let scan = d.path().join("scan.pdf");
    std::fs::write(&scan, fixture::scanned_pdf()).unwrap();
    let out = run(&scan, "docx");
    assert!(
        out.report
            .warnings
            .iter()
            .any(|w| w.contains("no text layer")),
        "{:?}",
        out.report.warnings
    );
    let get = |n: &str| {
        out.report
            .metrics
            .iter()
            .find(|m| m.name == n)
            .map(|m| (m.before, m.after))
    };
    assert_eq!(get("Images"), Some((1, 1)), "{:#?}", out.report);
}

#[test]
fn tool_info() {
    let t = PdfConvert::new(Arc::new(NativePandoc::find().unwrap()));
    assert!(t.accepts(&FileKind::Pdf) && !t.accepts(&FileKind::Docx));
    let i = t.info();
    assert!(i.best_effort);
    assert_eq!(i.targets, ["docx", "odt", "md", "html", "epub", "txt"]);
}

#[test]
fn tables_notes_and_running_text_in_markdown() {
    let d = tempfile::TempDir::new().unwrap();
    let src = d.path().join("notes.pdf");
    std::fs::write(&src, fixture::structured_pdf()).unwrap();
    let out = run(&src, "md");
    let md = std::fs::read_to_string(&out.files[0].0).unwrap();
    assert!(md.contains("water.[^1]"), "{md}");
    assert!(md.contains("[^1]: Readings were taken twice."), "{md}");
    assert!(md.contains("March") && md.contains("2.9"), "{md}");
    assert!(
        !md.contains("**Month**"),
        "header cells aren't bold text: {md}"
    );
    assert!(
        !md.contains("Survey notes") && !md.contains("Page 1"),
        "{md}"
    );
    let get = |n: &str| {
        out.report
            .metrics
            .iter()
            .find(|m| m.name == n)
            .map(|m| (m.before, m.after))
    };
    assert_eq!(get("Tables"), Some((1, 1)), "{:#?}", out.report);
    assert_eq!(get("Table cells"), Some((6, 6)), "{:#?}", out.report);
    assert_eq!(get("Footnotes"), Some((1, 1)), "{:#?}", out.report);
    assert!(
        out.report
            .warnings
            .iter()
            .any(|w| w.contains("Survey notes") && w.contains("page numbers")),
        "{:#?}",
        out.report.warnings
    );
    assert_eq!(out.report.text_similarity, Some(1.0), "{:#?}", out.report);
    // Leaving the header text out of Markdown is a real change: ⚠.
    assert_eq!(out.report.verdict(), fo_types::Verdict::Warn);
}

fn zip_part(docx: &Path, name: &str) -> String {
    let mut z = zip::ZipArchive::new(std::fs::File::open(docx).unwrap()).unwrap();
    let mut s = String::new();
    std::io::Read::read_to_string(&mut z.by_name(name).unwrap(), &mut s).unwrap();
    s
}

#[test]
fn docx_is_written_directly_with_fonts_colours_notes_and_running_text() {
    let d = tempfile::TempDir::new().unwrap();
    let src = d.path().join("notes.pdf");
    std::fs::write(&src, fixture::structured_pdf()).unwrap();
    let out = run(&src, "docx");
    let docx = &out.files[0].0;
    let get = |n: &str| {
        out.report
            .metrics
            .iter()
            .find(|m| m.name == n)
            .map(|m| (m.before, m.after))
    };
    assert_eq!(get("Tables"), Some((1, 1)), "{:#?}", out.report);
    assert_eq!(get("Table cells"), Some((6, 6)), "{:#?}", out.report);
    assert_eq!(get("Footnotes"), Some((1, 1)), "{:#?}", out.report);
    assert_eq!(out.report.text_similarity, Some(1.0), "{:#?}", out.report);
    assert_eq!(
        out.report.verdict(),
        fo_types::Verdict::Exact,
        "{:#?}",
        out.report
    );
    let body = zip_part(docx, "word/document.xml");
    assert!(body.contains(r#"w:ascii="Helvetica""#), "font kept");
    assert!(body.contains(r#"<w:sz w:val="22" />"#), "11pt kept");
    assert!(
        body.contains(r#"<w:color w:val="003399" />"#),
        "colour kept"
    );
    assert!(zip_part(docx, "word/header1.xml").contains("Survey notes"));
    assert!(zip_part(docx, "word/footer1.xml").contains("PAGE"));
    assert!(zip_part(docx, "word/footnotes.xml").contains("Readings were taken twice."));
}

/// A table with a cell spanning two columns and one spanning two rows
/// keeps both spans through DOCX (read back by Pandoc).
#[test]
fn docx_table_keeps_merged_cells() {
    use fo_core::pdfconv::docx::{write_docx, PageSetup};
    use fo_core::pdfconv::layout::{Block, BlockKind, Cell, Doc, Line, Span, Table};
    let cell = |col, colspan, rowspan, t: &str| Cell {
        col,
        colspan,
        rowspan,
        lines: vec![Line {
            spans: vec![Span {
                text: t.into(),
                size: 11.0,
                ..Span::default()
            }],
            ..Line::default()
        }],
    };
    let t = Table {
        rows: vec![
            vec![cell(0, 1, 2, "Region"), cell(1, 2, 1, "Seasons")],
            vec![cell(1, 1, 1, "Spring"), cell(2, 1, 1, "Summer")],
            vec![
                cell(0, 1, 1, "North"),
                cell(1, 1, 1, "12"),
                cell(2, 1, 1, "140"),
            ],
        ],
        cols: 3,
        header_rows: 0,
    };
    let doc = Doc {
        blocks: vec![Block {
            kind: BlockKind::Table(t),
            lines: vec![],
        }],
        ..Doc::default()
    };
    let bytes = write_docx(
        &doc,
        &[],
        &PageSetup {
            width: 595.0,
            height: 842.0,
        },
    )
    .unwrap();
    use fo_core::pandoc::{PandocEngine, PandocRequest};
    let r = NativePandoc::find()
        .unwrap()
        .run(
            PandocRequest {
                options: serde_json::json!({"from": "docx", "to": "html", "input-files": ["t.docx"]}),
                files: vec![("t.docx".into(), bytes)],
            },
            &CancelToken::new(),
        )
        .unwrap();
    let html = String::from_utf8_lossy(&r.stdout);
    assert!(
        html.contains(r#"rowspan="2""#) && html.contains("Region"),
        "{html}"
    );
    assert!(
        html.contains(r#"colspan="2""#) && html.contains("Seasons"),
        "{html}"
    );
    assert!(html.contains("140"), "{html}");
}

/// Paragraphs get space after them, headings space before, and headings
/// keep the PDF's bold on their runs (complex scripts need it there).
#[test]
fn docx_spacing_and_heading_bold() {
    let d = tempfile::TempDir::new().unwrap();
    let src = d.path().join("sample.pdf");
    std::fs::write(&src, fixture::sample_pdf()).unwrap();
    let out = run(&src, "docx");
    let body = zip_part(&out.files[0].0, "word/document.xml");
    let heading = &body[body.find("Heading1").unwrap()..];
    let heading = &heading[..heading.find("</w:p>").unwrap()];
    assert!(heading.contains("<w:b />"), "{heading}");
    assert!(heading.contains("w:before="), "{heading}");
    let para = &body[body.find("Slanted").unwrap().saturating_sub(600)..];
    assert!(para.contains("w:after="), "{para}");
}
