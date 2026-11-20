//! Developer aid: `cargo run -p fo-core --example pdf_diff -- tests/pdf-corpus/twocol.pdf`
//! prints the reconstructed Markdown and its word differences from the source.
use fo_core::pandoc::ast::stats;
use fo_core::pandoc::{NativePandoc, PandocEngine, PandocRequest};
use fo_core::pdfconv::tool::PdfConvert;
use fo_core::tool::{CancelToken, Ctx, Tool};
use std::sync::Arc;

fn words(e: &NativePandoc, from: &str, b: Vec<u8>) -> Vec<String> {
    let r = e
        .run(PandocRequest { options: serde_json::json!({"from": from, "to": "json", "standalone": true, "input-files": ["x"]}), files: vec![("x".into(), b)] }, &CancelToken::new())
        .unwrap();
    stats(&serde_json::from_slice(&r.stdout).unwrap()).words
}

fn main() {
    let pdf = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let stem = pdf.file_stem().unwrap().to_string_lossy().into_owned();
    let src = pdf.parent().unwrap().join("src");
    let e = Arc::new(NativePandoc::find().unwrap());
    let work = tempfile::TempDir::new().unwrap();
    let cancel = CancelToken::new();
    let opts = serde_json::json!({});
    let out = PdfConvert::new(e.clone())
        .run(
            &pdf,
            &Ctx {
                work_dir: work.path(),
                target: Some("md"),
                options: &opts,
                cancel: &cancel,
                progress: &|_| {},
            },
        )
        .unwrap();
    let (p, ext) = &out.files[0];
    let md = std::fs::read(if ext.is_empty() {
        p.join(format!("{stem}.md"))
    } else {
        p.clone()
    })
    .unwrap();
    println!("{}\n-----", String::from_utf8_lossy(&md));
    let truth = match std::fs::read(src.join(format!("{stem}.html"))) {
        Ok(h) => words(&e, "html", h),
        Err(_) => words(
            &e,
            "markdown",
            std::fs::read(src.join(format!("{stem}.md"))).unwrap(),
        ),
    };
    let got = words(&e, "markdown", md);
    let (a, b): (Vec<&str>, Vec<&str>) = (
        truth.iter().map(|s| s.as_str()).collect(),
        got.iter().map(|s| s.as_str()).collect(),
    );
    for op in similar::capture_diff_slices(similar::Algorithm::Myers, &a, &b) {
        match op {
            similar::DiffOp::Equal { .. } => {}
            similar::DiffOp::Delete {
                old_index, old_len, ..
            } => println!("- {:?}", &a[old_index..old_index + old_len]),
            similar::DiffOp::Insert {
                new_index, new_len, ..
            } => println!("+ {:?}", &b[new_index..new_index + new_len]),
            similar::DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                println!(
                    "~ {:?} -> {:?}",
                    &a[old_index..old_index + old_len],
                    &b[new_index..new_index + new_len]
                )
            }
        }
    }
}
