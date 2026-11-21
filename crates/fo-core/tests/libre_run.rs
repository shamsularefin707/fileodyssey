//! Finding and running LibreOffice. A fake `soffice` (a shell script)
//! makes these run anywhere; one test uses the real one when installed.
#![cfg(unix)]
use fo_core::libre::{convert, find_in, find_soffice};
use fo_core::tool::CancelToken;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

/// A fake that records its arguments and writes `<outdir>/input.<ext>`.
const WRITES: &str = r#"printf '%s\n' "$@" > "$(dirname "$0")/args.txt"
out=""; prev=""; file=""
for a in "$@"; do
  [ "$prev" = "--outdir" ] && out="$a"
  [ "$prev" = "--convert-to" ] && ext="${a%%:*}"
  prev="$a"; file="$a"
done
base=$(basename "$file"); echo converted > "$out/${base%.*}.$ext""#;

#[test]
fn finds_soffice_by_override_path_and_install_folder() {
    let d = tempfile::TempDir::new().unwrap();
    let bin = d.path().join("bin");
    let opt = d.path().join("opt/libreoffice/program");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&opt).unwrap();
    let on_path = script(&bin, "soffice", "exit 0");
    let installed = script(&opt, "soffice", "exit 0");
    let path_var = std::ffi::OsString::from(bin.as_os_str());
    assert_eq!(find_in(None, Some(&path_var), &[]), Some(on_path.clone()));
    assert_eq!(
        find_in(None, None, std::slice::from_ref(&installed)),
        Some(installed.clone())
    );
    assert_eq!(
        find_in(Some(installed.as_os_str()), Some(&path_var), &[]),
        Some(installed)
    );
    assert_eq!(find_in(None, None, &[d.path().join("missing")]), None);
}

#[test]
fn runs_headless_on_a_safe_copy_with_its_own_profile() {
    let d = tempfile::TempDir::new().unwrap();
    let fake = script(d.path(), "soffice", WRITES);
    let input = d.path().join("-my report é.docx");
    std::fs::write(&input, b"doc").unwrap();
    let work = d.path().join("work");
    std::fs::create_dir(&work).unwrap();
    let out = convert(
        &fake,
        &input,
        "pdf",
        "pdf",
        &work,
        &CancelToken::new(),
        Duration::from_secs(20),
    )
    .unwrap();
    assert_eq!(std::fs::read_to_string(&out).unwrap().trim(), "converted");
    assert!(out.starts_with(&work));
    let args = std::fs::read_to_string(d.path().join("args.txt")).unwrap();
    let args: Vec<&str> = args.lines().collect();
    assert!(
        args.contains(&"--headless") && args.contains(&"--convert-to"),
        "{args:?}"
    );
    let profile = args
        .iter()
        .find(|a| a.starts_with("-env:UserInstallation=file://"))
        .expect("own profile");
    assert!(profile.contains(work.to_str().unwrap()), "{profile}");
    let last = Path::new(args.last().unwrap());
    assert_eq!(last.file_name().unwrap(), "input.docx");
    assert!(last.is_absolute());
}

#[test]
fn exit_zero_without_output_is_an_error() {
    let d = tempfile::TempDir::new().unwrap();
    let fake = script(d.path(), "soffice", "exit 0");
    let input = d.path().join("a.docx");
    std::fs::write(&input, b"doc").unwrap();
    let e = convert(
        &fake,
        &input,
        "pdf",
        "pdf",
        d.path(),
        &CancelToken::new(),
        Duration::from_secs(20),
    )
    .unwrap_err();
    assert!(e.contains("LibreOffice couldn't convert"), "{e}");
}

#[test]
fn a_hanging_soffice_is_killed_at_the_limit() {
    let d = tempfile::TempDir::new().unwrap();
    let fake = script(d.path(), "soffice", "sleep 30");
    let input = d.path().join("a.docx");
    std::fs::write(&input, b"doc").unwrap();
    let t = Instant::now();
    let e = convert(
        &fake,
        &input,
        "pdf",
        "pdf",
        d.path(),
        &CancelToken::new(),
        Duration::from_millis(500),
    )
    .unwrap_err();
    assert!(t.elapsed() < Duration::from_secs(5));
    assert!(e.contains("took too long"), "{e}");
}

#[test]
fn cancel_kills_soffice() {
    let d = tempfile::TempDir::new().unwrap();
    let fake = script(d.path(), "soffice", "sleep 30");
    let input = d.path().join("a.docx");
    std::fs::write(&input, b"doc").unwrap();
    let cancel = CancelToken::new();
    let c = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        c.cancel();
    });
    let t = Instant::now();
    let e = convert(
        &fake,
        &input,
        "pdf",
        "pdf",
        d.path(),
        &cancel,
        Duration::from_secs(60),
    )
    .unwrap_err();
    assert!(t.elapsed() < Duration::from_secs(5));
    assert!(e.contains("Cancelled"), "{e}");
}

#[test]
fn real_libreoffice_docx_to_pdf() {
    let Some(soffice) = find_soffice() else {
        eprintln!("skipped: LibreOffice isn't installed");
        return;
    };
    let d = tempfile::TempDir::new().unwrap();
    let input = d.path().join("in.docx");
    let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/report.docx");
    std::fs::copy(sample, &input).unwrap();
    let out = convert(
        &soffice,
        &input,
        "pdf",
        "pdf",
        d.path(),
        &CancelToken::new(),
        Duration::from_secs(120),
    )
    .unwrap();
    assert!(std::fs::read(out).unwrap().starts_with(b"%PDF"));
}
