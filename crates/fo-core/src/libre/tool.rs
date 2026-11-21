//! `office.convert`: conversions that need LibreOffice (desktop only) —
//! legacy .doc/.xls/.ppt to modern formats, Office and OpenDocument files to
//! PDF with their exact layout, and PDF for documents Pandoc reads (through
//! Pandoc → DOCX → LibreOffice).

use super::{convert, find_soffice, DOWNLOAD_URL};
use crate::detect::detect_file;
use crate::pandoc::ast::{normalise_words, report, stats, Stats};
use crate::pandoc::formats::reader_for;
use crate::pandoc::tool::DocConvert;
use crate::pandoc::PandocEngine;
use crate::tool::{Ctx, Tool, ToolError, ToolOutput};
use fo_types::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Finds `soffice`; swapped out in tests.
pub type Locator = Arc<dyn Fn() -> Option<PathBuf> + Send + Sync>;

/// Longest one LibreOffice run may take.
const LIMIT: Duration = Duration::from_secs(300);

pub struct OfficeConvert {
    doc: DocConvert,
    locate: Locator,
}

impl OfficeConvert {
    pub fn new(pandoc: Arc<dyn PandocEngine>) -> Self {
        Self {
            doc: DocConvert::new(pandoc),
            locate: Arc::new(find_soffice),
        }
    }

    pub fn with_locator(mut self, locate: Locator) -> Self {
        self.locate = locate;
        self
    }

    fn needs() -> Needs {
        Needs {
            message: "Install LibreOffice to enable".into(),
            url: DOWNLOAD_URL.into(),
        }
    }
}

/// Documents Pandoc reads; they reach PDF through DOCX.
fn via_pandoc(kind: FileKind) -> bool {
    matches!(
        kind,
        FileKind::Markdown
            | FileKind::Html
            | FileKind::Text
            | FileKind::Latex
            | FileKind::Rst
            | FileKind::Org
            | FileKind::AsciiDoc
            | FileKind::Epub
    )
}

fn targets_for(kind: FileKind) -> &'static [&'static str] {
    match kind {
        FileKind::Doc => &["docx", "odt", "rtf", "pdf"],
        FileKind::Docx | FileKind::Odt | FileKind::Rtf => &["pdf"],
        FileKind::Xls => &["xlsx", "ods", "csv", "pdf"],
        FileKind::Xlsx => &["ods", "pdf"],
        FileKind::Ods => &["xlsx", "pdf"],
        FileKind::Ppt => &["pptx", "odp", "pdf"],
        FileKind::Pptx => &["odp", "pdf"],
        FileKind::Odp => &["pptx", "pdf"],
        k if via_pandoc(k) => &["pdf"],
        _ => &[],
    }
}

/// LibreOffice's `--convert-to` value for a target.
fn filter(target: &str) -> &'static str {
    match target {
        "docx" => "docx:MS Word 2007 XML",
        "xlsx" => "xlsx:Calc MS Excel 2007 XML",
        "pptx" => "pptx:Impress MS PowerPoint 2007 XML",
        // Comma-separated, double quotes, UTF-8 (76), first line as is.
        "csv" => "csv:Text - txt - csv (StarCalc):44,34,76,1",
        "odt" => "odt",
        "ods" => "ods",
        "odp" => "odp",
        "rtf" => "rtf",
        _ => "pdf",
    }
}

/// The OOXML sibling Pandoc can read, for formats only LibreOffice reads.
fn readable_sibling(kind_or_ext: &str) -> Option<&'static str> {
    Some(match kind_or_ext {
        "doc" => "docx",
        "xls" | "ods" => "xlsx",
        "ppt" | "odp" => "pptx",
        _ => return None,
    })
}

/// Pandoc reader for a file LibreOffice wrote or Pandoc reads directly.
fn pandoc_reader(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "docx" => "docx",
        "odt" => "odt",
        "rtf" => "rtf",
        "xlsx" => "xlsx",
        "pptx" => "pptx",
        "csv" => "csv",
        _ => return None,
    })
}

fn ext_of(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Doc => "doc",
        FileKind::Xls => "xls",
        FileKind::Ppt => "ppt",
        FileKind::Docx => "docx",
        FileKind::Xlsx => "xlsx",
        FileKind::Pptx => "pptx",
        FileKind::Odt => "odt",
        FileKind::Ods => "ods",
        FileKind::Odp => "odp",
        FileKind::Rtf => "rtf",
        _ => "",
    }
}

fn lo_error(e: String) -> ToolError {
    if e == "Cancelled" {
        ToolError::Cancelled
    } else {
        ToolError::Failed(e)
    }
}

/// Words share of `before` found in `after`, in any order: page layout
/// moves running text and list numbers, so order isn't what's measured.
fn recall(before: &[String], after: &[String]) -> f64 {
    if before.is_empty() {
        return 1.0;
    }
    let mut left: std::collections::HashMap<&str, usize> = Default::default();
    for w in after {
        *left.entry(w.as_str()).or_default() += 1;
    }
    let found = before
        .iter()
        .filter(|w| match left.get_mut(w.as_str()) {
            Some(n) if *n > 0 => {
                *n -= 1;
                true
            }
            _ => false,
        })
        .count();
    found as f64 / before.len() as f64
}

impl OfficeConvert {
    /// Stats of a file by reading it with Pandoc. Pandoc gives each sheet of
    /// a workbook a heading with its name, which isn't the sheet's content
    /// (and isn't printed): those headings are left out.
    fn read(&self, ctx: &Ctx, path: &Path, ext: &str) -> Result<Stats, ToolError> {
        let from = pandoc_reader(ext)
            .ok_or_else(|| ToolError::Failed(format!("Can't read {ext} back")))?;
        let name = format!("input.{ext}");
        let mut ast = self.doc.ast(ctx, from, &name, std::fs::read(path)?)?;
        if ext == "xlsx" {
            if let Some(blocks) = ast["blocks"].as_array_mut() {
                blocks.retain(|b| b["t"] != "Header");
            }
        }
        Ok(stats(&ast))
    }

    /// Stats of a file only LibreOffice reads: through its OOXML sibling.
    fn read_via_lo(
        &self,
        ctx: &Ctx,
        soffice: &Path,
        path: &Path,
        ext: &str,
        work: &Path,
    ) -> Result<Stats, ToolError> {
        let sib = readable_sibling(ext)
            .ok_or_else(|| ToolError::Failed(format!("Can't read {ext} back")))?;
        let copy =
            convert(soffice, path, filter(sib), sib, work, ctx.cancel, LIMIT).map_err(lo_error)?;
        self.read(ctx, &copy, sib)
    }
}

impl Tool for OfficeConvert {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            id: "office.convert".into(),
            label: "Convert with LibreOffice".into(),
            category: Category::Convert,
            targets: vec![
                "pdf".into(),
                "docx".into(),
                "xlsx".into(),
                "pptx".into(),
                "odt".into(),
                "ods".into(),
                "odp".into(),
                "rtf".into(),
                "csv".into(),
            ],
            best_effort: false,
            platforms: vec![Platform::Desktop],
            options: vec![],
            combines: false,
            needs: (self.locate)().is_none().then(Self::needs),
        }
    }

    fn info_for(&self, kind: &FileKind) -> ToolInfo {
        let mut i = self.info();
        i.targets = targets_for(*kind).iter().map(|t| t.to_string()).collect();
        i
    }

    fn accepts(&self, kind: &FileKind) -> bool {
        !targets_for(*kind).is_empty()
    }

    fn run(&self, input: &Path, ctx: &Ctx) -> Result<ToolOutput, ToolError> {
        let soffice = (self.locate)().ok_or_else(|| {
            ToolError::Failed(format!(
                "{} — LibreOffice wasn't found on this computer",
                Self::needs().message
            ))
        })?;
        let kind = detect_file(input)?.kind;
        let target = ctx
            .target
            .ok_or_else(|| ToolError::Failed("Choose a format to convert to".into()))?;
        if !targets_for(kind).contains(&target) {
            return Err(ToolError::Failed(format!(
                "LibreOffice can't turn a {} into {target}",
                kind.label()
            )));
        }
        let work = ctx.work_dir.join("libreoffice");
        let mut warnings = Vec::new();
        (ctx.progress)(0.05);

        // 1. What the source holds, and the file LibreOffice converts.
        let (before, lo_input) = if via_pandoc(kind) {
            let before = self.doc.source_stats(ctx, input, kind)?;
            let sub = work.join("pandoc");
            std::fs::create_dir_all(&sub)?;
            let docx = self.doc.run(
                input,
                &Ctx {
                    work_dir: &sub,
                    target: Some("docx"),
                    options: ctx.options,
                    cancel: ctx.cancel,
                    progress: &|_| {},
                },
            )?;
            warnings.extend(docx.report.warnings);
            (before, docx.files[0].0.clone())
        } else if reader_for(kind).is_some() || matches!(kind, FileKind::Xlsx | FileKind::Pptx) {
            (self.read(ctx, input, ext_of(kind))?, input.to_path_buf())
        } else {
            warnings.push(format!(
                "Only LibreOffice can read a {}, so this check compares the result with how LibreOffice reads the original",
                kind.label()
            ));
            let before =
                self.read_via_lo(ctx, &soffice, input, ext_of(kind), &work.join("before"))?;
            (before, input.to_path_buf())
        };
        (ctx.progress)(0.3);

        // 2. Convert.
        let out = convert(
            &soffice,
            &lo_input,
            filter(target),
            target,
            &work.join("run"),
            ctx.cancel,
            LIMIT,
        )
        .map_err(lo_error)?;
        (ctx.progress)(0.75);

        // 3. What survived.
        let mut rep = if target == "pdf" {
            let pages = crate::reader::pdf::engine()
                .map_err(ToolError::Failed)?
                .extract(&out)
                .map_err(ToolError::Failed)?;
            // Words as the PDF reconstruction reads them (word spaces decided,
            // marks that repeat a cluster's text dropped), in stream order.
            let doc = crate::pdfconv::layout::reconstruct(&pages);
            let words = normalise_words(&doc.stream_text);
            let images: u64 = pages.iter().map(|p| p.images.len() as u64).sum();
            let m = |name: &str, b: u64, a: u64| Metric {
                name: name.into(),
                before: b,
                after: a,
                informational: false,
            };
            let mut metrics = vec![m("Words", before.words.len() as u64, words.len() as u64)];
            if before.images > 0 || images > 0 {
                metrics.push(m("Images", before.images, images));
            }
            metrics.push(Metric {
                informational: true,
                ..m("Pages", pages.len() as u64, pages.len() as u64)
            });
            FidelityReport {
                metrics,
                text_similarity: Some(recall(&before.words, &words)),
                warnings: Vec::new(),
            }
        } else if pandoc_reader(target).is_some() {
            let after = self.read(ctx, &out, target)?;
            report(&before, &after, target != "csv")
        } else {
            let after = self.read_via_lo(ctx, &soffice, &out, target, &work.join("after"))?;
            report(&before, &after, true)
        };
        if target == "csv" {
            warnings.push("CSV keeps only the first sheet's values".into());
        }
        rep.warnings.extend(warnings);
        (ctx.progress)(1.0);
        Ok(ToolOutput {
            files: vec![(out, target.to_string())],
            report: rep,
        })
    }
}
