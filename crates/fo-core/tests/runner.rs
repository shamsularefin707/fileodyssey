use fo_core::registry::Registry;
use fo_core::runner::Runner;
use fo_core::tool::{Ctx, Tool, ToolError, ToolOutput};
use fo_types::*;
use std::path::Path;
use std::sync::{mpsc, Arc};
use std::time::Duration;
use tempfile::TempDir;

/// Writes the input uppercased. Options: {"sleep_ms": n, "fail": true}.
struct TestTool;

impl Tool for TestTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            id: "test.upper".into(),
            label: "Uppercase".into(),
            category: Category::Convert,
            targets: vec!["txt".into()],
            best_effort: false,
            platforms: vec![Platform::Desktop],
        }
    }
    fn accepts(&self, kind: &FileKind) -> bool {
        matches!(kind, FileKind::Text | FileKind::Markdown)
    }
    fn run(&self, input: &Path, ctx: &Ctx) -> Result<ToolOutput, ToolError> {
        let text = std::fs::read_to_string(input)?;
        let out = ctx.work_dir.join("partial.txt");
        std::fs::write(&out, "partial")?; // leftover if cleanup is broken
        (ctx.progress)(0.5);
        let sleep = ctx
            .options
            .get("sleep_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        for _ in 0..(sleep / 10) {
            if ctx.cancel.is_cancelled() {
                return Err(ToolError::Cancelled);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if ctx.options.get("fail").is_some() {
            return Err(ToolError::Failed("Boom".into()));
        }
        std::fs::write(&out, text.to_uppercase())?;
        Ok(ToolOutput {
            files: vec![(out, "txt".into())],
            report: FidelityReport::default(),
        })
    }
}

fn setup() -> (TempDir, Runner) {
    let mut reg = Registry::new();
    reg.register(Arc::new(TestTool));
    (TempDir::new().unwrap(), Runner::new(Arc::new(reg), 2))
}

fn req(input: &Path, out: &Path, options: serde_json::Value) -> JobRequest {
    JobRequest {
        tool_id: "test.upper".into(),
        inputs: vec![input.to_string_lossy().into()],
        target: Some("txt".into()),
        options,
        out_dir: out.to_string_lossy().into(),
    }
}

/// Submit and collect events until the single input finishes.
fn run(runner: &Runner, r: JobRequest, cancel_after: Option<Duration>) -> Vec<JobEvent> {
    let (tx, rx) = mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    let id = runner.submit(r, Arc::new(move |e| tx.lock().unwrap().send(e).unwrap()));
    if let Some(d) = cancel_after {
        std::thread::sleep(d);
        runner.cancel(&id);
    }
    let mut evs = vec![];
    loop {
        let e = rx.recv_timeout(Duration::from_secs(5)).expect("event");
        let done = e.status.is_finished();
        evs.push(e);
        if done {
            return evs;
        }
    }
}

fn entries(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn done_event_has_final_output_and_report() {
    let (d, runner) = setup();
    let input = d.path().join("নোট.md");
    std::fs::write(&input, "hello").unwrap();
    let evs = run(&runner, req(&input, d.path(), serde_json::json!({})), None);
    let JobStatus::Done { outputs, report } = &evs.last().unwrap().status else {
        panic!("{evs:?}")
    };
    assert_eq!(
        outputs,
        &vec![d
            .path()
            .join("নোট (converted).txt")
            .to_string_lossy()
            .into_owned()]
    );
    assert_eq!(std::fs::read_to_string(&outputs[0]).unwrap(), "HELLO");
    assert_eq!(report.verdict(), Verdict::Exact);
    assert_eq!(evs.last().unwrap().input, input.to_string_lossy());
    assert_eq!(entries(d.path()), vec!["নোট (converted).txt", "নোট.md"]);
}

#[test]
fn events_in_order_queued_running_done() {
    let (d, runner) = setup();
    let input = d.path().join("a.md");
    std::fs::write(&input, "x").unwrap();
    let evs = run(&runner, req(&input, d.path(), serde_json::json!({})), None);
    assert_eq!(evs[0].status, JobStatus::Queued);
    assert!(evs[1..evs.len() - 1]
        .iter()
        .all(|e| matches!(e.status, JobStatus::Running { .. })));
    assert!(evs.len() >= 3);
    assert!(matches!(evs.last().unwrap().status, JobStatus::Done { .. }));
}

#[test]
fn cancel_mid_job_leaves_no_output_and_no_workdir() {
    let (d, runner) = setup();
    let input = d.path().join("a.md");
    std::fs::write(&input, "x").unwrap();
    let evs = run(
        &runner,
        req(&input, d.path(), serde_json::json!({"sleep_ms": 500})),
        Some(Duration::from_millis(50)),
    );
    assert_eq!(evs.last().unwrap().status, JobStatus::Cancelled);
    assert_eq!(entries(d.path()), vec!["a.md"]);
}

#[test]
fn failing_tool_emits_failed_and_cleans_up() {
    let (d, runner) = setup();
    let input = d.path().join("a.md");
    std::fs::write(&input, "x").unwrap();
    let evs = run(
        &runner,
        req(&input, d.path(), serde_json::json!({"fail": true})),
        None,
    );
    assert_eq!(
        evs.last().unwrap().status,
        JobStatus::Failed {
            message: "Boom".into()
        }
    );
    assert_eq!(entries(d.path()), vec!["a.md"]);
}

#[test]
fn unknown_tool_fails_clearly() {
    let (d, runner) = setup();
    let input = d.path().join("a.md");
    std::fs::write(&input, "x").unwrap();
    let mut r = req(&input, d.path(), serde_json::json!({}));
    r.tool_id = "nope".into();
    let evs = run(&runner, r, None);
    assert_eq!(
        evs.last().unwrap().status,
        JobStatus::Failed {
            message: "This tool isn't available".into()
        }
    );
}

#[test]
fn missing_input_fails_clearly() {
    let (d, runner) = setup();
    let evs = run(
        &runner,
        req(&d.path().join("gone.md"), d.path(), serde_json::json!({})),
        None,
    );
    assert_eq!(
        evs.last().unwrap().status,
        JobStatus::Failed {
            message: "File not found: gone.md".into()
        }
    );
    assert!(entries(d.path()).is_empty());
}

#[test]
fn registry_filters_by_kind_and_platform() {
    let mut reg = Registry::new();
    reg.register(Arc::new(TestTool));
    assert_eq!(
        reg.tools_for(&FileKind::Markdown, Platform::Desktop).len(),
        1
    );
    assert!(reg.tools_for(&FileKind::Pdf, Platform::Desktop).is_empty());
    assert!(reg
        .tools_for(&FileKind::Markdown, Platform::Android)
        .is_empty());
    assert!(reg.get("test.upper").is_some());
    assert!(reg.get("nope").is_none());
}
