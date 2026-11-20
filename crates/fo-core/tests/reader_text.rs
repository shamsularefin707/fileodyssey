use fo_core::reader::text::{decode_text, markdown_html, read_text_doc, sanitize};
use fo_types::FileKind;
use std::collections::HashMap;
use std::sync::Arc;

fn no_chapters() -> Arc<HashMap<String, usize>> {
    Arc::new(HashMap::new())
}

#[test]
fn decodes_utf16_bom_windows_1252_and_bengali() {
    let mut le = vec![0xFF, 0xFE];
    for u in "héllo".encode_utf16() {
        le.extend_from_slice(&u.to_le_bytes());
    }
    assert_eq!(decode_text(&le), "héllo");
    // "Le café est très bon, déjà vu" in windows-1252.
    let cp = b"Le caf\xe9 est tr\xe8s bon, d\xe9j\xe0 vu \xe0 la cr\xe8me br\xfbl\xe9e";
    assert!(decode_text(cp).starts_with("Le café est très bon"));
    assert_eq!(decode_text("আমার সোনার বাংলা".as_bytes()), "আমার সোনার বাংলা");
    assert_eq!(decode_text(b"\xEF\xBB\xBFplain"), "plain");
}

#[test]
fn markdown_renders_headings_tables_and_code() {
    let h = markdown_html("# Title\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n```\nlet x = 1;\n```\n");
    assert!(h.contains("<h1>Title</h1>"), "{h}");
    assert!(h.contains("<table>") && h.contains("<td>2</td>"), "{h}");
    assert!(h.contains("<code>let x = 1;"), "{h}");
}

#[test]
fn sanitize_removes_active_and_remote_content() {
    let h = sanitize(
        r#"<p onclick="evil()">Hi <a href="javascript:alert(1)">x</a></p>
           <script>alert(1)</script><style>body{}</style><title>T</title>
           <img src="https://tracker.example/p.gif"><img src="data:image/png;base64,AAAA">
           <iframe src="https://x"></iframe><a href="https://example.org/a?b=1">site</a>"#,
        "",
        &no_chapters(),
    );
    for bad in [
        "onclick",
        "javascript:",
        "<script",
        "alert",
        "<style",
        "tracker.example",
        "<iframe",
        "<title",
    ] {
        assert!(!h.contains(bad), "{bad} survived: {h}");
    }
    assert!(
        h.contains("data:image/png;base64,AAAA"),
        "inline images stay: {h}"
    );
    assert!(
        h.contains(r#"href="fo-ext:https://example.org/a?b=1""#),
        "{h}"
    );
    assert!(h.contains(">site</a>"));
}

#[test]
fn sanitize_maps_local_resources_anchors_and_chapters() {
    let chapters = Arc::new(HashMap::from([("OEBPS/ch2.xhtml".to_string(), 1)]));
    let h = sanitize(
        r##"<img src="img/a%20b.png"><img src="../../../etc/passwd"><a href="#sec">s</a>
            <h2 id="sec">S</h2><a href="ch2.xhtml#top">next</a><a href="missing.xhtml">m</a>"##,
        "OEBPS",
        &chapters,
    );
    assert!(h.contains(r#"data-fo-src="OEBPS/img/a b.png""#), "{h}");
    assert!(!h.contains("passwd"), "{h}");
    assert!(
        h.contains(r##"href="#fo-sec""##) && h.contains(r#"id="fo-sec""#),
        "{h}"
    );
    assert!(h.contains(r#"href="fo-ch:1#fo-top""#), "{h}");
    assert!(
        h.contains("<a>m</a>") || h.contains(r#"<a rel="noopener noreferrer">m</a>"#),
        "{h}"
    );
}

#[test]
fn read_text_doc_handles_each_kind() {
    let d = tempfile::TempDir::new().unwrap();
    let t = d.path().join("a.txt");
    std::fs::write(&t, "1 < 2 & <b>not bold</b>").unwrap();
    let h = read_text_doc(&t, FileKind::Text).unwrap();
    assert!(h.contains("1 &lt; 2 &amp; &lt;b&gt;"), "{h}");
    let m = d.path().join("a.md");
    std::fs::write(&m, "**bold** ![pic](pics/p.png)").unwrap();
    let h = read_text_doc(&m, FileKind::Markdown).unwrap();
    assert!(
        h.contains("<strong>bold</strong>") && h.contains(r#"data-fo-src="pics/p.png""#),
        "{h}"
    );
    let w = d.path().join("a.html");
    std::fs::write(
        &w,
        "<html><head><title>x</title><script>1</script></head><body><h1>Hi</h1></body></html>",
    )
    .unwrap();
    assert_eq!(
        read_text_doc(&w, FileKind::Html).unwrap().trim(),
        "<h1>Hi</h1>"
    );
}
