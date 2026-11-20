//! PDF reconstruction corpus (spec §7.6).
//!
//! Each PDF in `tests/pdf-corpus/` was printed from a source in `src/`
//! (Chrome for HTML, reportlab for `fieldnotes`), so the true content is
//! known. Each PDF is converted to Markdown and read back; the result is
//! compared with the source: words kept in order (text and reading order),
//! images, and heading texts. Results must not drop below `baseline.json`
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

fn read(engine: &dyn PandocEngine, from: &str, name: &str, bytes: Vec<u8>) -> (Stats, Vec<String>) {
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
    (stats(&ast), heading_texts(&ast))
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
    assert!(pdfs.len() >= 7);
    let mut results = BTreeMap::new();
    let mut lines = Vec::new();
    for pdf in &pdfs {
        let stem = pdf.file_stem().unwrap().to_string_lossy().into_owned();
        let (truth, truth_heads) = {
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
        let work = tmp.path().join(&stem);
        std::fs::create_dir_all(&work).unwrap();
        let cancel = CancelToken::new();
        let opts = json!({});
        let out = tool
            .run(
                pdf,
                &Ctx {
                    work_dir: &work,
                    target: Some("md"),
                    options: &opts,
                    cancel: &cancel,
                    progress: &|_| {},
                },
            )
            .unwrap_or_else(|e| panic!("{stem}: {e:?}"));
        let (p, ext) = &out.files[0];
        let md = if ext.is_empty() {
            p.join(format!("{stem}.md"))
        } else {
            p.clone()
        };
        let (got, got_heads) = read(&*engine, "markdown", "o.md", std::fs::read(&md).unwrap());
        let sim = (similarity(&truth.words, &got.words) * 10000.0).round() / 10000.0;
        let heads_found = truth_heads.iter().filter(|h| got_heads.contains(h)).count();
        let bar = sim >= 0.99 && got.images == truth.images && heads_found == truth_heads.len();
        lines.push(format!(
            "{stem:12} text {:.2}%  images {}/{}  headings {}/{} (+{} extra)  {}",
            sim * 100.0,
            got.images,
            truth.images,
            heads_found,
            truth_heads.len(),
            got_heads.len().saturating_sub(heads_found),
            if bar { "meets bar" } else { "BELOW BAR" }
        ));
        results.insert(
            stem,
            json!({"similarity": sim, "images": got.images, "truth_images": truth.images, "headings_found": heads_found, "truth_headings": truth_heads.len(), "bar": bar}),
        );
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
        let Some(b) = base.get(k) else {
            problems.push(format!("{k}: not in baseline"));
            continue;
        };
        for m in ["similarity", "images", "headings_found"] {
            if now[m].as_f64().unwrap() + 1e-9 < b[m].as_f64().unwrap() {
                problems.push(format!("{k}: {m} {} < baseline {}", now[m], b[m]));
            }
        }
        if b["bar"] == true && now["bar"] != true {
            problems.push(format!("{k}: met the §7.6 bar before, not now"));
        }
    }
    assert!(
        problems.is_empty(),
        "PDF corpus regressions:\n{}",
        problems.join("\n")
    );
}
