use fo_core::pandoc::formats::{reader_for, targets_for, writer_for};
use fo_types::FileKind;

#[test]
fn readers_writers_and_targets() {
    assert_eq!(reader_for(FileKind::Docx), Some("docx"));
    assert_eq!(reader_for(FileKind::Markdown), Some("markdown"));
    assert_eq!(reader_for(FileKind::AsciiDoc), Some("asciidoc"));
    assert_eq!(reader_for(FileKind::Pdf), None, "PDF is M6");
    assert_eq!(writer_for("md"), Some("markdown"));
    assert_eq!(writer_for("txt"), Some("plain"));
    assert_eq!(writer_for("pdf"), None, "PDF output is M7");
    let t = targets_for(FileKind::Markdown);
    assert!(t.contains(&"docx") && t.contains(&"html") && !t.contains(&"md"));
    assert!(!targets_for(FileKind::Docx).contains(&"docx"));
    assert!(targets_for(FileKind::Text).contains(&"docx"));
    assert!(targets_for(FileKind::Pdf).is_empty());
}
