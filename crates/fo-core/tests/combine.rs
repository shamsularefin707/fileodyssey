use fo_core::image::tools::image_tools;
use fo_core::registry::Registry;
use fo_core::runner::Runner;
use fo_types::*;
use image::{DynamicImage, ImageBuffer, Rgb};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;

fn events(inputs: Vec<String>, out: &std::path::Path) -> Vec<JobEvent> {
    let mut r = Registry::new();
    for t in image_tools() {
        r.register(t);
    }
    let runner = Runner::new(Arc::new(r), 2);
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    runner.submit(
        JobRequest {
            tool_id: "img.to_pdf".into(),
            inputs,
            target: Some("pdf".into()),
            options: serde_json::json!({"page": "a4"}),
            out_dir: out.to_string_lossy().into(),
        },
        Arc::new(move |e| tx.lock().unwrap().send(e).unwrap()),
    );
    let mut evs = vec![];
    loop {
        let e: JobEvent = rx.recv_timeout(Duration::from_secs(30)).unwrap();
        let done = e.status.is_finished();
        evs.push(e);
        if done {
            // A combine job has exactly one unit: nothing else may follow.
            assert!(
                rx.recv_timeout(Duration::from_millis(300)).is_err(),
                "extra events"
            );
            return evs;
        }
    }
}

fn pic(d: &TempDir, name: &str, w: u32, h: u32) -> String {
    let p = d.path().join(name);
    DynamicImage::ImageRgb8(ImageBuffer::from_pixel(w, h, Rgb([w as u8, h as u8, 7])))
        .save(&p)
        .unwrap();
    p.to_string_lossy().into()
}

#[test]
fn combine_three_images_into_one_pdf() {
    let d = TempDir::new().unwrap();
    let inputs = vec![
        pic(&d, "a.png", 40, 30),
        pic(&d, "b.jpg", 30, 40),
        pic(&d, "c.png", 20, 20),
    ];
    let evs = events(inputs.clone(), d.path());
    assert!(evs.iter().all(|e| e.input == inputs[0]));
    let JobStatus::Done { outputs, report } = &evs.last().unwrap().status else {
        panic!("{evs:?}")
    };
    assert_eq!(outputs.len(), 1);
    assert!(outputs[0].ends_with("a (combined).pdf"), "{outputs:?}");
    let pdf = String::from_utf8_lossy(&std::fs::read(&outputs[0]).unwrap()).into_owned();
    assert_eq!(pdf.matches("/Type /Page\n").count(), 3);
    assert!(pdf.contains("/MediaBox [0 0 595.27"), "A4 page size");
    let pages = report.metrics.iter().find(|m| m.name == "Pages").unwrap();
    assert_eq!((pages.before, pages.after), (3, 3));
}

#[test]
fn combine_missing_input_fails() {
    let d = TempDir::new().unwrap();
    let inputs = vec![
        pic(&d, "a.png", 10, 10),
        d.path().join("gone.png").to_string_lossy().into(),
    ];
    let evs = events(inputs, d.path());
    assert_eq!(
        evs.last().unwrap().status,
        JobStatus::Failed {
            message: "File not found: gone.png".into()
        }
    );
    let names: Vec<_> = std::fs::read_dir(d.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 1, "{names:?}");
}

#[test]
fn combine_failure_names_the_bad_file() {
    let d = TempDir::new().unwrap();
    let bad = d.path().join("c.png");
    std::fs::write(&bad, b"\x89PNG\r\n\x1a\nnot really").unwrap();
    let inputs = vec![pic(&d, "a.png", 10, 10), bad.to_string_lossy().into()];
    match &events(inputs, d.path()).last().unwrap().status {
        JobStatus::Failed { message } => assert!(message.starts_with("c.png: "), "{message}"),
        other => panic!("{other:?}"),
    }
}
