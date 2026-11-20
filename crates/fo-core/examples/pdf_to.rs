//! Developer aid: convert PDFs with pdf.convert into a folder.
//! `cargo run -p fo-core --example pdf_to -- <target> <out-dir> file.pdf...`
use fo_core::tool::{CancelToken, Ctx, Tool};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let engine = std::sync::Arc::new(fo_core::pandoc::NativePandoc::find().expect("native pandoc"));
    let tool = fo_core::pdfconv::tool::PdfConvert::new(engine);
    let out = std::path::Path::new(&a[2]);
    std::fs::create_dir_all(out).unwrap();
    for f in &a[3..] {
        let p = std::path::Path::new(f);
        let stem = p.file_stem().unwrap().to_string_lossy();
        let work = out.join(format!("work-{stem}"));
        std::fs::create_dir_all(&work).unwrap();
        let cancel = CancelToken::new();
        let opts = serde_json::json!({});
        let r = tool
            .run(
                p,
                &Ctx {
                    work_dir: &work,
                    target: Some(&a[1]),
                    options: &opts,
                    cancel: &cancel,
                    progress: &|_| {},
                },
            )
            .unwrap();
        let dest = out.join(format!("{stem}.{}", a[1]));
        std::fs::rename(&r.files[0].0, &dest).unwrap();
        std::fs::remove_dir_all(&work).ok();
        println!(
            "{} {:?} {:?} similarity {:?}",
            dest.display(),
            r.report.verdict(),
            r.report.warnings,
            r.report.text_similarity
        );
        for m in &r.report.metrics {
            println!("   {} {} -> {}", m.name, m.before, m.after);
        }
    }
}
