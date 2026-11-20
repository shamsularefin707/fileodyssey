use fo_core::archive::create::create;
use fo_core::archive::extract::extract;
use fo_core::archive::guard::Budget;
use fo_core::detect::detect_kind;
use fo_core::tool::CancelToken;
use fo_types::JobStatus;
use std::path::PathBuf;
use tempfile::TempDir;

mod archive_tools_support {
    include!("archive_tools.rs");
}

fn random(n: usize) -> Vec<u8> {
    let mut s = 0x9E3779B9u32;
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            s as u8
        })
        .collect()
}

fn inputs(d: &TempDir) -> Vec<(PathBuf, Vec<u8>)> {
    let files: Vec<(&str, Vec<u8>)> = vec![
        ("notes.txt", b"plain text ".repeat(200)),
        ("রিপোর্ট.txt", "বাংলা লেখা".as_bytes().to_vec()),
        ("noise.bin", random(1 << 20)),
    ];
    files
        .into_iter()
        .map(|(n, b)| {
            let p = d.path().join(n);
            std::fs::write(&p, &b).unwrap();
            (p, b)
        })
        .collect()
}

#[test]
fn create_every_format_then_extract_matches() {
    let d = TempDir::new().unwrap();
    let ins = inputs(&d);
    let paths: Vec<PathBuf> = ins.iter().map(|(p, _)| p.clone()).collect();
    for format in ["zip", "7z", "tar", "tar.gz", "tar.xz", "tar.zst"] {
        let out = d.path().join(format!("out.{format}"));
        let r = create(&paths, format, 6, &out, &CancelToken::new())
            .unwrap_or_else(|e| panic!("{format}: {e:?}"));
        assert_eq!(r.entries, 3, "{format}");
        let dest = d.path().join(format!("x-{format}"));
        std::fs::create_dir(&dest).unwrap();
        let kind = detect_kind(&out).unwrap();
        extract(
            &out,
            &kind,
            &dest,
            Budget::new(1 << 30),
            &CancelToken::new(),
        )
        .unwrap_or_else(|e| panic!("{format}: {e:?}"));
        for (p, bytes) in &ins {
            let name = p.file_name().unwrap();
            assert_eq!(
                &std::fs::read(dest.join(name)).unwrap(),
                bytes,
                "{format} {name:?}"
            );
        }
    }
}

#[test]
fn duplicate_names_are_renamed() {
    let d = TempDir::new().unwrap();
    std::fs::create_dir(d.path().join("one")).unwrap();
    std::fs::create_dir(d.path().join("two")).unwrap();
    let a = d.path().join("one/same.txt");
    let b = d.path().join("two/same.txt");
    std::fs::write(&a, "first").unwrap();
    std::fs::write(&b, "second").unwrap();
    let out = d.path().join("dup.zip");
    let r = create(&[a, b], "zip", 6, &out, &CancelToken::new()).unwrap();
    assert_eq!(r.renamed, 1);
    let mut z = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
    let names: Vec<_> = (0..z.len())
        .map(|i| z.by_index(i).unwrap().name().to_owned())
        .collect();
    assert_eq!(names, vec!["same.txt", "same (2).txt"]);
}

#[test]
fn create_tool_names_archive_after_first_file() {
    let d = TempDir::new().unwrap();
    let ins = inputs(&d);
    let list = ins
        .iter()
        .map(|(p, _)| p.to_string_lossy().into_owned())
        .collect();
    match archive_tools_support::run(
        "arc.create",
        list,
        "tar.gz",
        serde_json::json!({"level": 9}),
        d.path(),
    ) {
        JobStatus::Done { outputs, report } => {
            assert!(
                outputs[0].ends_with("notes (archive).tar.gz"),
                "{outputs:?}"
            );
            let files = report.metrics.iter().find(|m| m.name == "Files").unwrap();
            assert_eq!((files.before, files.after), (3, 3));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn smaller_archive_is_not_reported_as_loss() {
    let d = TempDir::new().unwrap();
    let p = d.path().join("text.txt");
    std::fs::write(&p, b"repeat ".repeat(5000)).unwrap();
    match archive_tools_support::run(
        "arc.create",
        vec![p.to_string_lossy().into()],
        "zip",
        serde_json::json!({}),
        d.path(),
    ) {
        JobStatus::Done { report, .. } => {
            assert_eq!(report.verdict(), fo_types::Verdict::Exact, "{report:?}")
        }
        other => panic!("{other:?}"),
    }
}
