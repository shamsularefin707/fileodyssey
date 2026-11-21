//! office.convert with the real LibreOffice. Each test prints "skipped"
//! and passes when LibreOffice isn't installed (CI without it).
use fo_core::libre::{convert, find_soffice, tool::OfficeConvert};
use fo_core::pandoc::{NativePandoc, PandocEngine, PandocRequest};
use fo_core::tool::{CancelToken, Ctx, Tool, ToolError, ToolOutput};
use fo_types::{FidelityReport, Verdict};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

fn corpus(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/corpus")
        .join(name)
}

fn setup() -> Option<(OfficeConvert, PathBuf, tempfile::TempDir)> {
    let Some(soffice) = find_soffice() else {
        eprintln!("skipped: LibreOffice isn't installed");
        return None;
    };
    let pandoc = Arc::new(NativePandoc::find().expect("native pandoc"));
    Some((
        OfficeConvert::new(pandoc),
        soffice,
        tempfile::TempDir::new().unwrap(),
    ))
}

fn run(t: &OfficeConvert, input: &Path, target: &str, d: &Path) -> Result<ToolOutput, ToolError> {
    let work = d.join(format!(
        "work-{target}-{}",
        input.file_name().unwrap().to_string_lossy()
    ));
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
}

fn metric(r: &FidelityReport, name: &str) -> Option<(u64, u64)> {
    r.metrics
        .iter()
        .find(|m| m.name == name)
        .map(|m| (m.before, m.after))
}

/// A copy of `src` converted by LibreOffice itself (to make .doc / .ods).
fn lo_made(soffice: &Path, src: &Path, ext: &str, d: &Path) -> PathBuf {
    let work = d.join(format!("make-{ext}"));
    let out = convert(
        soffice,
        src,
        ext,
        ext,
        &work,
        &CancelToken::new(),
        Duration::from_secs(120),
    )
    .unwrap();
    let dest = d.join(format!("made.{ext}"));
    std::fs::copy(out, &dest).unwrap();
    dest
}

#[test]
fn docx_to_pdf_keeps_every_word_and_image() {
    let Some((t, _, d)) = setup() else { return };
    let out = run(&t, &corpus("report.docx"), "pdf", d.path()).unwrap();
    assert!(std::fs::read(&out.files[0].0).unwrap().starts_with(b"%PDF"));
    let r = &out.report;
    assert_eq!(r.text_similarity, Some(1.0), "{r:#?}");
    assert_eq!(metric(r, "Images"), Some((1, 1)), "{r:#?}");
    assert!(r
        .metrics
        .iter()
        .any(|m| m.name == "Pages" && m.informational));
    assert_eq!(r.verdict(), Verdict::Exact, "{r:#?}");
}

#[test]
fn legacy_doc_to_docx_keeps_tables_and_images() {
    let Some((t, soffice, d)) = setup() else {
        return;
    };
    let doc = lo_made(&soffice, &corpus("report.docx"), "doc", d.path());
    assert_eq!(
        fo_core::detect::detect_kind(&doc).unwrap(),
        fo_types::FileKind::Doc
    );
    let out = run(&t, &doc, "docx", d.path()).unwrap();
    let r = &out.report;
    assert_eq!(metric(r, "Tables"), Some((1, 1)), "{r:#?}");
    assert_eq!(metric(r, "Images"), Some((1, 1)), "{r:#?}");
    assert_eq!(r.text_similarity, Some(1.0), "{r:#?}");
    assert!(
        r.warnings
            .iter()
            .any(|w| w.contains("Only LibreOffice can read")),
        "{r:#?}"
    );
}

#[test]
fn xlsx_to_pdf_keeps_every_cell() {
    let Some((t, _, d)) = setup() else { return };
    let mut wb = rust_xlsxwriter::Workbook::new();
    let ws = wb.add_worksheet();
    for (r, (m, n)) in [("March", 2.1), ("April", 2.9), ("May", 3.4)]
        .iter()
        .enumerate()
    {
        ws.write_string(r as u32, 0, *m).unwrap();
        ws.write_number(r as u32, 1, *n).unwrap();
    }
    ws.write_string(3, 0, "Highest level after rain").unwrap();
    let p = d.path().join("levels.xlsx");
    wb.save(&p).unwrap();
    let out = run(&t, &p, "pdf", d.path()).unwrap();
    assert_eq!(out.report.text_similarity, Some(1.0), "{:#?}", out.report);
}

#[test]
fn pptx_to_pdf_keeps_the_slides_text() {
    let Some((t, _, d)) = setup() else { return };
    let md = "# River survey\n\nSeven markers along the lower river.\n\n# Results\n\n- Levels rose every month\n- Herons counted daily\n";
    let r = NativePandoc::find()
        .unwrap()
        .run(
            PandocRequest {
                options: serde_json::json!({"from": "markdown", "to": "pptx", "input-files": ["s.md"], "output-file": "s.pptx"}),
                files: vec![("s.md".into(), md.as_bytes().to_vec())],
            },
            &CancelToken::new(),
        )
        .unwrap();
    let pptx = r.files.into_iter().find(|(n, _)| n == "s.pptx").unwrap().1;
    let p = d.path().join("deck.pptx");
    std::fs::write(&p, pptx).unwrap();
    let out = run(&t, &p, "pdf", d.path()).unwrap();
    assert_eq!(out.report.text_similarity, Some(1.0), "{:#?}", out.report);
    assert_eq!(
        metric(&out.report, "Pages"),
        Some((2, 2)),
        "{:#?}",
        out.report
    );
}

#[test]
fn markdown_to_pdf_through_pandoc() {
    let Some((t, _, d)) = setup() else { return };
    let p = d.path().join("notes.md");
    std::fs::write(&p, "# Field notes\n\nThe fog lay on the water until **nine**.\n\n| Marker | Level |\n|---|---|\n| Mill | 2.1 |\n").unwrap();
    let out = run(&t, &p, "pdf", d.path()).unwrap();
    assert!(std::fs::read(&out.files[0].0).unwrap().starts_with(b"%PDF"));
    assert_eq!(out.report.text_similarity, Some(1.0), "{:#?}", out.report);
    assert_eq!(out.report.verdict(), Verdict::Exact, "{:#?}", out.report);
}

#[test]
fn ods_to_xlsx() {
    let Some((t, soffice, d)) = setup() else {
        return;
    };
    let mut wb = rust_xlsxwriter::Workbook::new();
    let ws = wb.add_worksheet();
    ws.write_string(0, 0, "Marker").unwrap();
    ws.write_string(0, 1, "Level").unwrap();
    ws.write_string(1, 0, "Mill").unwrap();
    ws.write_number(1, 1, 2.1).unwrap();
    let x = d.path().join("src.xlsx");
    wb.save(&x).unwrap();
    let ods = lo_made(&soffice, &x, "ods", d.path());
    assert_eq!(
        fo_core::detect::detect_kind(&ods).unwrap(),
        fo_types::FileKind::Ods
    );
    let out = run(&t, &ods, "xlsx", d.path()).unwrap();
    assert_eq!(out.report.text_similarity, Some(1.0), "{:#?}", out.report);
    assert_eq!(
        metric(&out.report, "Table cells"),
        Some((4, 4)),
        "{:#?}",
        out.report
    );
}

#[test]
fn a_broken_document_fails_clearly() {
    let Some((t, _, d)) = setup() else { return };
    let p = d.path().join("broken.docx");
    // A zip that claims to be a Word document but has no document in it.
    let mut z = zip::ZipWriter::new(std::fs::File::create(&p).unwrap());
    z.start_file(
        "[Content_Types].xml",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    std::io::Write::write_all(&mut z, b"<Types/>").unwrap();
    z.start_file("word/nothing.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    z.finish().unwrap();
    match run(&t, &p, "pdf", d.path()) {
        Err(ToolError::Failed(m)) => assert!(!m.is_empty(), "{m}"),
        Ok(o) => panic!("converted a broken file: {:#?}", o.report),
        Err(e) => panic!("{e:?}"),
    }
}
