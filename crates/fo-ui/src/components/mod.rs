mod combine;
mod file_list;
mod jobs;
mod pond;
mod rail;
mod report;

use crate::bridge::{invoke, is_preview, listen};
use fo_types::*;
use fo_ui::state::{apply_event, merge_files, JobRow};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A file in the list plus the tools offered for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub file: DetectedFile,
    pub tools: Vec<ToolInfo>,
}

/// Shared app state, provided as context. All fields are Copy signals.
#[derive(Clone, Copy)]
pub struct AppCtx {
    pub platform: RwSignal<Platform>,
    pub tab: RwSignal<Category>,
    pub entries: RwSignal<Vec<Entry>>,
    /// path → "tool_id|target"
    pub choices: RwSignal<HashMap<String, String>>,
    pub jobs: RwSignal<Vec<JobRow>>,
    pub out_dir: RwSignal<String>,
    pub report: RwSignal<Option<(String, FidelityReport)>>,
    pub errors: RwSignal<Vec<String>>,
    pub ripple: RwSignal<u32>,
    /// path → option values the user changed
    pub options: RwSignal<HashMap<String, serde_json::Value>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Detected {
    Ok(DetectedFile),
    Err(String),
}

#[derive(Serialize)]
struct Paths {
    paths: Vec<String>,
}

#[derive(Serialize)]
struct KindArg {
    kind: FileKind,
}

#[derive(Serialize)]
struct NoArgs {}

pub fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned()
}

impl AppCtx {
    /// Detect `paths`, look up their tools and merge them into the list.
    pub fn add_paths(self, paths: Vec<String>) {
        if paths.is_empty() {
            return;
        }
        self.ripple.update(|n| *n += 1);
        spawn_local(async move {
            let detected: Vec<Detected> = invoke("detect_files", &Paths { paths })
                .await
                .unwrap_or_default();
            let mut new = Vec::new();
            let mut errs = Vec::new();
            for d in detected {
                match d {
                    Detected::Ok(f) => new.push(f),
                    Detected::Err(e) => errs.push(e),
                }
            }
            let mut entries = Vec::new();
            for f in new {
                let tools: Vec<ToolInfo> = invoke("tools_for", &KindArg { kind: f.kind })
                    .await
                    .unwrap_or_default();
                entries.push(Entry { file: f, tools });
            }
            self.entries.update(|list| {
                let existing: Vec<DetectedFile> = list.iter().map(|e| e.file.clone()).collect();
                let merged =
                    merge_files(&existing, entries.iter().map(|e| e.file.clone()).collect());
                for f in merged.into_iter().skip(existing.len()) {
                    if let Some(e) = entries.iter().find(|e| e.file.path == f.path) {
                        list.push(e.clone());
                    }
                }
            });
            self.errors.set(errs);
        });
    }

    pub fn pick_files(self) {
        spawn_local(async move {
            let paths: Vec<String> = invoke("pick_files", &NoArgs {}).await.unwrap_or_default();
            self.add_paths(paths);
        });
    }
}

#[component]
pub fn App() -> impl IntoView {
    let ctx = AppCtx {
        platform: RwSignal::new(Platform::Desktop),
        tab: RwSignal::new(Category::Convert),
        entries: RwSignal::new(vec![]),
        choices: RwSignal::new(HashMap::new()),
        jobs: RwSignal::new(vec![]),
        out_dir: RwSignal::new(String::new()),
        report: RwSignal::new(None),
        errors: RwSignal::new(vec![]),
        ripple: RwSignal::new(0),
        options: RwSignal::new(HashMap::new()),
    };
    provide_context(ctx);

    listen("job-event", move |ev: JobEvent| {
        ctx.jobs.update(|j| apply_event(j, ev))
    });
    listen("files-dropped", move |paths: Vec<String>| {
        ctx.add_paths(paths)
    });

    spawn_local(async move {
        if let Ok(p) = invoke::<_, Platform>("platform", &NoArgs {}).await {
            ctx.platform.set(p);
        }
        if let Ok(d) = invoke::<_, String>("default_out_dir", &NoArgs {}).await {
            ctx.out_dir.set(d);
        }
        if is_preview() {
            if let Some(tab) = crate::mock::query("tab") {
                if let Ok(c) = serde_json::from_value::<Category>(serde_json::Value::String(tab)) {
                    ctx.tab.set(c);
                }
            }
            if crate::mock::query("demo").is_some() {
                demo(ctx).await;
            }
        }
    });

    view! {
        <div class="shell" class:android=move || ctx.platform.get() == Platform::Android>
            <rail::Rail/>
            <main class="workbench" id="main">
                <header class="intro">
                    <h1>{move || fo_ui::strings::tab_label(ctx.tab.get())}</h1>
                    <p>{move || fo_ui::strings::tab_intro(ctx.tab.get())}</p>
                </header>
                <pond::Pond/>
                <file_list::FileList/>
                <combine::CombineBar/>
                <jobs::Jobs/>
            </main>
            <report::ReportDrawer/>
        </div>
    }
}

/// Preview-only: fill the screen with representative content for screenshots.
async fn demo(ctx: AppCtx) {
    let paths: Vec<String> = crate::mock::SAMPLE_FILES
        .iter()
        .map(|s| s.to_string())
        .collect();
    let detected: Vec<Detected> = invoke("detect_files", &Paths { paths })
        .await
        .unwrap_or_default();
    for d in detected {
        if let Detected::Ok(f) = d {
            let tools: Vec<ToolInfo> = invoke("tools_for", &KindArg { kind: f.kind })
                .await
                .unwrap_or_default();
            ctx.entries.update(|l| l.push(Entry { file: f, tools }));
        }
    }
    let pdf = crate::mock::SAMPLE_FILES[0].to_string();
    let md = crate::mock::SAMPLE_FILES[1].to_string();
    ctx.choices.update(|c| {
        c.insert(pdf.clone(), "pdf.convert|docx".into());
    });
    let done = |input: &str, out: &str, best: bool| JobRow {
        job_id: "demo".into(),
        input: input.into(),
        status: JobStatus::Done {
            outputs: vec![out.into()],
            report: crate::mock::sample_report(best),
        },
    };
    ctx.jobs.set(vec![
        done(
            &pdf,
            "/home/you/Documents/FileOdyssey/Quarterly report (converted).docx",
            true,
        ),
        done(
            &md,
            "/home/you/Documents/FileOdyssey/meeting notes (converted).pdf",
            false,
        ),
        JobRow {
            job_id: "demo2".into(),
            input: "/home/you/Pictures/river at dusk.jpg".into(),
            status: JobStatus::Running { fraction: 0.62 },
        },
    ]);
    if crate::mock::query("report").is_some() {
        ctx.report.set(Some((
            "Quarterly report (converted).docx".into(),
            crate::mock::sample_report(true),
        )));
    }
}
