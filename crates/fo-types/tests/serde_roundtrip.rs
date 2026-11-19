use fo_types::*;

fn report(metrics: Vec<Metric>, sim: Option<f64>, warnings: Vec<&str>) -> FidelityReport {
    FidelityReport {
        metrics,
        text_similarity: sim,
        warnings: warnings.into_iter().map(String::from).collect(),
    }
}

fn m(name: &str, before: u64, after: u64) -> Metric {
    Metric { name: name.into(), before, after }
}

#[test]
fn roundtrip_job_event_done() {
    let ev = JobEvent {
        job_id: "j1".into(),
        input: "/tmp/রিপোর্ট ফাইল.pdf".into(),
        status: JobStatus::Done {
            outputs: vec!["/tmp/out.docx".into()],
            report: report(vec![m("tables", 2, 2)], Some(1.0), vec![]),
        },
    };
    let json = serde_json::to_string(&ev).unwrap();
    assert!(json.contains("\"done\""), "{json}");
    let back: JobEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(back, ev);
}

#[test]
fn verdict_exact_when_nothing_lost() {
    assert_eq!(report(vec![m("images", 3, 3)], Some(1.0), vec![]).verdict(), Verdict::Exact);
}

#[test]
fn verdict_warn_on_dropped_metric() {
    assert_eq!(report(vec![m("tables", 3, 2)], Some(1.0), vec![]).verdict(), Verdict::Warn);
}

#[test]
fn verdict_warn_on_similarity_0_985() {
    assert_eq!(report(vec![], Some(0.985), vec![]).verdict(), Verdict::Warn);
}

#[test]
fn verdict_warn_on_any_warning() {
    assert_eq!(report(vec![], None, vec!["font substituted"]).verdict(), Verdict::Warn);
}

#[test]
fn image_kind_serializes_as_tagged_object() {
    let json = serde_json::to_string(&FileKind::Image(ImageFormat::Png)).unwrap();
    assert_eq!(json, r#"{"image":"png"}"#);
    assert_eq!(serde_json::to_string(&FileKind::Pdf).unwrap(), r#""pdf""#);
}

#[test]
fn labels_are_human_readable() {
    assert_eq!(FileKind::Docx.label(), "Word document");
    assert_eq!(FileKind::Image(ImageFormat::Png).label(), "PNG image");
}
