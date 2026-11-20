//! `data.convert`: CSV ↔ JSON and CSV → Excel, keeping every cell.
//!
//! Cells stay text (so `007` keeps its zeros); only in Excel do values that
//! read back identically as numbers become numbers.

use crate::reader::text::decode_text;
use crate::tool::{Ctx, Tool, ToolError, ToolOutput};
use fo_types::{Category, FidelityReport, FileKind, Metric, Platform, ToolInfo};
use serde_json::{Map, Value};
use std::path::Path;

pub struct DataConvert;

type Table = (Vec<String>, Vec<Vec<String>>);

fn fail(m: impl Into<String>) -> ToolError {
    ToolError::Failed(m.into())
}

/// The delimiter used most on the first line: comma, semicolon or tab.
fn sniff(text: &str) -> u8 {
    let first = text.lines().next().unwrap_or("");
    (*b",;\t")
        .into_iter()
        .max_by_key(|d| first.bytes().filter(|b| b == d).count())
        .unwrap_or(b',')
}

fn read_csv(path: &Path) -> Result<Table, ToolError> {
    let text = decode_text(&std::fs::read(path)?);
    let mut r = csv::ReaderBuilder::new()
        .delimiter(sniff(&text))
        .flexible(true)
        .has_headers(false)
        .from_reader(text.as_bytes());
    let mut rows: Vec<Vec<String>> = Vec::new();
    for rec in r.records() {
        let rec = rec.map_err(|e| fail(format!("This CSV file is damaged: {e}")))?;
        rows.push(rec.iter().map(str::to_owned).collect());
    }
    if rows.is_empty() {
        return Err(fail("This CSV file is empty"));
    }
    let mut header = rows.remove(0);
    let width = rows
        .iter()
        .map(Vec::len)
        .chain([header.len()])
        .max()
        .unwrap_or(0);
    header.resize(width, String::new());
    // Every column needs a unique, non-empty name to become a JSON key.
    let mut seen = std::collections::HashSet::new();
    for (i, h) in header.iter_mut().enumerate() {
        if h.trim().is_empty() || !seen.insert(h.clone()) {
            *h = format!("column {}", i + 1);
            seen.insert(h.clone());
        }
    }
    Ok((header, rows))
}

fn cell_text(v: &Value, nested: &mut usize) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => {
            *nested += 1;
            other.to_string()
        }
    }
}

fn read_json(path: &Path) -> Result<(Table, usize), ToolError> {
    let v: Value = serde_json::from_str(&decode_text(&std::fs::read(path)?))
        .map_err(|e| fail(format!("This JSON file is damaged: {e}")))?;
    let Value::Array(items) = v else {
        return Err(fail("To make a table, the JSON must be a list of records"));
    };
    let mut nested = 0;
    if items.iter().all(Value::is_array) {
        let rows: Vec<Vec<String>> = items
            .iter()
            .map(|r| {
                r.as_array()
                    .into_iter()
                    .flatten()
                    .map(|c| cell_text(c, &mut nested))
                    .collect()
            })
            .collect();
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        let header = (1..=width).map(|i| format!("column {i}")).collect();
        return Ok(((header, rows), nested));
    }
    let mut header: Vec<String> = Vec::new();
    for it in &items {
        let Value::Object(o) = it else {
            return Err(fail(
                "To make a table, every item in the JSON list must be a record",
            ));
        };
        for k in o.keys() {
            if !header.contains(k) {
                header.push(k.clone());
            }
        }
    }
    let rows = items
        .iter()
        .map(|it| {
            let o = it.as_object().expect("checked above");
            header
                .iter()
                .map(|h| {
                    o.get(h)
                        .map(|c| cell_text(c, &mut nested))
                        .unwrap_or_default()
                })
                .collect()
        })
        .collect();
    Ok(((header, rows), nested))
}

fn cells(t: &Table) -> u64 {
    t.1.iter().flatten().filter(|c| !c.is_empty()).count() as u64
}

/// A cell Excel can hold as a number without changing how it reads.
fn as_number(s: &str) -> Option<f64> {
    let n: f64 = s.parse().ok()?;
    (n.is_finite() && s.len() < 16 && n.to_string() == s).then_some(n)
}

fn sheet_name(stem: &str) -> String {
    let clean: String = stem
        .chars()
        .filter(|c| !"[]:*?/\\".contains(*c))
        .take(31)
        .collect();
    if clean.trim().is_empty() {
        "Sheet1".into()
    } else {
        clean
    }
}

impl Tool for DataConvert {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            id: "data.convert".into(),
            label: "Convert table".into(),
            category: Category::Convert,
            targets: vec!["json".into(), "csv".into(), "xlsx".into()],
            best_effort: false,
            platforms: vec![Platform::Desktop, Platform::Android],
            options: vec![],
            combines: false,
        }
    }

    fn info_for(&self, kind: &FileKind) -> ToolInfo {
        let mut i = self.info();
        i.targets = match kind {
            FileKind::Csv => vec!["json".into(), "xlsx".into()],
            _ => vec!["csv".into()],
        };
        i
    }

    fn accepts(&self, kind: &FileKind) -> bool {
        matches!(kind, FileKind::Csv | FileKind::Json)
    }

    fn run(&self, input: &Path, ctx: &Ctx) -> Result<ToolOutput, ToolError> {
        let kind = crate::detect::detect_file(input)?.kind;
        let target = ctx.target.unwrap_or("");
        let (table, nested) = match kind {
            FileKind::Csv => (read_csv(input)?, 0),
            FileKind::Json => read_json(input)?,
            _ => return Err(fail("This tool converts CSV and JSON tables")),
        };
        let out = ctx.work_dir.join(format!("out.{target}"));
        match (kind, target) {
            (FileKind::Csv, "json") => {
                let records: Vec<Value> = table
                    .1
                    .iter()
                    .map(|r| {
                        let mut o = Map::new();
                        for (i, h) in table.0.iter().enumerate() {
                            o.insert(
                                h.clone(),
                                Value::String(r.get(i).cloned().unwrap_or_default()),
                            );
                        }
                        Value::Object(o)
                    })
                    .collect();
                let mut s =
                    serde_json::to_string_pretty(&records).map_err(|e| fail(e.to_string()))?;
                s.push('\n');
                std::fs::write(&out, s)?;
            }
            (FileKind::Json, "csv") => {
                let mut w = csv::Writer::from_path(&out).map_err(|e| fail(e.to_string()))?;
                w.write_record(&table.0).map_err(|e| fail(e.to_string()))?;
                for r in &table.1 {
                    w.write_record(r).map_err(|e| fail(e.to_string()))?;
                }
                w.flush()?;
            }
            (FileKind::Csv, "xlsx") => {
                let mut wb = rust_xlsxwriter::Workbook::new();
                let bold = rust_xlsxwriter::Format::new().set_bold();
                let stem = input
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let ws = wb.add_worksheet();
                let x = |e: rust_xlsxwriter::XlsxError| fail(e.to_string());
                ws.set_name(sheet_name(&stem)).map_err(x)?;
                for (c, h) in table.0.iter().enumerate() {
                    ws.write_string_with_format(0, c as u16, h, &bold)
                        .map_err(x)?;
                }
                for (r, row) in table.1.iter().enumerate() {
                    for (c, v) in row.iter().enumerate() {
                        let (r, c) = (r as u32 + 1, c as u16);
                        match as_number(v) {
                            Some(n) => ws.write_number(r, c, n).map_err(x)?,
                            None => ws.write_string(r, c, v).map_err(x)?,
                        };
                    }
                }
                wb.save(&out).map_err(x)?;
            }
            _ => return Err(fail(format!("Can't convert this table to {target}"))),
        }
        let n = |name: &str, v: u64| Metric {
            name: name.into(),
            before: v,
            after: v,
            informational: false,
        };
        let mut warnings = Vec::new();
        if nested > 0 {
            warnings.push(format!("{nested} nested values were written as JSON text"));
        }
        let report = FidelityReport {
            metrics: vec![n("Rows", table.1.len() as u64), n("Cells", cells(&table))],
            text_similarity: None,
            warnings,
        };
        Ok(ToolOutput {
            files: vec![(out, target.to_owned())],
            report,
        })
    }
}
