use fo_core::naming::output_path;
use tempfile::TempDir;

#[test]
fn first_name_is_converted_suffix() {
    let d = TempDir::new().unwrap();
    let input = d.path().join("a.pdf");
    assert_eq!(
        output_path(&input, d.path(), "docx"),
        d.path().join("a (converted).docx")
    );
}

#[test]
fn existing_name_gets_counter() {
    let d = TempDir::new().unwrap();
    std::fs::write(d.path().join("a (converted).docx"), b"x").unwrap();
    std::fs::write(d.path().join("a (converted 2).docx"), b"x").unwrap();
    let got = output_path(&d.path().join("a.pdf"), d.path(), "docx");
    assert_eq!(got, d.path().join("a (converted 3).docx"));
}

#[test]
fn never_returns_input_path() {
    let d = TempDir::new().unwrap();
    let input = d.path().join("x (converted).md");
    std::fs::write(&input, b"x").unwrap();
    let got = output_path(&input, d.path(), "md");
    assert_ne!(got, input);
    assert_eq!(got, d.path().join("x (converted) (converted).md"));
}

#[test]
fn unicode_stem_preserved() {
    let d = TempDir::new().unwrap();
    let got = output_path(&d.path().join("রিপোর্ট ফাইল.pdf"), d.path(), "docx");
    assert_eq!(
        got.file_name().unwrap().to_str().unwrap(),
        "রিপোর্ট ফাইল (converted).docx"
    );
}

#[test]
fn naming_combined_label() {
    let d = TempDir::new().unwrap();
    let got =
        fo_core::naming::output_path_with(&d.path().join("a.png"), d.path(), "pdf", "combined");
    assert_eq!(got, d.path().join("a (combined).pdf"));
    std::fs::write(&got, b"x").unwrap();
    let next =
        fo_core::naming::output_path_with(&d.path().join("a.png"), d.path(), "pdf", "combined");
    assert_eq!(next, d.path().join("a (combined 2).pdf"));
}
