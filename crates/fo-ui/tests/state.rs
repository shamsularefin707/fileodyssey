use fo_types::*;
use fo_ui::state::{apply_event, merge_files, visible_tabs, JobRow};

fn file(path: &str) -> DetectedFile {
    DetectedFile {
        path: path.into(),
        name: path.rsplit('/').next().unwrap().into(),
        size: 1,
        kind: FileKind::Pdf,
    }
}

fn ev(job: &str, input: &str, status: JobStatus) -> JobEvent {
    JobEvent {
        job_id: job.into(),
        input: input.into(),
        status,
    }
}

#[test]
fn scan_tab_hidden_on_desktop() {
    let tabs = visible_tabs(Platform::Desktop);
    assert!(!tabs.contains(&Category::Scan));
    assert_eq!(
        tabs,
        vec![
            Category::Convert,
            Category::Compress,
            Category::Images,
            Category::Read
        ]
    );
}

#[test]
fn scan_tab_shown_on_android() {
    assert_eq!(
        visible_tabs(Platform::Android).last(),
        Some(&Category::Scan)
    );
}

#[test]
fn merge_dedupes_by_path_and_keeps_order() {
    let existing = vec![file("/a.pdf"), file("/b.pdf")];
    let merged = merge_files(
        &existing,
        vec![file("/b.pdf"), file("/c.pdf"), file("/c.pdf")],
    );
    let paths: Vec<_> = merged.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["/a.pdf", "/b.pdf", "/c.pdf"]);
}

#[test]
fn apply_event_upserts_running_then_done() {
    let mut jobs: Vec<JobRow> = vec![];
    apply_event(&mut jobs, ev("j", "/a.pdf", JobStatus::Queued));
    apply_event(&mut jobs, ev("j", "/b.pdf", JobStatus::Queued));
    apply_event(
        &mut jobs,
        ev("j", "/a.pdf", JobStatus::Running { fraction: 0.4 }),
    );
    assert_eq!(jobs.len(), 2);
    assert_eq!(jobs[0].status, JobStatus::Running { fraction: 0.4 });
    apply_event(
        &mut jobs,
        ev(
            "j",
            "/a.pdf",
            JobStatus::Done {
                outputs: vec!["/a.docx".into()],
                report: FidelityReport::default(),
            },
        ),
    );
    assert_eq!(jobs.len(), 2);
    assert!(matches!(jobs[0].status, JobStatus::Done { .. }));
    assert_eq!(jobs[1].input, "/b.pdf");
}

#[test]
fn late_running_event_never_overwrites_finished() {
    let mut jobs: Vec<JobRow> = vec![];
    apply_event(&mut jobs, ev("j", "/a.pdf", JobStatus::Cancelled));
    apply_event(
        &mut jobs,
        ev("j", "/a.pdf", JobStatus::Running { fraction: 0.9 }),
    );
    assert_eq!(jobs[0].status, JobStatus::Cancelled);
}
