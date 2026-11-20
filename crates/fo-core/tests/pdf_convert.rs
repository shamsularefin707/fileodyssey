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
