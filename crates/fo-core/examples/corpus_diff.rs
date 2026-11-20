//! Developer aid: `cargo run -p fo-core --example corpus_diff -- tests/corpus/notes.md rtf`
//! prints the report and the word-level differences for one conversion.
use fo_core::pandoc::ast::{normalise_words, stats, strip_note_markers};
use fo_core::pandoc::formats::reader_for_target;
use fo_core::pandoc::tool::DocConvert;
use fo_core::pandoc::{NativePandoc, PandocEngine, PandocRequest};
use fo_core::tool::{CancelToken, Ctx, Tool};
use std::sync::Arc;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (src, target) = (std::path::PathBuf::from(&a[0]), a[1].as_str());
    let e = Arc::new(NativePandoc::find().expect("pandoc"));
    let tool = DocConvert::new(e.clone());
    let work = tempfile::TempDir::new().unwrap();
    let cancel = CancelToken::new();
    let opts = serde_json::json!({});
    let out = tool
        .run(
            &src,
            &Ctx {
                work_dir: work.path(),
                target: Some(target),
                options: &opts,
                cancel: &cancel,
                progress: &|_| {},
            },
        )
        .unwrap();
    println!("{:#?}", out.report);
    let (p, ext) = &out.files[0];
    let file = if ext.is_empty() {
        std::fs::read_dir(p)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|f| f.is_file())
            .unwrap()
    } else {
        p.clone()
    };
    let bytes = std::fs::read(&file).unwrap();
    let kind = fo_core::detect::detect_file(&src).unwrap().kind;
    let w = |from: &str, name: &str, b: Vec<u8>| {
        let r = e
            .run(
                PandocRequest { options: serde_json::json!({"from": from, "to": "json", "standalone": true, "input-files": [name]}), files: vec![(name.into(), b)] },
                &CancelToken::new(),
            )
            .unwrap();
        stats(&serde_json::from_slice(&r.stdout).unwrap()).words
    };
    let before = match fo_core::pandoc::formats::reader_for(kind).unwrap() {
        "json" => normalise_words(&String::from_utf8_lossy(&std::fs::read(&src).unwrap())),
        f => w(f, "in", std::fs::read(&src).unwrap()),
    };
    let after = match reader_for_target(target) {
        Some(r) => w(r, "out", bytes),
        None => normalise_words(&strip_note_markers(&String::from_utf8_lossy(&bytes))),
    };
    let (b, af): (Vec<&str>, Vec<&str>) = (
        before.iter().map(|s| s.as_str()).collect(),
        after.iter().map(|s| s.as_str()).collect(),
    );
    for op in similar::capture_diff_slices(similar::Algorithm::Myers, &b, &af) {
        match op {
            similar::DiffOp::Equal { .. } => {}
            similar::DiffOp::Delete {
                old_index, old_len, ..
            } => println!("- {:?}", &b[old_index..old_index + old_len]),
            similar::DiffOp::Insert {
                new_index, new_len, ..
            } => println!("+ {:?}", &af[new_index..new_index + new_len]),
            similar::DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                println!(
                    "~ {:?} -> {:?}",
                    &b[old_index..old_index + old_len],
                    &af[new_index..new_index + new_len]
                )
            }
        }
    }
}
