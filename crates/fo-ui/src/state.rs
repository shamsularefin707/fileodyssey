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

use fo_types::{OptionKind, OptionSpec};

fn applies(spec: &OptionSpec, target: &str) -> bool {
    spec.targets.is_empty() || spec.targets.iter().any(|t| t == target)
}

fn default_value(kind: &OptionKind) -> serde_json::Value {
    match kind {
        OptionKind::Slider { default, .. } | OptionKind::Number { default, .. } => {
            (*default).into()
        }
        OptionKind::Toggle { default } => (*default).into(),
        OptionKind::Choice { default, .. } => default.clone().into(),
    }
}

/// Defaults for every option that applies to `target`.
pub fn default_options(specs: &[OptionSpec], target: &str) -> serde_json::Value {
    let map = specs
        .iter()
        .filter(|s| applies(s, target))
        .map(|s| (s.key.clone(), default_value(&s.kind)))
        .collect();
    serde_json::Value::Object(map)
}

/// Defaults with the user's values on top (only for keys that apply).
pub fn merged_options(
    specs: &[OptionSpec],
    target: &str,
    user: Option<&serde_json::Value>,
) -> serde_json::Value {
    let mut v = default_options(specs, target);
    if let (Some(obj), Some(serde_json::Value::Object(u))) = (v.as_object_mut(), user) {
        for (k, val) in u {
            if obj.contains_key(k) {
                obj.insert(k.clone(), val.clone());
            }
        }
    }
    v
}

/// Options to show for `target`, honouring `show_if` against current `values`.
pub fn visible_options<'a>(
    specs: &'a [OptionSpec],
    target: &str,
    values: &serde_json::Value,
) -> Vec<&'a OptionSpec> {
    specs
        .iter()
        .filter(|s| applies(s, target))
        .filter(|s| match &s.show_if {
            None => true,
            Some((key, want)) => {
                let have = values.get(key).and_then(|v| v.as_str()).unwrap_or("");
                match want.strip_prefix('!') {
                    Some(not) => have != not,
                    None => have == want,
                }
            }
        })
        .collect()
}
