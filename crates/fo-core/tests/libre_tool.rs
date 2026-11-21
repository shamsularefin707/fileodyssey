//! office.convert: targets per kind and the unavailable state.
use fo_core::libre::tool::OfficeConvert;
use fo_core::pandoc::NativePandoc;
use fo_core::tool::{CancelToken, Ctx, Tool, ToolError};
use fo_types::{FileKind, Platform};
use std::path::PathBuf;
use std::sync::Arc;

fn tool(soffice: Option<PathBuf>) -> OfficeConvert {
    let pandoc = Arc::new(NativePandoc::find().expect("native pandoc"));
    OfficeConvert::new(pandoc).with_locator(Arc::new(move || soffice.clone()))
}

fn targets(t: &OfficeConvert, k: FileKind) -> Vec<String> {
    assert!(t.accepts(&k), "{k:?}");
    t.info_for(&k).targets
}

#[test]
fn targets_depend_on_the_source() {
    let t = tool(Some(PathBuf::from("/usr/bin/soffice")));
    assert_eq!(targets(&t, FileKind::Doc), ["docx", "odt", "rtf", "pdf"]);
    assert_eq!(targets(&t, FileKind::Docx), ["pdf"]);
    assert_eq!(targets(&t, FileKind::Xls), ["xlsx", "ods", "csv", "pdf"]);
    assert_eq!(targets(&t, FileKind::Xlsx), ["ods", "pdf"]);
    assert_eq!(targets(&t, FileKind::Ods), ["xlsx", "pdf"]);
    assert_eq!(targets(&t, FileKind::Ppt), ["pptx", "odp", "pdf"]);
    assert_eq!(targets(&t, FileKind::Pptx), ["odp", "pdf"]);
    assert_eq!(targets(&t, FileKind::Odp), ["pptx", "pdf"]);
    assert_eq!(targets(&t, FileKind::Markdown), ["pdf"]);
    assert!(!t.accepts(&FileKind::Pdf) && !t.accepts(&FileKind::Csv));
    let i = t.info();
    assert_eq!(i.platforms, vec![Platform::Desktop]);
    assert!(i.needs.is_none());
    assert!(!i.best_effort);
}

#[test]
fn without_libreoffice_the_tool_says_what_it_needs() {
    let t = tool(None);
    let n = t.info_for(&FileKind::Doc).needs.expect("needs LibreOffice");
    assert_eq!(n.message, "Install LibreOffice to enable");
    assert!(n.url.starts_with("https://www.libreoffice.org/download"));
    let d = tempfile::TempDir::new().unwrap();
    let input = d.path().join("a.docx");
    std::fs::write(&input, b"PK").unwrap();
    let cancel = CancelToken::new();
    let opts = serde_json::json!({});
    let Err(e) = t.run(
        &input,
        &Ctx {
            work_dir: d.path(),
            target: Some("pdf"),
            options: &opts,
            cancel: &cancel,
            progress: &|_| {},
        },
    ) else {
        panic!("ran without LibreOffice");
    };
    match e {
        ToolError::Failed(m) => assert!(m.contains("Install LibreOffice"), "{m}"),
        other => panic!("{other:?}"),
    }
}
