//! Scan history: a local, deletable list.
use fo_core::scan::history::History;
use fo_types::ScanKind;

fn code(c: &str) -> ScanKind {
    ScanKind::Code {
        content: c.into(),
        format: "QR_CODE".into(),
    }
}

#[test]
fn newest_first_and_kept_across_reopen() {
    let d = tempfile::TempDir::new().unwrap();
    let mut h = History::open(d.path());
    let a = h.add(code("first")).unwrap();
    let b = h.add(code("second")).unwrap();
    assert_ne!(a.id, b.id);
    let h2 = History::open(d.path());
    let ids: Vec<String> = h2.list().iter().map(|r| r.id.clone()).collect();
    assert_eq!(ids, vec![b.id, a.id]);
}

#[test]
fn delete_removes_only_files_in_the_store() {
    let d = tempfile::TempDir::new().unwrap();
    let store = d.path().join("store");
    let mut h = History::open(&store);
    let inside = h.files_dir().join("page-1.jpg");
    std::fs::create_dir_all(inside.parent().unwrap()).unwrap();
    std::fs::write(&inside, b"jpg").unwrap();
    let outside = d.path().join("Scan 2026-11-22.pdf");
    std::fs::write(&outside, b"pdf").unwrap();
    let r = h
        .add(ScanKind::Document {
            files: vec![inside.clone(), outside.clone()],
        })
        .unwrap();
    h.add(code("kept")).unwrap();
    h.delete(&r.id).unwrap();
    assert!(!inside.exists(), "store copy removed");
    assert!(outside.exists(), "the user's saved file stays");
    assert_eq!(History::open(&store).list().len(), 1);
}

#[test]
fn clear_and_corrupt_and_cap() {
    let d = tempfile::TempDir::new().unwrap();
    let mut h = History::open(d.path());
    for i in 0..505 {
        h.add(code(&format!("c{i}"))).unwrap();
    }
    let list = h.list();
    assert_eq!(list.len(), 500);
    assert!(matches!(&list[0].kind, ScanKind::Code { content, .. } if content == "c504"));
    h.clear().unwrap();
    assert!(History::open(d.path()).list().is_empty());
    std::fs::write(d.path().join("history.json"), b"{not json").unwrap();
    assert!(History::open(d.path()).list().is_empty());
}
