use fo_core::pandoc::tool::DocConvert;
use fo_core::pandoc::{NativePandoc, PandocEngine, PandocRequest, PandocResult};
use fo_core::tool::{CancelToken, Ctx, Tool, ToolError};
use fo_types::{FileKind, Verdict};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;

fn tool() -> DocConvert {
    DocConvert::new(Arc::new(NativePandoc::find().expect("native pandoc")))
}

fn run(
    t: &DocConvert,
    input: &Path,
    target: &str,
    opts: serde_json::Value,
) -> Result<fo_core::tool::ToolOutput, ToolError> {
    let work = input.parent().unwrap().join(format!("work-{target}"));
    std::fs::create_dir_all(&work).unwrap();
    let cancel = CancelToken::new();
    let ctx = Ctx {
        work_dir: &work,
        target: Some(target),
        options: &opts,
        cancel: &cancel,
        progress: &|_| {},
    };
    t.run(input, &ctx)
}

const MD: &str = "# Field notes\n\nThe river *rose* slowly; see [the map](https://example.org/map).[^1]\n\n| Month | Level |\n|---|---|\n| May | 2.1 |\n| June | 2.9 |\n\n[^1]: Measured at the old ferry.\n";

fn write(d: &TempDir, name: &str, data: &[u8]) -> PathBuf {
    let p = d.path().join(name);
    std::fs::write(&p, data).unwrap();
    p
}

fn png() -> Vec<u8> {
    let mut v = Vec::new();
    image::RgbImage::from_pixel(4, 4, image::Rgb([10, 20, 30]))
        .write_to(&mut std::io::Cursor::new(&mut v), image::ImageFormat::Png)
        .unwrap();
    v
}

#[test]
fn markdown_to_docx_is_exact() {
    let d = TempDir::new().unwrap();
    let out = run(
        &tool(),
        &write(&d, "notes.md", MD.as_bytes()),
        "docx",
        json!({}),
    )
    .unwrap();
    assert_eq!(out.files.len(), 1);
    assert_eq!(out.files[0].1, "docx");
    assert!(std::fs::read(&out.files[0].0).unwrap().starts_with(b"PK"));
    assert_eq!(out.report.verdict(), Verdict::Exact, "{:#?}", out.report);
    let get = |n: &str| {
        out.report
            .metrics
            .iter()
            .find(|m| m.name == n)
            .map(|m| (m.before, m.after))
    };
    assert_eq!(get("Footnotes"), Some((1, 1)));
    assert_eq!(get("Table cells"), Some((6, 6)));
}

#[test]
fn local_images_are_carried_into_docx_and_out_to_a_media_folder() {
    let d = TempDir::new().unwrap();
    std::fs::create_dir(d.path().join("pics")).unwrap();
    write(&d, "pics/dot.png", &png());
    let md = write(&d, "with pic.md", b"# Pic\n\n![A dot](pics/dot.png)\n");
    let docx = run(&tool(), &md, "docx", json!({})).unwrap();
    assert!(
        docx.report.warnings.is_empty(),
        "{:?}",
        docx.report.warnings
    );
    assert_eq!(
        docx.report
            .metrics
            .iter()
            .find(|m| m.name == "Images")
            .map(|m| (m.before, m.after)),
        Some((1, 1))
    );
    let docx_path = d.path().join("pic.docx");
    std::fs::copy(&docx.files[0].0, &docx_path).unwrap();
    let back = run(&tool(), &docx_path, "md", json!({})).unwrap();
    let (dir, ext) = &back.files[0];
    assert_eq!(ext, "", "a folder");
    assert!(
        dir.join("pic.md").is_file(),
        "{:?}",
        std::fs::read_dir(dir).unwrap().collect::<Vec<_>>()
    );
    let media: Vec<_> = std::fs::read_dir(dir.join("media")).unwrap().collect();
    assert_eq!(media.len(), 1);
    let md_text = std::fs::read_to_string(dir.join("pic.md")).unwrap();
    assert!(md_text.contains("media/"), "{md_text}");
}

#[test]
fn plain_text_both_ways() {
    let d = TempDir::new().unwrap();
    let txt = run(
        &tool(),
        &write(&d, "notes.md", MD.as_bytes()),
        "txt",
        json!({}),
    )
    .unwrap();
    assert_eq!(txt.report.verdict(), Verdict::Warn);
    assert!(
        txt.report.text_similarity.unwrap() > 0.99,
        "{:?}",
        txt.report
    );
    let src = write(
        &d,
        "চিঠি.txt",
        "প্রিয় বন্ধু,\nকেমন আছ?\n\nNew paragraph * not emphasis *.\n".as_bytes(),
    );
    let out = run(&tool(), &src, "docx", json!({})).unwrap();
    assert_eq!(out.report.verdict(), Verdict::Exact, "{:#?}", out.report);
}

#[test]
fn markdown_flavour_option() {
    let d = TempDir::new().unwrap();
    let src = write(&d, "a.html", b"<h1>T</h1><p>One <del>two</del></p>");
    let out = run(&tool(), &src, "md", json!({"flavour": "gfm"})).unwrap();
    let text = std::fs::read_to_string(&out.files[0].0).unwrap();
    assert!(text.contains("~~two~~"), "{text}");
}

#[test]
fn targets_exclude_own_format() {
    let t = tool();
    assert!(!t
        .info_for(&FileKind::Markdown)
        .targets
        .contains(&"md".to_string()));
    assert!(t
        .info_for(&FileKind::Docx)
        .targets
        .contains(&"md".to_string()));
    assert!(t.accepts(&FileKind::Rst) && !t.accepts(&FileKind::Pdf));
}

struct Broken;
impl PandocEngine for Broken {
    fn run(&self, _: PandocRequest, _: &CancelToken) -> Result<PandocResult, String> {
        Err("Unknown reader: wat".into())
    }
}

#[test]
fn engine_errors_become_job_errors() {
    let d = TempDir::new().unwrap();
    let t = DocConvert::new(Arc::new(Broken));
    match run(&t, &write(&d, "a.md", b"# x"), "docx", json!({})) {
        Err(ToolError::Failed(m)) => assert!(m.contains("Unknown reader"), "{m}"),
        other => panic!("{:?}", other.map(|o| o.report)),
    }
}
