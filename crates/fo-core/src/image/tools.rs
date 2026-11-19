//! The image tools offered in the UI.

use super::decode::{decodable, decode, Decoded};
use super::encode::{encode, EncodeOpts, WRITE_TARGETS};
use super::ops::{encode_to_size, optimise_png, resize, ResizeSpec};
use crate::detect::detect_kind;
use crate::tool::{Ctx, Tool, ToolError, ToolOutput};
use fo_types::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub fn image_tools() -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(ImageTool::Convert),
        Arc::new(ImageTool::Resize),
        Arc::new(ImageTool::Compress),
        Arc::new(ImageTool::QrRead),
        Arc::new(ImagesToPdf),
    ]
}

/// Several images → one PDF, one page each, in the order given.
struct ImagesToPdf;

impl Tool for ImagesToPdf {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            id: s("img.to_pdf"),
            label: s("Combine into one PDF"),
            category: Category::Images,
            targets: targets(&["pdf"]),
            best_effort: false,
            platforms: vec![Platform::Desktop, Platform::Android],
            options: vec![OptionSpec {
                key: s("page"),
                label: s("Page size"),
                kind: OptionKind::Choice {
                    choices: vec![
                        (s("image"), s("Same as each image")),
                        (s("a4"), s("A4")),
                        (s("letter"), s("Letter")),
                    ],
                    default: s("image"),
                },
                targets: vec![],
                show_if: None,
            }],
            combines: true,
        }
    }

    fn accepts(&self, kind: &FileKind) -> bool {
        decodable(kind)
    }

    fn combines(&self) -> bool {
        true
    }

    fn run(&self, input: &Path, ctx: &Ctx) -> Result<ToolOutput, ToolError> {
        self.run_many(&[input.to_path_buf()], ctx)
    }

    fn run_many(&self, inputs: &[PathBuf], ctx: &Ctx) -> Result<ToolOutput, ToolError> {
        use super::pdf::{write_pdf, PageSize, PdfImage};
        let page = match ctx.options.get("page").and_then(|v| v.as_str()) {
            Some("a4") => PageSize::A4,
            Some("letter") => PageSize::Letter,
            _ => PageSize::Image,
        };
        let mut decoded = Vec::with_capacity(inputs.len());
        let mut warnings = vec![];
        for (i, path) in inputs.iter().enumerate() {
            if ctx.cancel.is_cancelled() {
                return Err(ToolError::Cancelled);
            }
            let d = decode(path, &detect_kind(path)?)?;
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if d.frames.len() > 1 {
                warnings.push(format!(
                    "{name}: only the first of {} frames was used",
                    d.frames.len()
                ));
            }
            if d.source_bytes.is_none() && d.bit_depth > 8 {
                warnings.push(format!(
                    "{name}: colour depth reduced from {} to 8 bits",
                    d.bit_depth
                ));
            }
            decoded.push(d);
            (ctx.progress)((i + 1) as f32 / (inputs.len() + 1) as f32);
        }
        let pages: Vec<PdfImage> = decoded
            .iter()
            .map(|d| PdfImage {
                image: d.first(),
                jpeg: d.source_bytes.clone(),
            })
            .collect();
        let out = out_path(ctx, "pdf");
        write_pdf(&pages, page, &out)
            .map_err(|e| ToolError::Failed(format!("Couldn't save the PDF: {e}")))?;
        let n = inputs.len() as u64;
        Ok(ToolOutput {
            files: vec![(out, s("pdf"))],
            report: FidelityReport {
                metrics: vec![Metric {
                    name: s("Pages"),
                    before: n,
                    after: pages.len() as u64,
                }],
                text_similarity: None,
                warnings,
            },
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum ImageTool {
    Convert,
    Resize,
    Compress,
    QrRead,
}

fn s(v: &str) -> String {
    v.to_owned()
}

fn targets(list: &[&str]) -> Vec<String> {
    list.iter().map(|t| s(t)).collect()
}

fn quality_option(default: u32, for_targets: &[&str]) -> OptionSpec {
    OptionSpec {
        key: s("quality"),
        label: s("Quality"),
        kind: OptionKind::Slider {
            min: 1,
            max: 100,
            step: 1,
            default,
            unit: s(""),
        },
        targets: targets(for_targets),
        show_if: None,
    }
}

fn number(key: &str, label: &str, min: u32, max: u32, default: u32, unit: &str) -> OptionSpec {
    OptionSpec {
        key: s(key),
        label: s(label),
        kind: OptionKind::Number {
            min,
            max,
            default,
            unit: s(unit),
        },
        targets: vec![],
        show_if: None,
    }
}

fn shown_if(mut o: OptionSpec, key: &str, value: &str) -> OptionSpec {
    o.show_if = Some((s(key), s(value)));
    o
}

const LOSSY: &[&str] = &["jpg", "webp", "avif"];

impl ImageTool {
    fn options(self) -> Vec<OptionSpec> {
        match self {
            ImageTool::Convert => vec![
                quality_option(90, LOSSY),
                OptionSpec {
                    key: s("keep_metadata"),
                    label: s("Keep photo details (EXIF, colour profile)"),
                    kind: OptionKind::Toggle { default: true },
                    targets: vec![],
                    show_if: None,
                },
            ],
            ImageTool::Resize => vec![
                OptionSpec {
                    key: s("mode"),
                    label: s("Resize by"),
                    kind: OptionKind::Choice {
                        choices: vec![
                            (s("fit"), s("Fit inside a box")),
                            (s("percent"), s("Percentage")),
                            (s("exact"), s("Exact size")),
                        ],
                        default: s("fit"),
                    },
                    targets: vec![],
                    show_if: None,
                },
                shown_if(
                    number("width", "Width", 1, 30000, 1920, "px"),
                    "mode",
                    "!percent",
                ),
                shown_if(
                    number("height", "Height", 1, 30000, 1080, "px"),
                    "mode",
                    "!percent",
                ),
                shown_if(
                    number("percent", "Percent", 1, 400, 50, "%"),
                    "mode",
                    "percent",
                ),
                quality_option(90, LOSSY),
            ],
            ImageTool::Compress => vec![
                number(
                    "max_kb",
                    "Largest file size (0 = no limit)",
                    0,
                    100_000,
                    0,
                    "KB",
                ),
                quality_option(80, &["same", "jpg", "webp", "avif"]),
            ],
            ImageTool::QrRead => vec![],
        }
    }
}

impl Tool for ImageTool {
    fn info(&self) -> ToolInfo {
        let (id, label, category, t): (&str, &str, Category, Vec<String>) = match self {
            ImageTool::Convert => (
                "img.convert",
                "Convert image",
                Category::Images,
                targets(WRITE_TARGETS),
            ),
            ImageTool::Resize => (
                "img.resize",
                "Resize image",
                Category::Images,
                std::iter::once(s("same"))
                    .chain(WRITE_TARGETS.iter().filter(|t| **t != "pdf").map(|t| s(t)))
                    .collect(),
            ),
            ImageTool::Compress => (
                "img.compress",
                "Compress image",
                Category::Compress,
                targets(&["same", "jpg", "webp", "avif"]),
            ),
            ImageTool::QrRead => (
                "qr.read",
                "Read QR code or barcode",
                Category::Images,
                targets(&["txt"]),
            ),
        };
        ToolInfo {
            id: s(id),
            label: s(label),
            category,
            targets: t,
            best_effort: false,
            platforms: vec![Platform::Desktop, Platform::Android],
            options: self.options(),
            combines: false,
        }
    }

    fn accepts(&self, kind: &FileKind) -> bool {
        decodable(kind)
    }

    fn run(&self, input: &Path, ctx: &Ctx) -> Result<ToolOutput, ToolError> {
        let kind = detect_kind(input)?;
        (ctx.progress)(0.1);
        let mut d = decode(input, &kind)?;
        if ctx.cancel.is_cancelled() {
            return Err(ToolError::Cancelled);
        }
        (ctx.progress)(0.4);
        let opt_u = |k: &str, def: u32| {
            ctx.options
                .get(k)
                .and_then(|v| v.as_u64())
                .map(|v| v as u32)
                .unwrap_or(def)
        };
        let opt_b =
            |k: &str, def: bool| ctx.options.get(k).and_then(|v| v.as_bool()).unwrap_or(def);
        let quality = |def: u32| opt_u("quality", def).clamp(1, 100) as u8;

        let (target, report) = match self {
            ImageTool::Convert => {
                let target = ctx.target.unwrap_or("png");
                if !opt_b("keep_metadata", true) {
                    d.icc = None;
                    d.exif = None;
                }
                let out = out_path(ctx, target);
                let r = encode(
                    &d,
                    target,
                    &EncodeOpts {
                        quality: quality(90),
                        keep_metadata: true,
                        lossless: false,
                    },
                    &out,
                )?;
                (target.to_owned(), r)
            }
            ImageTool::Resize => {
                let target = resolve_same(ctx.target.unwrap_or("same"), &d);
                let spec = match ctx
                    .options
                    .get("mode")
                    .and_then(|v| v.as_str())
                    .unwrap_or("fit")
                {
                    "percent" => ResizeSpec::Percent(opt_u("percent", 50).clamp(1, 400)),
                    "exact" => ResizeSpec::Exact {
                        w: opt_u("width", 1920),
                        h: opt_u("height", 1080),
                    },
                    _ => ResizeSpec::Fit {
                        w: opt_u("width", 1920),
                        h: opt_u("height", 1080),
                    },
                };
                let (ow, oh) = (d.first().width(), d.first().height());
                let resized = Decoded {
                    frames: d
                        .frames
                        .iter()
                        .map(|f| super::decode::Frame {
                            image: resize(&f.image, &spec),
                            delay_ms: f.delay_ms,
                        })
                        .collect(),
                    source_bytes: None,
                    ..d.clone()
                };
                let out = out_path(ctx, &target);
                let mut r = encode(
                    &resized,
                    &target,
                    &EncodeOpts {
                        quality: quality(90),
                        ..Default::default()
                    },
                    &out,
                )?;
                set_before(&mut r, "Width", ow as u64);
                set_before(&mut r, "Height", oh as u64);
                (target, r)
            }
            ImageTool::Compress => {
                let target = resolve_same(ctx.target.unwrap_or("same"), &d);
                let out = out_path(ctx, &target);
                let max_kb = opt_u("max_kb", 0) as u64;
                let mut r = if max_kb > 0 {
                    encode_to_size(&d, &target, max_kb * 1024, &out)?
                } else if target == "png" {
                    let r = encode(&d, "png", &EncodeOpts::default(), &out)?;
                    optimise_png(&out)?;
                    r
                } else {
                    encode(
                        &d,
                        &target,
                        &EncodeOpts {
                            quality: quality(80),
                            ..Default::default()
                        },
                        &out,
                    )?
                };
                let kb = |p: &Path| {
                    std::fs::metadata(p)
                        .map(|m| m.len().div_ceil(1024))
                        .unwrap_or(0)
                };
                let (before, after) = (kb(input), kb(&out));
                r.metrics.push(Metric {
                    name: s("File size (KB)"),
                    before,
                    after,
                });
                if after >= before {
                    r.warnings
                        .push(s("The result isn't smaller than the original"));
                }
                (target, r)
            }
            ImageTool::QrRead => {
                let codes = crate::qr::read_codes(d.first());
                let out = out_path(ctx, "txt");
                std::fs::write(
                    &out,
                    codes.join("\n") + if codes.is_empty() { "" } else { "\n" },
                )?;
                let mut warnings = vec![];
                if codes.is_empty() {
                    warnings.push(s("No QR code or barcode found"));
                }
                let r = FidelityReport {
                    metrics: vec![Metric {
                        name: s("Codes found"),
                        before: 0,
                        after: codes.len() as u64,
                    }],
                    text_similarity: None,
                    warnings,
                };
                (s("txt"), r)
            }
        };
        (ctx.progress)(1.0);
        Ok(ToolOutput {
            files: vec![(out_path(ctx, &target), target)],
            report,
        })
    }
}

fn out_path(ctx: &Ctx, ext: &str) -> PathBuf {
    ctx.work_dir.join(format!("out.{ext}"))
}

/// "same" means the source format, when we can write it; otherwise PNG.
fn resolve_same(target: &str, d: &Decoded) -> String {
    if target != "same" {
        return target.to_owned();
    }
    let ext = match d.source {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Gif => "gif",
        ImageFormat::Bmp => "bmp",
        ImageFormat::Ico => "ico",
        ImageFormat::Tiff => "tiff",
        ImageFormat::Webp => "webp",
        ImageFormat::Jxl => "jxl",
        ImageFormat::Tga => "tga",
        ImageFormat::Qoi => "qoi",
        ImageFormat::Pnm => "pnm",
        _ => "png",
    };
    ext.to_owned()
}

fn set_before(r: &mut FidelityReport, name: &str, before: u64) {
    if let Some(m) = r.metrics.iter_mut().find(|m| m.name == name) {
        m.before = before;
    }
}
