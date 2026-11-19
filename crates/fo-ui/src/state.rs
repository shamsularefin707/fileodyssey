//! Pure UI state logic, kept free of Leptos so it can be unit-tested natively.

use fo_types::{Category, DetectedFile, JobEvent, JobStatus, Platform};

/// Tabs in rail order. Scanning needs a camera, so it's Android-only.
pub fn visible_tabs(platform: Platform) -> Vec<Category> {
    let mut tabs = vec![
        Category::Convert,
        Category::Compress,
        Category::Images,
        Category::Read,
    ];
    if platform == Platform::Android {
        tabs.push(Category::Scan);
    }
    tabs
}

/// Append `new` files, skipping any path already present. Keeps order.
pub fn merge_files(existing: &[DetectedFile], new: Vec<DetectedFile>) -> Vec<DetectedFile> {
    let mut out = existing.to_vec();
    for f in new {
        if !out.iter().any(|e| e.path == f.path) {
            out.push(f);
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct JobRow {
    pub job_id: String,
    pub input: String,
    pub status: JobStatus,
}

/// Upsert by (job_id, input). A finished row is never moved back to running
/// by a late progress event.
pub fn apply_event(jobs: &mut Vec<JobRow>, ev: JobEvent) {
    match jobs
        .iter_mut()
        .find(|r| r.job_id == ev.job_id && r.input == ev.input)
    {
        Some(row) => {
            if !row.status.is_finished() {
                row.status = ev.status;
            }
        }
        None => jobs.push(JobRow {
            job_id: ev.job_id,
            input: ev.input,
            status: ev.status,
        }),
    }
}

/// "Run all" needs a real output folder (never the app's working dir) and at
/// least one file with a tool chosen.
pub fn can_run(out_dir: &str, runnable: usize) -> bool {
    !out_dir.trim().is_empty() && runnable > 0
}
