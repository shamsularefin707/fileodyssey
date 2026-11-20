//! `pdf.convert`: rebuild a digital PDF as an editable document.

use super::ast::{image_name, to_ast};
use super::layout::{layout, Block, BlockKind};
use crate::pandoc::ast::{normalise_words, Stats};
use crate::pandoc::formats::writer_for;
use crate::pandoc::tool::{DocConvert, Source};
use crate::pandoc::PandocEngine;
use crate::reader::pdf::engine;
use crate::tool::{Ctx, Tool, ToolError, ToolOutput};
use fo_types::{FileKind, ToolInfo};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;

/// Width of page pictures kept for PDFs with no text layer.
const SCAN_WIDTH: u32 = 1600;

pub struct PdfConvert {
    doc: DocConvert,
}

impl PdfConvert {
    pub fn new(engine: Arc<dyn PandocEngine>) -> Self {
        Self {
            doc: DocConvert::new(engine),
        }
    }
}

fn failed(e: String) -> ToolError {
    ToolError::Failed(e)
}

impl Tool for PdfConvert {
    fn info(&self) -> ToolInfo {
        let base = self.doc.info();
        ToolInfo {
            id: "pdf.convert".into(),
            label: "Convert PDF".into(),
            targets: ["docx", "odt", "md", "html", "epub", "txt"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            best_effort: true,
            ..base
        }
    }

    fn accepts(&self, kind: &FileKind) -> bool {
        *kind == FileKind::Pdf
    }

    fn run(&self, input: &Path, ctx: &Ctx) -> Result<ToolOutput, ToolError> {
        let target = ctx
            .target
            .ok_or_else(|| ToolError::Failed("Choose a format to convert to".into()))?;
        let flavour = ctx
            .options
            .get("flavour")
            .and_then(Value::as_str)
            .unwrap_or("markdown");
        let flavour = if ["markdown", "gfm", "commonmark"].contains(&flavour) {
            flavour
        } else {
            "markdown"
        };
        let to = if target == "md" {
            flavour
        } else {
            writer_for(target)
                .ok_or_else(|| ToolError::Failed(format!("Can't convert to {target}")))?
        };
        let stem = input
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "document".into());
        let pdf = engine().map_err(failed)?;
        let pages = pdf.extract(input).map_err(failed)?;
        if ctx.cancel.is_cancelled() {
            return Err(ToolError::Cancelled);
        }
        (ctx.progress)(0.3);

        // What the PDF itself holds: its text in stream order, images, links.
        let mut text = String::new();
        for g in pages.iter().flat_map(|p| &p.glyphs) {
            text.push_str(&g.text);
            if g.generated {
                text.push(' ');
            }
        }
        let has_text = pages
            .iter()
            .flat_map(|p| &p.glyphs)
            .any(|g| !g.generated && !g.text.trim().is_empty());
        let mut warnings = Vec::new();
        let (blocks, pictures): (Vec<Block>, Vec<Vec<u8>>) = if has_text {
            let pics = pages
                .iter()
                .flat_map(|p| p.images.iter().map(|i| i.png.clone()))
                .collect();
            (layout(&pages), pics)
        } else {
            // A scan: keep each page as a picture.
            warnings.push("This PDF has no text layer (it's scanned or made of pictures), so the pages are kept as images. Reading text from images (OCR) isn't supported.".to_string());
            let (id, info) = pdf.open(input).map_err(failed)?;
            let mut pics = Vec::new();
            for i in 0..info.pages.len() as u32 {
                if ctx.cancel.is_cancelled() {
                    pdf.close(id);
                    return Err(ToolError::Cancelled);
                }
                pics.push(pdf.render(id, i, SCAN_WIDTH).map_err(failed)?);
            }
            pdf.close(id);
            let blocks = (0..pics.len())
                .map(|i| Block {
                    kind: BlockKind::Image(i),
                    lines: vec![],
                })
                .collect();
            (blocks, pics)
        };
        let before = Stats {
            words: normalise_words(&text),
            images: pictures.len() as u64,
            links: pages.iter().map(|p| p.links.len() as u64).sum(),
            ..Stats::default()
        };
        (ctx.progress)(0.45);

        let api = self.doc.ast(ctx, "markdown", "probe.md", Vec::new())?;
        let ast = to_ast(&blocks, &api["pandoc-api-version"]);
        let mut files = vec![(
            "input.json".to_string(),
            serde_json::to_vec(&ast).unwrap_or_default(),
        )];
        files.extend(
            pictures
                .into_iter()
                .enumerate()
                .map(|(i, b)| (image_name(i), b)),
        );
        let mut out = self.doc.write_and_check(
            ctx,
            Source {
                from: "json",
                in_name: "input.json",
                files,
                before,
                has_title: false,
                stem: &stem,
                known_structure: false,
            },
            target,
            flavour,
            to,
        )?;
        warnings.append(&mut out.report.warnings);
        out.report.warnings = warnings;
        Ok(out)
    }
}
