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
    "/home/you/Pictures/river at dusk.jpg",
    "/home/you/Pictures/lichen close-up.png",
    "/home/you/Documents/রিপোর্ট ফাইল.docx",
    "/home/you/Downloads/holiday photos.zip",
];

fn kind_for(path: &str) -> FileKind {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "pdf" => FileKind::Pdf,
        "md" => FileKind::Markdown,
        "docx" => FileKind::Docx,
        "epub" => FileKind::Epub,
        "png" => FileKind::Image(ImageFormat::Png),
        "gif" => FileKind::Image(ImageFormat::Gif),
        "webp" => FileKind::Image(ImageFormat::Webp),
        "jpg" | "jpeg" => FileKind::Image(ImageFormat::Jpeg),
        "heic" => FileKind::Image(ImageFormat::Heic),
        "zip" => FileKind::Archive(ArchiveFormat::Zip),
        "7z" => FileKind::Archive(ArchiveFormat::SevenZ),
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
        combines: false,
    }
}

fn level_option() -> OptionSpec {
    opt(
        "level",
        "Compression",
        OptionKind::Choice {
            choices: vec![
                ("lossless".into(), "Lossless (nothing changes)".into()),
                ("balanced".into(), "Balanced (smaller images)".into()),
                ("strong".into(), "Strong (smallest)".into()),
            ],
            default: "balanced".into(),
        },
        &[],
        None,
    )
}

/// Mirrors fo-core's archive, PDF and Office compression tools.
fn compress_tools(kind: FileKind) -> Vec<ToolInfo> {
    let mut archive = tool(
        "arc.create",
        "Put files in an archive",
        Category::Compress,
        &["zip", "7z", "tar", "tar.gz", "tar.xz", "tar.zst"],
        false,
    );
    archive.combines = true;
    archive.options = vec![opt(
        "level",
        "Compression level",
        OptionKind::Slider {
            min: 1,
            max: 9,
            step: 1,
            default: 6,
            unit: "".into(),
        },
        &["zip", "7z", "tar.gz", "tar.xz"],
        None,
    )];
    let mut out = vec![archive];
    match kind {
        FileKind::Pdf => {
            let mut t = tool(
                "pdf.compress",
                "Compress PDF",
                Category::Compress,
                &["pdf"],
                false,
            );
            t.options = vec![level_option()];
            out.push(t);
        }
        FileKind::Docx | FileKind::Pptx | FileKind::Xlsx | FileKind::Odt | FileKind::Epub => {
            let mut t = tool(
                "office.compress",
                "Compress document",
                Category::Compress,
                &["same"],
                false,
            );
            t.options = vec![level_option()];
            out.push(t);
        }
        FileKind::Archive(_) => out.push(tool(
            "arc.extract",
            "Extract archive",
            Category::Compress,
            &["folder"],
            false,
        )),
        _ => {}
    }
    out
}

fn tools_for(kind: FileKind) -> Vec<ToolInfo> {
    let mut tools = tools_for_kind(kind);
    tools.retain(|t| t.id != "pdf.compress");
    tools.extend(compress_tools(kind));
    tools
}

fn tools_for_kind(kind: FileKind) -> Vec<ToolInfo> {
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
        FileKind::Image(f) if !matches!(f, ImageFormat::Avif | ImageFormat::Heic) => image_tools(),
        _ => vec![],
    }
}

fn opt(
    key: &str,
    label: &str,
    kind: OptionKind,
    targets: &[&str],
    show_if: Option<(&str, &str)>,
) -> OptionSpec {
    OptionSpec {
        key: key.into(),
        label: label.into(),
        kind,
        targets: targets.iter().map(|t| t.to_string()).collect(),
        show_if: show_if.map(|(k, v)| (k.into(), v.into())),
    }
}

/// Mirrors fo-core's image tools (fo-core itself doesn't build for wasm).
fn image_tools() -> Vec<ToolInfo> {
    const WRITE: &[&str] = &[
        "png", "jpg", "webp", "avif", "jxl", "gif", "bmp", "ico", "tiff", "tga", "qoi", "pnm",
        "pdf",
    ];
    let quality = |d| {
        opt(
            "quality",
            "Quality",
            OptionKind::Slider {
                min: 1,
                max: 100,
                step: 1,
                default: d,
                unit: "".into(),
            },
            &["jpg", "webp", "avif"],
            None,
        )
    };
    let num = |k: &str, l: &str, max, d, u: &str, si| {
        opt(
            k,
            l,
            OptionKind::Number {
                min: 1,
                max,
                default: d,
                unit: u.into(),
            },
            &[],
            si,
        )
    };
    let mut convert = tool(
        "img.convert",
        "Convert image",
        Category::Images,
        WRITE,
        false,
    );
    convert.options = vec![
        quality(90),
        opt(
            "keep_metadata",
            "Keep photo details (EXIF, colour profile)",
            OptionKind::Toggle { default: true },
            &[],
            None,
        ),
    ];
    let mut resize_targets = vec!["same"];
    resize_targets.extend(WRITE.iter().filter(|t| **t != "pdf"));
    let mut resize = tool(
        "img.resize",
        "Resize image",
        Category::Images,
        &resize_targets,
        false,
    );
    resize.options = vec![
        opt(
            "mode",
            "Resize by",
            OptionKind::Choice {
                choices: vec![
                    ("fit".into(), "Fit inside a box".into()),
                    ("percent".into(), "Percentage".into()),
                    ("exact".into(), "Exact size".into()),
                ],
                default: "fit".into(),
            },
            &[],
            None,
        ),
        num(
            "width",
            "Width",
            30000,
            1920,
            "px",
            Some(("mode", "!percent")),
        ),
        num(
            "height",
            "Height",
            30000,
            1080,
            "px",
            Some(("mode", "!percent")),
        ),
        num(
            "percent",
            "Percent",
            400,
            50,
            "%",
            Some(("mode", "percent")),
        ),
        quality(90),
    ];
    let mut compress = tool(
        "img.compress",
        "Compress image",
        Category::Compress,
        &["same", "jpg", "webp", "avif"],
        false,
    );
    compress.options = vec![
        opt(
            "max_kb",
            "Largest file size (0 = no limit)",
            OptionKind::Number {
                min: 0,
                max: 100000,
                default: 0,
                unit: "KB".into(),
            },
            &[],
            None,
        ),
        opt(
            "quality",
            "Quality",
            OptionKind::Slider {
                min: 1,
                max: 100,
                step: 1,
                default: 80,
                unit: "".into(),
            },
            &["same", "jpg", "webp", "avif"],
            None,
        ),
    ];
    let qr = tool(
        "qr.read",
        "Read QR code or barcode",
        Category::Images,
        &["txt"],
        false,
    );
    let mut pdf = tool(
        "img.to_pdf",
        "Combine into one PDF",
        Category::Images,
        &["pdf"],
        false,
    );
    pdf.combines = true;
    pdf.options = vec![opt(
        "page",
        "Page size",
        OptionKind::Choice {
            choices: vec![
                ("image".into(), "Same as each image".into()),
                ("a4".into(), "A4".into()),
                ("letter".into(), "Letter".into()),
            ],
            default: "image".into(),
        },
        &[],
        None,
    )];
    vec![convert, resize, compress, qr, pdf]
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
            let units: Vec<String> = if req.tool_id == "img.to_pdf" || req.tool_id == "arc.create" {
                req.inputs.first().cloned().into_iter().collect()
            } else {
                req.inputs.clone()
            };
            for input in &units {
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
                    informational: false,
                },
                Metric {
                    name: "Tables".into(),
                    before: 3,
                    after: 3,
                    informational: false,
                },
                Metric {
                    name: "Images".into(),
                    before: 4,
                    after: 4,
                    informational: false,
                },
                Metric {
                    name: "Links".into(),
                    before: 9,
                    after: 8,
                    informational: false,
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
                    informational: false,
                },
                Metric {
                    name: "Lists".into(),
                    before: 4,
                    after: 4,
                    informational: false,
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
            let name = match target.as_str() {
                "folder" => format!("{stem} (extracted)"),
                "zip" | "7z" | "tar" | "tar.gz" | "tar.xz" | "tar.zst" => {
                    format!("{stem} (archive).{target}")
                }
                _ => format!("{stem} (converted).{target}"),
            };
            let outputs = vec![name];
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
