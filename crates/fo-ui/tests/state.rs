use fo_types::*;
use fo_ui::state::{apply_event, can_run, merge_files, visible_tabs, JobRow};

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

#[test]
fn run_needs_an_output_folder_and_something_to_run() {
    assert!(!can_run("", 2));
    assert!(!can_run("/home/you/Documents/FileOdyssey", 0));
    assert!(can_run("/home/you/Documents/FileOdyssey", 1));
}

fn spec(
    key: &str,
    kind: OptionKind,
    targets: &[&str],
    show_if: Option<(&str, &str)>,
) -> OptionSpec {
    OptionSpec {
        key: key.into(),
        label: key.into(),
        kind,
        targets: targets.iter().map(|t| t.to_string()).collect(),
        show_if: show_if.map(|(k, v)| (k.into(), v.into())),
    }
}

fn resize_specs() -> Vec<OptionSpec> {
    vec![
        spec(
            "quality",
            OptionKind::Slider {
                min: 1,
                max: 100,
                step: 1,
                default: 90,
                unit: "".into(),
            },
            &["jpg", "webp"],
            None,
        ),
        spec("keep", OptionKind::Toggle { default: true }, &[], None),
        spec(
            "mode",
            OptionKind::Choice {
                choices: vec![("fit".into(), "Fit".into()), ("percent".into(), "%".into())],
                default: "fit".into(),
            },
            &[],
            None,
        ),
        spec(
            "percent",
            OptionKind::Number {
                min: 1,
                max: 400,
                default: 50,
                unit: "%".into(),
            },
            &[],
            Some(("mode", "percent")),
        ),
        spec(
            "width",
            OptionKind::Number {
                min: 1,
                max: 30000,
                default: 1920,
                unit: "px".into(),
            },
            &[],
            Some(("mode", "!percent")),
        ),
    ]
}

#[test]
fn default_options_takes_spec_defaults() {
    let v = fo_ui::state::default_options(&resize_specs(), "jpg");
    assert_eq!(
        v,
        serde_json::json!({"quality": 90, "keep": true, "mode": "fit", "percent": 50, "width": 1920})
    );
}

#[test]
fn visible_options_filters_by_target() {
    let values = fo_ui::state::default_options(&resize_specs(), "png");
    let keys: Vec<_> = fo_ui::state::visible_options(&resize_specs(), "png", &values)
        .iter()
        .map(|o| o.key.clone())
        .collect();
    assert_eq!(keys, vec!["keep", "mode", "width"]);
}

#[test]
fn visible_options_follow_show_if() {
    let values = serde_json::json!({"mode": "percent"});
    let keys: Vec<_> = fo_ui::state::visible_options(&resize_specs(), "jpg", &values)
        .iter()
        .map(|o| o.key.clone())
        .collect();
    assert_eq!(keys, vec!["quality", "keep", "mode", "percent"]);
}

#[test]
fn merged_options_overlay_user_values_on_defaults() {
    let user = serde_json::json!({"quality": 40});
    let v = fo_ui::state::merged_options(&resize_specs(), "jpg", Some(&user));
    assert_eq!(v["quality"], 40);
    assert_eq!(v["mode"], "fit");
}

fn tool_info(id: &str, category: Category, combines: bool) -> ToolInfo {
    ToolInfo {
        id: id.into(),
        label: id.into(),
        category,
        targets: vec![],
        best_effort: false,
        platforms: vec![Platform::Desktop],
        options: vec![],
        combines,
    }
}

#[test]
fn combine_candidates_lists_archive_for_any_file_on_compress_tab() {
    let archive = tool_info("arc.create", Category::Compress, true);
    let files = vec![(
        "/a.bin".to_string(),
        vec![archive.clone(), tool_info("x.y", Category::Compress, false)],
    )];
    let got = fo_ui::state::combine_candidates(&files, Category::Compress);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0.id, "arc.create");
    assert_eq!(got[0].1, vec!["/a.bin".to_string()]);
    assert!(fo_ui::state::combine_candidates(&files, Category::Images).is_empty());
}

#[test]
fn combine_candidates_needs_two_images_for_pdf() {
    let pdf = tool_info("img.to_pdf", Category::Images, true);
    let one = vec![("/a.png".to_string(), vec![pdf.clone()])];
    assert!(fo_ui::state::combine_candidates(&one, Category::Images).is_empty());
    let two = vec![
        ("/a.png".to_string(), vec![pdf.clone()]),
        ("/b.txt".to_string(), vec![]),
        ("/c.jpg".to_string(), vec![pdf]),
    ];
    let got = fo_ui::state::combine_candidates(&two, Category::Images);
    assert_eq!(got[0].1, vec!["/a.png".to_string(), "/c.jpg".to_string()]);
}
