use fo_core::pandoc::ast::{normalise_words, report, similarity, stats};
use fo_core::pandoc::{NativePandoc, PandocEngine, PandocRequest};
use fo_core::tool::CancelToken;
use serde_json::json;

fn ast(md: &str) -> serde_json::Value {
    let p = NativePandoc::find().expect("native pandoc");
    let r = p
        .run(
            PandocRequest {
                options: json!({"from": "markdown", "to": "json", "input-files": ["a.md"]}),
                files: vec![("a.md".into(), md.as_bytes().to_vec())],
            },
            &CancelToken::new(),
        )
        .unwrap();
    serde_json::from_slice(&r.stdout).unwrap()
}

#[test]
fn counts_structure() {
    let doc = ast(
        "# One\n\n## Two\n\nA [link](https://x.org) and ![pic](a.png) here.[^n]\n\n[^n]: Note text.\n\n\
         | a | b |\n|---|---|\n| 1 | [![i](b.png)](https://y.org) |\n| 3 | 4 |\n\n- item one\n- item two\n",
    );
    let s = stats(&doc);
    assert_eq!(s.headings, 2);
    assert_eq!(s.tables, 1);
    assert_eq!(s.cells, 6, "header row + two body rows");
    assert_eq!(s.images, 2);
    assert_eq!(s.links, 2);
    assert_eq!(s.footnotes, 1);
    assert!(s.paragraphs >= 2);
    assert!(
        s.words.contains(&"Note".to_string()),
        "footnote text counts: {:?}",
        s.words
    );
}

#[test]
fn normalisation_ignores_typography_not_words() {
    assert_eq!(
        normalise_words("“Hello,”  world — it’s  2019–2020!"),
        vec!["Hello", "world", "it's", "2019-2020"]
    );
    assert_eq!(
        normalise_words("ﬁne café"),
        normalise_words("fine cafe\u{301}")
    );
    assert_eq!(normalise_words("আমার সোনার বাংলা।"), vec!["আমার", "সোনার", "বাংলা"]);
}

#[test]
fn similarity_ratio() {
    let a: Vec<String> = (0..100).map(|i| format!("w{i}")).collect();
    assert_eq!(similarity(&a, &a), 1.0);
    let mut b = a.clone();
    b[10] = "x".into();
    b[50] = "y".into();
    let s = similarity(&a, &b);
    assert!(s < 0.99 && s > 0.97, "{s}");
    assert_eq!(similarity(&[], &[]), 1.0);
}

#[test]
fn report_flags_drops_and_plain_text() {
    let before = stats(&ast("# T\n\nSome words [l](https://x.org).\n"));
    let same = report(&before, &before.clone(), true);
    assert_eq!(same.verdict(), fo_types::Verdict::Exact);
    assert_eq!(
        same.metrics
            .iter()
            .find(|m| m.name == "Links")
            .unwrap()
            .before,
        1
    );
    let flat = stats(&ast("T\n\nSome words l.\n"));
    let r = report(&before, &flat, true);
    assert_eq!(r.verdict(), fo_types::Verdict::Warn);
    let plain = report(&before, &flat, false);
    assert!(plain.metrics.iter().all(|m| m.name == "Words"));
    assert!(
        plain.warnings.iter().any(|w| w.contains("Plain text")),
        "{:?}",
        plain.warnings
    );
}

#[test]
fn footnote_words_compare_at_the_end_and_markers_strip() {
    let s = stats(&ast("Body one.[^a] Body two.\n\n[^a]: Note words.\n"));
    assert_eq!(s.words, vec!["Body", "one", "Body", "two", "Note", "words"]);
    assert_eq!(
        fo_core::pandoc::ast::strip_note_markers("map.[1] and [12] but [x]"),
        "map.  and   but [x]"
    );
}
