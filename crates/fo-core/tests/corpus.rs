//! Golden corpus gate (spec §7.2–7.3).
//!
//! Every file in `tests/corpus/` is converted to every target it offers
//! with the native Pandoc that matches the app's pandoc.wasm. The fidelity
//! of each pair is compared with `tests/corpus/baseline.json`: similarity
//! and every element count must not drop, and a pair that was exact must
//! stay exact. `FO_UPDATE_BASELINE=1 cargo test --test corpus` rewrites the
//! baseline after an intended change; review the diff before committing.

use fo_core::detect::detect_file;
use fo_core::pandoc::ast::{stats, Stats};
use fo_core::pandoc::formats::targets_for;
use fo_core::pandoc::tool::DocConvert;
use fo_core::pandoc::{NativePandoc, PandocEngine, PandocRequest};
use fo_core::tool::{CancelToken, Ctx, Tool};
use fo_types::Verdict;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus")
}

fn engine() -> Arc<NativePandoc> {
    Arc::new(NativePandoc::find().expect("native pandoc (scripts/fetch-pandoc.sh --native)"))
}

fn sources() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(corpus())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.is_file() && p.extension().is_some_and(|e| e != "json") && !p.ends_with("README.md")
        })
        .collect();
    v.sort();
    v
}

fn convert(
    tool: &DocConvert,
    src: &Path,
    target: &str,
    work: &Path,
) -> Result<fo_core::tool::ToolOutput, String> {
    std::fs::create_dir_all(work).unwrap();
    let cancel = CancelToken::new();
    let opts = json!({});
    let ctx = Ctx {
        work_dir: work,
        target: Some(target),
        options: &opts,
        cancel: &cancel,
        progress: &|_| {},
    };
    tool.run(src, &ctx).map_err(|e| format!("{e:?}"))
}

#[test]
fn corpus_meets_baseline() {
    let tool = Arc::new(DocConvert::new(engine()));
    let tmp = tempfile::TempDir::new().unwrap();
    let mut jobs = Vec::new();
    for src in sources() {
        let kind = detect_file(&src).unwrap().kind;
        for t in targets_for(kind) {
            jobs.push((src.clone(), t));
        }
    }
    assert!(jobs.len() >= 100, "corpus covers {} pairs", jobs.len());
    let results = Mutex::new(BTreeMap::<String, Value>::new());
    let next = Mutex::new(jobs.into_iter().enumerate());
    std::thread::scope(|s| {
        for _ in 0..6 {
            s.spawn(|| loop {
                let Some((i, (src, t))) = next.lock().unwrap().next() else { break };
                let key = format!("{} -> {t}", src.file_name().unwrap().to_string_lossy());
                let entry = match convert(&tool, &src, t, &tmp.path().join(i.to_string())) {
                    Ok(out) => {
                        let mut metrics = Map::new();
                        for m in &out.report.metrics {
                            metrics.insert(m.name.clone(), json!([m.before, m.after]));
                        }
                        json!({
                            "similarity": (out.report.text_similarity.unwrap_or(1.0) * 10000.0).round() / 10000.0,
                            "metrics": metrics,
                            "exact": out.report.verdict() == Verdict::Exact,
                        })
                    }
                    Err(e) => json!({"error": e}),
                };
                results.lock().unwrap().insert(key, entry);
            });
        }
    });
    let results = results.into_inner().unwrap();
    let path = corpus().join("baseline.json");
    if std::env::var_os("FO_UPDATE_BASELINE").is_some() {
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&results).unwrap() + "\n",
        )
        .unwrap();
    }
    let baseline: BTreeMap<String, Value> =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("baseline.json")).unwrap();
    let mut problems = Vec::new();
    let mut below_bar = Vec::new();
    for (key, now) in &results {
        if let Some(e) = now.get("error") {
            problems.push(format!("{key}: failed: {e}"));
            continue;
        }
        let sim = now["similarity"].as_f64().unwrap();
        if sim < 0.99 && !key.ends_with("-> txt") {
            below_bar.push(format!("{key} {sim:.4}"));
        }
        let Some(base) = baseline.get(key) else {
            problems.push(format!(
                "{key}: not in baseline (run with FO_UPDATE_BASELINE=1)"
            ));
            continue;
        };
        if sim + 1e-9 < base["similarity"].as_f64().unwrap_or(0.0) {
            problems.push(format!(
                "{key}: similarity {sim} < baseline {}",
                base["similarity"]
            ));
        }
        if base["exact"] == true && now["exact"] != true {
            problems.push(format!("{key}: was exact, now ⚠"));
        }
        for (name, ba) in base["metrics"].as_object().into_iter().flatten() {
            let b_after = ba[1].as_u64().unwrap_or(0);
            let n_after = now["metrics"]
                .get(name)
                .and_then(|v| v[1].as_u64())
                .unwrap_or(0);
            if n_after < b_after {
                problems.push(format!("{key}: {name} {n_after} < baseline {b_after}"));
            }
        }
    }
    eprintln!(
        "{} pairs; below 99% similarity (excluding → txt): {}",
        results.len(),
        below_bar.len()
    );
    for b in &below_bar {
        eprintln!("  {b}");
    }
    assert!(
        problems.is_empty(),
        "corpus regressions:\n{}",
        problems.join("\n")
    );
}

/// Normalised words of a file read by Pandoc.
fn words(engine: &dyn PandocEngine, from: &str, name: &str, bytes: Vec<u8>) -> Stats {
    let r = engine
        .run(
            PandocRequest {
                options: json!({"from": from, "to": "json", "input-files": [name]}),
                files: vec![(name.into(), bytes)],
            },
            &CancelToken::new(),
        )
        .unwrap();
    stats(&serde_json::from_slice(&r.stdout).unwrap())
}

fn round_trip(src: &str, from: &str, back_target: &str, back_reader: &str) {
    let e = engine();
    let tool = DocConvert::new(e.clone());
    let tmp = tempfile::TempDir::new().unwrap();
    let source = corpus().join(src);
    let docx = convert(&tool, &source, "docx", &tmp.path().join("a")).unwrap();
    let docx_file = tmp.path().join("mid.docx");
    std::fs::copy(&docx.files[0].0, &docx_file).unwrap();
    let back = convert(&tool, &docx_file, back_target, &tmp.path().join("b")).unwrap();
    let (out, ext) = &back.files[0];
    let file = if ext.is_empty() {
        out.join(format!("mid.{back_target}"))
    } else {
        out.clone()
    };
    let original = words(&*e, from, src, std::fs::read(&source).unwrap());
    let returned = words(&*e, back_reader, "back", std::fs::read(&file).unwrap());
    // Every word comes back, in order. Standalone writers may add a title
    // block, so extra copies of the title are not a failure.
    let kept = fo_core::pandoc::ast::similarity(&original.words, &returned.words);
    assert_eq!(
        kept, 1.0,
        "{src}: words lost on the round trip\n{:?}\n{:?}",
        original.words, returned.words
    );
    assert_eq!(
        (
            original.tables,
            original.cells,
            original.links,
            original.footnotes,
            original.images
        ),
        (
            returned.tables,
            returned.cells,
            returned.links,
            returned.footnotes,
            returned.images
        ),
        "{src}: structure changed on the round trip"
    );
    assert!(
        returned.headings >= original.headings,
        "{src}: headings lost"
    );
}

#[test]
fn markdown_round_trips_through_docx() {
    round_trip("notes.md", "markdown", "md", "markdown");
}

#[test]
fn html_round_trips_through_docx() {
    round_trip("article.html", "html", "html", "html");
}
