//! Archive tools offered in the UI.

use super::extract::extract;
use super::guard::Budget;
use crate::detect::detect_kind;
use crate::tool::{Ctx, Tool, ToolError, ToolOutput};
use fo_types::*;
use std::path::Path;
use std::sync::Arc;

pub fn archive_tools() -> Vec<Arc<dyn Tool>> {
    vec![Arc::new(Extract)]
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
                    },
                    Metric {
                        name: s("Extracted size (KB)"),
                        before: kb,
                        after: kb,
                    },
                ],
                text_similarity: None,
                warnings,
            },
        })
    }
}
