//! PDF reconstruction corpus (spec §7.6).
//!
//! Each PDF in `tests/pdf-corpus/` was printed from a source in `src/`
//! (Chrome for HTML, reportlab for the `.md` ones), so the true content is
//! known. Each PDF is converted to Markdown and to DOCX and read back; each
//! result is compared with the source: words kept in order (text and
//! reading order), images, heading texts, table cells in the right row and
//! column, and footnotes. Results must not drop below `baseline.json`
//! (`FO_UPDATE_BASELINE=1` rewrites it); the §7.6 bar status is printed.

use fo_core::pandoc::ast::{similarity, stats, Stats};
use fo_core::pandoc::{NativePandoc, PandocEngine, PandocRequest};
use fo_core::pdfconv::tool::PdfConvert;
use fo_core::tool::{CancelToken, Ctx, Tool};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/pdf-corpus")
}

struct Read {
    stats: Stats,
    heads: Vec<String>,
    tables: Vec<Grid>,
}

/// A table as origin cells: (row, col) → normalised text.
type Grid = BTreeMap<(usize, usize), String>;

fn read(engine: &dyn PandocEngine, from: &str, name: &str, bytes: Vec<u8>) -> Read {
    let r = engine
        .run(
            PandocRequest {
                options: json!({"from": from, "to": "json", "standalone": true, "input-files": [name]}),
                files: vec![(name.into(), bytes)],
            },
            &CancelToken::new(),
        )
        .unwrap();
    let mut ast: Value = serde_json::from_slice(&r.stdout).unwrap();
    // Metadata (an HTML <title>) isn't printed on the page.
    ast["meta"] = json!({});
    Read {
        stats: stats(&ast),
        heads: heading_texts(&ast),
        tables: tables(&ast),
    }
}

fn words_of(blocks: &Value) -> String {
    stats(&json!({"blocks": blocks})).words.join(" ")
}

/// Every table's origin cells by grid position (spans expanded).
fn tables(ast: &Value) -> Vec<Grid> {
    fn walk(v: &Value, out: &mut Vec<Grid>) {
        match v {
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            Value::Object(o) => {
                if o.get("t").and_then(Value::as_str) == Some("Table") {
                    let c = &v["c"];
                    let mut rows: Vec<&Value> = Vec::new();
                    rows.extend(c[3][1].as_array().into_iter().flatten());
                    for b in c[4].as_array().into_iter().flatten() {
                        rows.extend(b[2].as_array().into_iter().flatten());
                        rows.extend(b[3].as_array().into_iter().flatten());
                    }
                    rows.extend(c[5][1].as_array().into_iter().flatten());
                    let mut g = Grid::new();
                    let mut taken: std::collections::HashSet<(usize, usize)> = Default::default();
                    for (r, row) in rows.iter().enumerate() {
                        let mut col = 0;
                        for cell in row[1].as_array().into_iter().flatten() {
                            while taken.contains(&(r, col)) {
                                col += 1;
                            }
                            let (rs, cs) = (
                                cell[2].as_u64().unwrap_or(1) as usize,
                                cell[3].as_u64().unwrap_or(1) as usize,
                            );
                            g.insert((r, col), words_of(&cell[4]));
                            for dr in 0..rs.max(1) {
                                for dc in 0..cs.max(1) {
                                    taken.insert((r + dr, col + dc));
                                }
                            }
                            col += cs.max(1);
                        }
                    }
                    out.push(g);
                } else if let Some(c) = o.get("c") {
                    walk(c, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(&ast["blocks"], &mut out);
    out
}

/// Share of the truth's table cells found at the same position with the
/// same text (tables paired in order); 1.0 when there are none.
fn cell_score(truth: &[Grid], got: &[Grid]) -> (usize, usize) {
    let total: usize = truth.iter().map(BTreeMap::len).sum();
    let right: usize = truth
        .iter()
        .zip(got)
        .map(|(t, g)| {
            for (k, v) in t {
                if g.get(k) != Some(v) {
                    eprintln!("  cell {k:?}: want {v:?}, got {:?}", g.get(k));
                }
            }
            t.iter().filter(|(k, v)| g.get(k) == Some(v)).count()
        })
        .sum();
    (right, total)
}

/// Text of every heading, normalised.
fn heading_texts(ast: &Value) -> Vec<String> {
    fn walk(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            Value::Object(o) => {
                if o.get("t").and_then(Value::as_str) == Some("Header") {
                    let words = stats(&json!({"blocks": [{"t": "Para", "c": v["c"][2]}]})).words;
                    out.push(words.join(" "));
                } else if let Some(c) = o.get("c") {
                    walk(c, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(&ast["blocks"], &mut out);
    out
}

#[test]
fn pdf_corpus_meets_baseline() {
    let engine = Arc::new(NativePandoc::find().expect("native pandoc"));
    let tool = PdfConvert::new(engine.clone());
    let tmp = tempfile::TempDir::new().unwrap();
    let mut pdfs: Vec<PathBuf> = std::fs::read_dir(dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "pdf"))
        .collect();
    pdfs.sort();
    assert!(pdfs.len() >= 9);
    let mut results = BTreeMap::new();
    let mut lines = Vec::new();
    for pdf in &pdfs {
        let stem = pdf.file_stem().unwrap().to_string_lossy().into_owned();
        let truth = {
            let html = dir().join("src").join(format!("{stem}.html"));
            if html.is_file() {
                read(&*engine, "html", "t.html", std::fs::read(html).unwrap())
            } else {
                read(
                    &*engine,
                    "markdown",
                    "t.md",
                    std::fs::read(dir().join("src").join(format!("{stem}.md"))).unwrap(),
                )
            }
        };
        let mut entry = serde_json::Map::new();
        for target in ["md", "docx"] {
            let work = tmp.path().join(format!("{stem}-{target}"));
            std::fs::create_dir_all(&work).unwrap();
            let cancel = CancelToken::new();
            let opts = json!({});
            let out = tool
                .run(
                    pdf,
                    &Ctx {
                        work_dir: &work,
                        target: Some(target),
                        options: &opts,
                        cancel: &cancel,
                        progress: &|_| {},
                    },
                )
                .unwrap_or_else(|e| panic!("{stem} → {target}: {e:?}"));
            let (p, ext) = &out.files[0];
            let file = if ext.is_empty() {
                p.join(format!("{stem}.{target}"))
            } else {
                p.clone()
            };
            let from = if target == "md" { "markdown" } else { "docx" };
            let got = read(
                &*engine,
                from,
                &format!("o.{target}"),
                std::fs::read(&file).unwrap(),
            );
            let sim =
                (similarity(&truth.stats.words, &got.stats.words) * 10000.0).round() / 10000.0;
            let heads_found = truth.heads.iter().filter(|h| got.heads.contains(h)).count();
            let (right, total) = cell_score(&truth.tables, &got.tables);
            let cells = if total == 0 {
                1.0
            } else {
                right as f64 / total as f64
            };
            let cells = (cells * 10000.0).round() / 10000.0;
            let notes_ok = got.stats.footnotes == truth.stats.footnotes;
            let bar = sim >= 0.99
                && got.stats.images == truth.stats.images
                && heads_found == truth.heads.len()
                && cells >= 0.9
                && notes_ok;
            lines.push(format!(
                "{stem:11} {target:4} text {:.2}%  images {}/{}  headings {}/{} (+{})  cells {right}/{total}  notes {}/{}  {}",
                sim * 100.0,
                got.stats.images,
                truth.stats.images,
                heads_found,
                truth.heads.len(),
                got.heads.len().saturating_sub(heads_found),
                got.stats.footnotes,
                truth.stats.footnotes,
                if bar { "meets bar" } else { "BELOW BAR" }
            ));
            entry.insert(
                target.into(),
                json!({"similarity": sim, "images": got.stats.images, "headings_found": heads_found, "cells": cells, "footnotes": got.stats.footnotes, "bar": bar}),
            );
        }
        results.insert(stem, Value::Object(entry));
    }
    eprintln!("{}", lines.join("\n"));
    let path = dir().join("baseline.json");
    if std::env::var_os("FO_UPDATE_BASELINE").is_some() {
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&results).unwrap() + "\n",
        )
        .unwrap();
    }
    let base: BTreeMap<String, Value> =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("baseline.json")).unwrap();
    let mut problems = Vec::new();
    for (k, now) in &results {
        for target in ["md", "docx"] {
            let (Some(b), Some(n)) = (base.get(k).and_then(|b| b.get(target)), now.get(target))
            else {
                problems.push(format!("{k} {target}: not in baseline"));
                continue;
            };
            for m in [
                "similarity",
                "images",
                "headings_found",
                "cells",
                "footnotes",
            ] {
                if n[m].as_f64().unwrap() + 1e-9 < b[m].as_f64().unwrap() {
                    problems.push(format!("{k} {target}: {m} {} < baseline {}", n[m], b[m]));
                }
            }
            if b["bar"] == true && n["bar"] != true {
                problems.push(format!("{k} {target}: met the §7.6 bar before, not now"));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "PDF corpus regressions:\n{}",
        problems.join("\n")
    );
}

/// The report's own check (against the PDF's text) must agree with the
/// ground truth: every corpus file that meets the bar reports full text.
#[test]
fn reports_agree_with_ground_truth() {
    let engine = Arc::new(NativePandoc::find().expect("native pandoc"));
    let tool = PdfConvert::new(engine);
    let tmp = tempfile::TempDir::new().unwrap();
    for stem in ["bangla", "footnotes", "report", "twocol"] {
        for target in ["md", "docx"] {
            let work = tmp.path().join(format!("{stem}-{target}"));
            std::fs::create_dir_all(&work).unwrap();
            let cancel = CancelToken::new();
            let opts = json!({});
            let out = tool
                .run(
                    &dir().join(format!("{stem}.pdf")),
                    &Ctx {
                        work_dir: &work,
                        target: Some(target),
                        options: &opts,
                        cancel: &cancel,
                        progress: &|_| {},
                    },
                )
                .unwrap();
            let sim = out.report.text_similarity.unwrap();
            assert!(sim >= 0.99, "{stem} → {target}: {sim} {:#?}", out.report);
        }
    }
}
