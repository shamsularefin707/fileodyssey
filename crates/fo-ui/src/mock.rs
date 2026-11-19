//! In-browser stand-in for the Tauri backend (preview and screenshots only).
//! `?platform=android` previews the phone layout; `?demo=1` preloads files,
//! one finished job and an open report.

use fo_types::*;
use serde::Serialize;
use std::cell::RefCell;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

type Handler = Box<dyn Fn(JsValue)>;

thread_local! {
    static LISTENERS: RefCell<HashMap<String, Vec<Handler>>> = RefCell::new(HashMap::new());
    static NEXT_JOB: RefCell<u32> = const { RefCell::new(1) };
}

pub fn listen(event: &str, h: Handler) {
    LISTENERS.with(|l| l.borrow_mut().entry(event.to_owned()).or_default().push(h));
}

pub fn emit<T: Serialize>(event: &str, payload: &T) {
    let ser = serde_wasm_bindgen::Serializer::json_compatible();
    let Ok(v) = payload.serialize(&ser) else {
        return;
    };
    LISTENERS.with(|l| {
        if let Some(hs) = l.borrow().get(event) {
            for h in hs {
                h(v.clone());
            }
        }
    });
}

pub fn query(key: &str) -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    search.trim_start_matches('?').split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key).then(|| v.to_owned())
    })
}

pub const SAMPLE_FILES: &[&str] = &[
    "/home/you/Documents/Quarterly report.pdf",
    "/home/you/Documents/meeting notes.md",
    "/home/you/Pictures/river at dusk.heic",
    "/home/you/Documents/রিপোর্ট ফাইল.docx",
];

fn kind_for(path: &str) -> FileKind {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "pdf" => FileKind::Pdf,
        "md" => FileKind::Markdown,
        "docx" => FileKind::Docx,
        "epub" => FileKind::Epub,
        "png" => FileKind::Image(ImageFormat::Png),
        "jpg" | "jpeg" => FileKind::Image(ImageFormat::Jpeg),
        "heic" => FileKind::Image(ImageFormat::Heic),
        "zip" => FileKind::Archive(ArchiveFormat::Zip),
        _ => FileKind::Unknown,
    }
}

fn tool(
    id: &str,
    label: &str,
    category: Category,
    targets: &[&str],
    best_effort: bool,
) -> ToolInfo {
    ToolInfo {
        id: id.into(),
        label: label.into(),
        category,
        targets: targets.iter().map(|s| s.to_string()).collect(),
        best_effort,
        platforms: vec![Platform::Desktop, Platform::Android],
        options: vec![],
    }
}

fn tools_for(kind: FileKind) -> Vec<ToolInfo> {
    match kind {
        FileKind::Pdf => vec![
            tool(
                "pdf.convert",
                "Convert PDF",
                Category::Convert,
                &["docx", "md", "html", "txt"],
                true,
            ),
            tool(
                "pdf.compress",
                "Compress PDF",
                Category::Compress,
                &["pdf"],
                false,
            ),
            tool("read.pdf", "Read", Category::Read, &[], false),
        ],
        FileKind::Markdown | FileKind::Docx => vec![
            tool(
                "doc.convert",
                "Convert document",
                Category::Convert,
                &["docx", "pdf", "html", "epub", "odt", "md", "txt"],
                false,
            ),
            tool("read.doc", "Read", Category::Read, &[], false),
        ],
        FileKind::Image(_) => vec![
            tool(
                "img.convert",
                "Convert image",
                Category::Images,
                &["jpg", "png", "webp", "avif", "jxl"],
                false,
            ),
            tool(
                "img.compress",
                "Compress image",
                Category::Compress,
                &["jpg", "webp"],
                false,
            ),
        ],
        _ => vec![],
    }
}

fn detected(path: &str) -> DetectedFile {
    DetectedFile {
        path: path.into(),
        name: path.rsplit('/').next().unwrap_or(path).into(),
        size: 120_000 + path.len() as u64 * 731,
        kind: kind_for(path),
    }
}

fn arg<T: serde::de::DeserializeOwned>(args: &JsValue, key: &str) -> Option<T> {
    let v = js_sys::Reflect::get(args, &JsValue::from_str(key)).ok()?;
    serde_wasm_bindgen::from_value(v).ok()
}

fn to_js<T: Serialize>(v: &T) -> JsValue {
    v.serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .unwrap_or(JsValue::NULL)
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Detected {
    Ok(DetectedFile),
}

pub async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, String> {
    Ok(match cmd {
        "platform" => to_js(&match query("platform").as_deref() {
            Some("android") => Platform::Android,
            _ => Platform::Desktop,
        }),
        "detect_files" => {
            let paths: Vec<String> = arg(&args, "paths").unwrap_or_default();
            to_js(
                &paths
                    .iter()
                    .map(|p| Detected::Ok(detected(p)))
                    .collect::<Vec<_>>(),
            )
        }
        "tools_for" => {
            let kind: FileKind = arg(&args, "kind").unwrap_or(FileKind::Unknown);
            let own = match kind {
                FileKind::Docx => "docx",
                FileKind::Markdown => "md",
                _ => "",
            };
            let mut tools = tools_for(kind);
            for t in &mut tools {
                t.targets.retain(|x| x != own);
            }
            to_js(&tools)
        }
        "default_out_dir" => to_js(&"/home/you/Documents/FileOdyssey"),
        "pick_files" => to_js(&SAMPLE_FILES.to_vec()),
        "pick_folder" => to_js(&Some("/home/you/Desktop")),
        "start_job" => {
            let req: JobRequest = arg(&args, "req").ok_or("bad request")?;
            let id = NEXT_JOB.with(|n| {
                let mut n = n.borrow_mut();
                *n += 1;
                format!("job-{n}")
            });
            for input in &req.inputs {
                emit(
                    "job-event",
                    &JobEvent {
                        job_id: id.clone(),
                        input: input.clone(),
                        status: JobStatus::Queued,
                    },
                );
                simulate(
                    id.clone(),
                    input.clone(),
                    req.target.clone().unwrap_or_default(),
                    0,
                );
            }
            to_js(&id)
        }
        "cancel_job" => {
            let id: String = arg(&args, "jobId").unwrap_or_default();
            CANCELLED.with(|c| c.borrow_mut().push(id));
            JsValue::NULL
        }
        _ => JsValue::NULL,
    })
}

thread_local! {
    static CANCELLED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

pub fn sample_report(best_effort: bool) -> FidelityReport {
    if best_effort {
        FidelityReport {
            metrics: vec![
                Metric {
                    name: "Headings".into(),
                    before: 12,
                    after: 12,
                },
                Metric {
                    name: "Tables".into(),
                    before: 3,
                    after: 3,
                },
                Metric {
                    name: "Images".into(),
                    before: 4,
                    after: 4,
                },
                Metric {
                    name: "Links".into(),
                    before: 9,
                    after: 8,
                },
            ],
            text_similarity: Some(0.996),
            warnings: vec!["One link pointed inside the PDF and was kept as plain text.".into()],
        }
    } else {
        FidelityReport {
            metrics: vec![
                Metric {
                    name: "Headings".into(),
                    before: 6,
                    after: 6,
                },
                Metric {
                    name: "Lists".into(),
                    before: 4,
                    after: 4,
                },
            ],
            text_similarity: Some(1.0),
            warnings: vec![],
        }
    }
}

fn simulate(job: String, input: String, target: String, step: u32) {
    let window = web_sys::window().expect("window");
    let cb = Closure::once_into_js(move || {
        if CANCELLED.with(|c| c.borrow().contains(&job)) {
            emit(
                "job-event",
                &JobEvent {
                    job_id: job,
                    input,
                    status: JobStatus::Cancelled,
                },
            );
            return;
        }
        if step < 10 {
            let fraction = step as f32 / 10.0;
            emit(
                "job-event",
                &JobEvent {
                    job_id: job.clone(),
                    input: input.clone(),
                    status: JobStatus::Running { fraction },
                },
            );
            simulate(job, input, target, step + 1);
        } else {
            let stem = input
                .rsplit_once('.')
                .map(|(s, _)| s)
                .unwrap_or(&input)
                .to_owned();
            let report = sample_report(input.ends_with(".pdf"));
            let outputs = vec![format!("{stem} (converted).{target}")];
            emit(
                "job-event",
                &JobEvent {
                    job_id: job,
                    input,
                    status: JobStatus::Done { outputs, report },
                },
            );
        }
    });
    let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(cb.unchecked_ref(), 180);
}
