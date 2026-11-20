use fo_core::archive::tools::archive_tools;
use fo_core::registry::Registry;
use fo_core::runner::Runner;
use fo_types::*;
use std::io::Write;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;

pub fn run(
    tool: &str,
    inputs: Vec<String>,
    target: &str,
    options: serde_json::Value,
    out: &std::path::Path,
) -> JobStatus {
    let mut r = Registry::new();
    for t in archive_tools() {
        r.register(t);
    }
    let runner = Runner::new(Arc::new(r), 1);
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    runner.submit(
        JobRequest {
            tool_id: tool.into(),
            inputs,
            target: Some(target.into()),
            options,
            out_dir: out.to_string_lossy().into(),
        },
        Arc::new(move |e| tx.lock().unwrap().send(e).unwrap()),
    );
    loop {
        let e: JobEvent = rx.recv_timeout(Duration::from_secs(60)).unwrap();
        if e.status.is_finished() {
            return e.status;
        }
    }
}

#[test]
fn extract_tool_places_folder_via_runner() {
    let d = TempDir::new().unwrap();
    let src = d.path().join("pack.zip");
    let mut z = zip::ZipWriter::new(std::fs::File::create(&src).unwrap());
    z.start_file("in/side.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    z.write_all(b"inside").unwrap();
    z.finish().unwrap();
    match run(
        "arc.extract",
        vec![src.to_string_lossy().into()],
        "folder",
        serde_json::json!({}),
        d.path(),
    ) {
        JobStatus::Done { outputs, report } => {
            assert_eq!(
                outputs,
                vec![d
                    .path()
                    .join("pack (extracted)")
                    .to_string_lossy()
                    .into_owned()]
            );
            assert_eq!(
                std::fs::read(d.path().join("pack (extracted)/in/side.txt")).unwrap(),
                b"inside"
            );
            let files = report.metrics.iter().find(|m| m.name == "Files").unwrap();
            assert_eq!((files.before, files.after), (1, 1));
            assert_eq!(report.verdict(), Verdict::Exact);
        }
        other => panic!("{other:?}"),
    }
}
