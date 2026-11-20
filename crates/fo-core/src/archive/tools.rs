//! Archive tools offered in the UI.

use super::create::{create, FORMATS};
use super::extract::extract;
use super::guard::Budget;
use crate::detect::detect_kind;
use crate::tool::{Ctx, Tool, ToolError, ToolOutput};
use fo_types::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub fn archive_tools() -> Vec<Arc<dyn Tool>> {
    vec![Arc::new(Extract), Arc::new(Create)]
}

/// Many files → one archive.
struct Create;

impl Tool for Create {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            id: s("arc.create"),
            label: s("Put files in an archive"),
            category: Category::Compress,
            targets: FORMATS.iter().map(|f| s(f)).collect(),
            best_effort: false,
            platforms: vec![Platform::Desktop, Platform::Android],
            options: vec![OptionSpec {
                key: s("level"),
                label: s("Compression level"),
                kind: OptionKind::Slider {
                    min: 1,
                    max: 9,
                    step: 1,
                    default: 6,
                    unit: s(""),
                },
                targets: FORMATS
                    .iter()
                    .filter(|f| **f != "tar" && **f != "tar.zst")
                    .map(|f| s(f))
                    .collect(),
                show_if: None,
            }],
            combines: true,
        }
    }

    fn accepts(&self, _kind: &FileKind) -> bool {
        true
    }

    fn combines(&self) -> bool {
        true
    }

    fn output_label(&self) -> &'static str {
        "archive"
    }

    fn run(&self, input: &Path, ctx: &Ctx) -> Result<ToolOutput, ToolError> {
        self.run_many(&[input.to_path_buf()], ctx)
    }

    fn run_many(&self, inputs: &[PathBuf], ctx: &Ctx) -> Result<ToolOutput, ToolError> {
        let format = ctx.target.unwrap_or("zip");
        let level = ctx
            .options
            .get("level")
            .and_then(|v| v.as_u64())
            .unwrap_or(6) as u32;
        let out = ctx.work_dir.join(format!("out.{format}"));
        (ctx.progress)(0.05);
        let r = create(inputs, format, level, &out, ctx.cancel)?;
        let size = std::fs::metadata(&out)?.len();
        let mut warnings = vec![];
        if r.renamed > 0 {
            warnings.push(format!("Renamed {} files with the same name", r.renamed));
        }
        if size > r.input_bytes {
            warnings.push(s(
                "Archive is larger than the files (they were already compressed)",
            ));
        }
        let n = inputs.len() as u64;
        Ok(ToolOutput {
            files: vec![(out, s(format))],
            report: FidelityReport {
                metrics: vec![
                    Metric {
                        name: s("Files"),
                        before: n,
                        after: r.entries,
                        informational: false,
                    },
                    Metric {
                        name: s("Size (KB)"),
                        before: r.input_bytes.div_ceil(1024),
                        after: size.div_ceil(1024),
                        informational: true,
                    },
                ],
                text_similarity: None,
                warnings,
            },
        })
    }
}

fn s(v: &str) -> String {
    v.to_owned()
}

const EXTRACTABLE: &[ArchiveFormat] = &[
    ArchiveFormat::Zip,
    ArchiveFormat::SevenZ,
    ArchiveFormat::Tar,
    ArchiveFormat::TarGz,
    ArchiveFormat::TarXz,
    ArchiveFormat::TarZst,
];

struct Extract;

impl Tool for Extract {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            id: s("arc.extract"),
            label: s("Extract archive"),
            category: Category::Compress,
            targets: vec![s("folder")],
            best_effort: false,
            platforms: vec![Platform::Desktop, Platform::Android],
            options: vec![],
            combines: false,
        }
    }

    fn accepts(&self, kind: &FileKind) -> bool {
        matches!(kind, FileKind::Archive(a) if EXTRACTABLE.contains(a))
    }

    fn output_label(&self) -> &'static str {
        "extracted"
    }

    fn run(&self, input: &Path, ctx: &Ctx) -> Result<ToolOutput, ToolError> {
        let kind = detect_kind(input)?;
        let dest = ctx.work_dir.join("out");
        std::fs::create_dir(&dest)?;
        let len = std::fs::metadata(input)?.len();
        (ctx.progress)(0.05);
        let r = extract(input, &kind, &dest, Budget::new(len), ctx.cancel)?;
        let mut warnings = vec![];
        if r.skipped_unsafe > 0 {
            warnings.push(format!(
                "Skipped {} entries with unsafe paths",
                r.skipped_unsafe
            ));
        }
        if r.skipped_links > 0 {
            warnings.push(format!(
                "Skipped {} links or special files",
                r.skipped_links
            ));
        }
        if r.skipped_unsupported > 0 {
            warnings.push(format!(
                "Skipped {} entries that use unsupported compression or a password",
                r.skipped_unsupported
            ));
        }
        let kb = r.bytes.div_ceil(1024);
        Ok(ToolOutput {
            files: vec![(dest, String::new())],
            report: FidelityReport {
                metrics: vec![
                    Metric {
                        name: s("Files"),
                        before: r.file_entries(),
                        after: r.files,
                        informational: false,
                    },
                    Metric {
                        name: s("Extracted size (KB)"),
                        before: kb,
                        after: kb,
                        informational: true,
                    },
                ],
                text_similarity: None,
                warnings,
            },
        })
    }
}
