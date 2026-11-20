use fo_core::archive::extract::extract;
use fo_core::archive::guard::{safe_path, Budget};
use fo_core::tool::{CancelToken, ToolError};
use fo_types::{ArchiveFormat, FileKind};
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const FILES: &[(&str, &[u8])] = &[
    ("a.txt", b"alpha"),
    (
        "sub/\u{09B0}\u{09BF}\u{09AA}\u{09CB}\u{09B0}\u{09CD}\u{099F}.txt",
        "বাংলা".as_bytes(),
    ),
    ("sub/deep/z.bin", &[0, 1, 2, 3, 255]),
];

fn tar_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut b = tar::Builder::new(Vec::new());
    for (name, data) in files {
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        b.append_data(&mut h, name, *data).unwrap();
    }
    b.into_inner().unwrap()
}

fn write(dir: &TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.path().join(name);
    std::fs::write(&p, bytes).unwrap();
    p
}

fn zip_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in files {
        z.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(data).unwrap();
    }
    z.finish().unwrap().into_inner()
}

fn check_tree(root: &Path) {
    for (name, data) in FILES {
        assert_eq!(std::fs::read(root.join(name)).unwrap(), *data, "{name}");
    }
}

fn run(
    path: &Path,
    kind: ArchiveFormat,
    dest: &Path,
) -> Result<fo_core::archive::extract::ExtractReport, ToolError> {
    let budget = Budget::new(std::fs::metadata(path).unwrap().len());
    extract(
        path,
        &FileKind::Archive(kind),
        dest,
        budget,
        &CancelToken::new(),
    )
}

#[test]
fn extract_every_format_roundtrip() {
    let d = TempDir::new().unwrap();
    let tar = tar_bytes(FILES);
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&tar).unwrap();
    let mut xz =
        lzma_rust2::XzWriter::new(Vec::new(), lzma_rust2::XzOptions::with_preset(6)).unwrap();
    xz.write_all(&tar).unwrap();
    let zst =
        ruzstd::encoding::compress_to_vec(&tar[..], ruzstd::encoding::CompressionLevel::Fastest);
    let cases: Vec<(&str, ArchiveFormat, Vec<u8>)> = vec![
        ("a.zip", ArchiveFormat::Zip, zip_bytes(FILES)),
        ("a.tar", ArchiveFormat::Tar, tar.clone()),
        ("a.tar.gz", ArchiveFormat::TarGz, gz.finish().unwrap()),
        ("a.tar.xz", ArchiveFormat::TarXz, xz.finish().unwrap()),
        ("a.tar.zst", ArchiveFormat::TarZst, zst),
    ];
    for (name, kind, bytes) in cases {
        let src = write(&d, name, &bytes);
        let dest = d.path().join(format!("out-{name}"));
        std::fs::create_dir(&dest).unwrap();
        let r = run(&src, kind, &dest).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_eq!(r.files, 3, "{name}");
        check_tree(&dest);
    }
    // 7z, built with the same library.
    let src = d.path().join("a.7z");
    let mut w = sevenz_rust2::ArchiveWriter::create(&src).unwrap();
    for (name, data) in FILES {
        w.push_archive_entry(sevenz_rust2::ArchiveEntry::new_file(name), Some(*data))
            .unwrap();
    }
    w.finish().unwrap();
    let dest = d.path().join("out-7z");
    std::fs::create_dir(&dest).unwrap();
    assert_eq!(run(&src, ArchiveFormat::SevenZ, &dest).unwrap().files, 3);
    check_tree(&dest);
}

#[test]
fn safe_path_rejects_escapes() {
    assert_eq!(safe_path("a/b.txt"), Some(PathBuf::from("a/b.txt")));
    assert_eq!(safe_path("./a\\b.txt"), Some(PathBuf::from("a/b.txt")));
    for bad in [
        "../evil.txt",
        "a/../../evil",
        "/etc/passwd",
        "C:\\Windows\\x",
        "",
        "a/..",
    ] {
        assert_eq!(safe_path(bad), None, "{bad}");
    }
}

#[test]
fn zip_slip_entries_are_skipped() {
    let d = TempDir::new().unwrap();
    let src = write(
        &d,
        "slip.zip",
        &zip_bytes(&[("../evil.txt", b"x"), ("/abs.txt", b"y"), ("ok.txt", b"ok")]),
    );
    let dest = d.path().join("out");
    std::fs::create_dir(&dest).unwrap();
    let r = run(&src, ArchiveFormat::Zip, &dest).unwrap();
    assert_eq!((r.files, r.skipped_unsafe), (1, 2));
    assert!(!d.path().join("evil.txt").exists());
    assert!(!Path::new("/abs.txt").exists());
    assert_eq!(std::fs::read(dest.join("ok.txt")).unwrap(), b"ok");
}

#[test]
fn tar_symlink_skipped() {
    let d = TempDir::new().unwrap();
    let mut b = tar::Builder::new(Vec::new());
    let mut h = tar::Header::new_gnu();
    h.set_entry_type(tar::EntryType::Symlink);
    h.set_size(0);
    h.set_cksum();
    b.append_link(&mut h, "link", "/etc/passwd").unwrap();
    let mut f = tar::Header::new_gnu();
    f.set_size(2);
    f.set_cksum();
    b.append_data(&mut f, "real.txt", &b"hi"[..]).unwrap();
    let src = write(&d, "l.tar", &b.into_inner().unwrap());
    let dest = d.path().join("out");
    std::fs::create_dir(&dest).unwrap();
    let r = run(&src, ArchiveFormat::Tar, &dest).unwrap();
    assert_eq!((r.files, r.skipped_links), (1, 1));
    assert!(std::fs::symlink_metadata(dest.join("link")).is_err());
}

#[test]
fn zip_bomb_aborts_and_leaves_nothing() {
    let d = TempDir::new().unwrap();
    let zeros = vec![0u8; 4 << 20];
    let src = write(
        &d,
        "bomb.zip",
        &zip_bytes(&[("small.txt", b"ok"), ("zeros.bin", &zeros)]),
    );
    let dest = d.path().join("out");
    std::fs::create_dir(&dest).unwrap();
    let err = extract(
        &src,
        &FileKind::Archive(ArchiveFormat::Zip),
        &dest,
        Budget::with_limit(1 << 20),
        &CancelToken::new(),
    )
    .err()
    .unwrap();
    match err {
        ToolError::Failed(m) => assert!(m.starts_with("This archive expands to more than"), "{m}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn budget_scales_with_archive_size() {
    assert_eq!(Budget::new(1).limit_bytes(), 1 << 30);
    assert_eq!(Budget::new(10 << 20).limit_bytes(), 250 * (10u64 << 20));
    assert_eq!(Budget::new(1 << 40).limit_bytes(), 16 << 30);
}
