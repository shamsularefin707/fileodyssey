use fo_core::pandoc::frame::{decode_request, decode_result, encode_request, encode_result};
use fo_core::pandoc::{NativePandoc, PandocEngine, PandocRequest, PandocResult};
use fo_core::tool::CancelToken;
use serde_json::json;

fn engine() -> NativePandoc {
    NativePandoc::find().expect("native pandoc (run scripts/fetch-pandoc.sh --native)")
}

#[test]
fn converts_markdown_to_docx_and_back() {
    let req = PandocRequest {
        options: json!({"from": "markdown", "to": "docx", "input-files": ["নোট.md"], "output-file": "out.docx"}),
        files: vec![(
            "নোট.md".into(),
            "# শিরোনাম\n\nHello *there*.\n".as_bytes().to_vec(),
        )],
    };
    let r = engine().run(req, &CancelToken::new()).unwrap();
    let docx = r
        .files
        .iter()
        .find(|(n, _)| n == "out.docx")
        .expect("output file")
        .1
        .clone();
    assert!(docx.starts_with(b"PK"));
    let back = engine()
        .run(
            PandocRequest {
                options: json!({"from": "docx", "to": "markdown", "input-files": ["in.docx"]}),
                files: vec![("in.docx".into(), docx)],
            },
            &CancelToken::new(),
        )
        .unwrap();
    let md = String::from_utf8(back.stdout).unwrap();
    assert!(md.contains("শিরোনাম") && md.contains("*there*"), "{md}");
}

#[test]
fn pandoc_errors_are_reported() {
    let req = PandocRequest {
        options: json!({"from": "nonsense-format", "to": "html", "input-files": ["a.txt"]}),
        files: vec![("a.txt".into(), b"x".to_vec())],
    };
    let e = engine().run(req, &CancelToken::new()).unwrap_err();
    assert!(e.to_lowercase().contains("nonsense"), "{e}");
}

#[test]
fn frames_round_trip() {
    let req = PandocRequest {
        options: json!({"to": "html", "input-files": ["ফাইল.md", "empty"]}),
        files: vec![
            ("ফাইল.md".into(), b"# Hi".to_vec()),
            ("empty".into(), vec![]),
        ],
    };
    let back = decode_request(&encode_request(&req)).unwrap();
    assert_eq!(back.options, req.options);
    assert_eq!(back.files, req.files);
    let res = PandocResult {
        stdout: b"<h1>Hi</h1>".to_vec(),
        stderr: "warn".into(),
        files: vec![("media/ছবি.png".into(), vec![1, 2, 3])],
        warnings: vec!["Could not fetch resource".into()],
    };
    assert_eq!(
        decode_result(&encode_result(&Ok(res.clone()))).unwrap(),
        Ok(res)
    );
    assert_eq!(
        decode_result(&encode_result(&Err("boom".into()))).unwrap(),
        Err("boom".to_string())
    );
    assert!(decode_result(b"garbage").is_err());
}
