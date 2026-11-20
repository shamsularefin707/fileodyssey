use fo_core::data::DataConvert;
use fo_core::tool::{CancelToken, Ctx, Tool, ToolError, ToolOutput};
use fo_types::{FileKind, Verdict};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn run(input: &Path, target: &str) -> Result<ToolOutput, ToolError> {
    let work = input.parent().unwrap().join(format!("w-{target}"));
    std::fs::create_dir_all(&work).unwrap();
    let cancel = CancelToken::new();
    let opts = serde_json::json!({});
    DataConvert.run(
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

fn write(d: &TempDir, n: &str, b: &[u8]) -> PathBuf {
    let p = d.path().join(n);
    std::fs::write(&p, b).unwrap();
    p
}

const CSV: &[u8] =
    b"\xEF\xBB\xBFname,note,code\nRumana,\"likes, commas\",007\nTanvir,\"two\nlines\",12.5\n";

#[test]
fn csv_to_json_keeps_every_cell_as_text() {
    let d = TempDir::new().unwrap();
    let out = run(&write(&d, "people.csv", CSV), "json").unwrap();
    let v: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&out.files[0].0).unwrap()).unwrap();
    assert_eq!(v[0]["name"], "Rumana");
    assert_eq!(v[0]["note"], "likes, commas");
    assert_eq!(v[0]["code"], "007", "leading zeros survive");
    assert_eq!(v[1]["note"], "two\nlines");
    assert_eq!(out.report.verdict(), Verdict::Exact);
    let rows = out
        .report
        .metrics
        .iter()
        .find(|m| m.name == "Rows")
        .unwrap();
    assert_eq!((rows.before, rows.after), (2, 2));
}

#[test]
fn semicolon_csv_and_json_back_to_csv() {
    let d = TempDir::new().unwrap();
    let out = run(&write(&d, "eu.csv", b"a;b\n1,5;x\n"), "json").unwrap();
    let json = std::fs::read(&out.files[0].0).unwrap();
    let v: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(v[0]["a"], "1,5");
    let back = run(&write(&d, "eu.json", &json), "csv").unwrap();
    assert_eq!(
        std::fs::read_to_string(&back.files[0].0).unwrap(),
        "a,b\n\"1,5\",x\n"
    );
}

#[test]
fn nested_json_is_written_as_text_with_a_warning() {
    let d = TempDir::new().unwrap();
    let out = run(
        &write(
            &d,
            "n.json",
            br#"[{"a":1,"b":{"c":2}},{"a":true,"d":null}]"#,
        ),
        "csv",
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(&out.files[0].0).unwrap(),
        "a,b,d\n1,\"{\"\"c\"\":2}\",\ntrue,,\n"
    );
    assert!(
        out.report.warnings.iter().any(|w| w.contains("nested")),
        "{:?}",
        out.report.warnings
    );
    let e = run(&write(&d, "o.json", br#"{"not":"a list"}"#), "csv")
        .err()
        .unwrap();
    assert!(matches!(e, ToolError::Failed(m) if m.contains("list")));
}

#[test]
fn csv_to_xlsx_opens_with_the_same_cells() {
    use calamine::{open_workbook_auto, Data, Reader};
    let d = TempDir::new().unwrap();
    let out = run(&write(&d, "people.csv", CSV), "xlsx").unwrap();
    let mut wb = open_workbook_auto(&out.files[0].0).unwrap();
    let sheet = wb.worksheet_range_at(0).unwrap().unwrap();
    assert_eq!(
        sheet.get_value((1, 0)),
        Some(&Data::String("Rumana".into()))
    );
    assert_eq!(
        sheet.get_value((1, 2)),
        Some(&Data::String("007".into())),
        "007 stays text"
    );
    assert_eq!(
        sheet.get_value((2, 2)),
        Some(&Data::Float(12.5)),
        "plain numbers become numbers"
    );
    assert!(DataConvert.accepts(&FileKind::Csv) && DataConvert.accepts(&FileKind::Json));
}
