//! `pdf.convert`: rebuild a digital PDF as an editable document.

use super::ast::{image_name, to_ast};
use super::docx::{write_docx, PageSetup, Picture};
use super::layout::{reconstruct, Block, BlockKind, Doc};
use crate::pandoc::ast::{normalise_words, Stats};
use crate::pandoc::formats::writer_for;
use crate::pandoc::tool::{DocConvert, Source, Written};
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

/// The fidelity check reads footnotes after the text (where Pandoc puts
/// them): move each note's words from the page bottom to the end of the
/// PDF's word sequence too, so placing notes isn't counted as reordering.
fn move_notes(doc: &Doc, words: &mut Vec<String>) {
    let mut moved = Vec::new();
    for note in &doc.notes {
        let text: String = note.iter().map(|l| l.text() + " ").collect();
        let nw = normalise_words(&text);
        if nw.is_empty() || nw.len() > words.len() {
            continue;
        }
        if let Some(from) = (0..=words.len() - nw.len())
            .rev()
            .find(|&i| words[i..i + nw.len()] == nw[..])
        {
            moved.extend(words.drain(from..from + nw.len()));
        }
    }
    words.extend(moved);
}

/// Report line for running headers/footers left out of a target that has
/// no page header (only DOCX keeps them).
fn running_text_note(doc: &Doc) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for t in [&doc.header, &doc.footer].into_iter().flatten() {
        parts.push(format!("“{t}”"));
    }
    // Page numbers alone aren't content worth a warning.
    if parts.is_empty() {
        return None;
    }
    if doc.page_numbers {
        parts.push("page numbers".into());
    }
    let list = match parts.len() {
        1 => parts[0].clone(),
        n => format!("{} and {}", parts[..n - 1].join(", "), parts[n - 1]),
    };
    Some(format!(
        "The running header/footer text {list} repeats on every page, so it was left out; Word (DOCX) keeps it as the page header and footer"
    ))
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
        for p in &pages {
            for g in &p.glyphs {
                text.push_str(&g.text);
                if g.generated {
                    text.push(' ');
                }
            }
            text.push('\n');
        }
        let has_text = pages
            .iter()
            .flat_map(|p| &p.glyphs)
            .any(|g| !g.generated && !g.text.trim().is_empty());
        let mut warnings = Vec::new();
        let (doc, pictures): (Doc, Vec<Picture>) = if has_text {
            let pics = pages
                .iter()
                .flat_map(|p| {
                    p.images.iter().map(|i| Picture {
                        png: i.png.clone(),
                        width: i.x1 - i.x0,
                        height: i.y1 - i.y0,
                    })
                })
                .collect();
            (reconstruct(&pages), pics)
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
                let pg = &pages[i as usize];
                pics.push(Picture {
                    png: pdf.render(id, i, SCAN_WIDTH).map_err(failed)?,
                    width: pg.width,
                    height: pg.height,
                });
            }
            pdf.close(id);
            let blocks = (0..pics.len())
                .map(|i| Block {
                    kind: BlockKind::Image(i),
                    lines: vec![],
                })
                .collect();
            (
                Doc {
                    blocks,
                    ..Doc::default()
                },
                pics,
            )
        };
        let tables: Vec<_> = doc
            .blocks
            .iter()
            .filter_map(|b| match &b.kind {
                BlockKind::Table(t) => Some(t),
                _ => None,
            })
            .collect();
        // Words as extraction spaced them (some producers draw no spaces),
        // without the running header/footer, which is kept apart.
        let mut words = normalise_words(if has_text { &doc.stream_text } else { &text });
        // Each note's label appears twice: at its marker and before the note.
        let labels = doc
            .blocks
            .iter()
            .flat_map(|b| &b.lines)
            .flat_map(|l| &l.spans)
            .filter(|s| s.note.is_some())
            .flat_map(|s| normalise_words(&s.text));
        for w in labels.flat_map(|w| [w.clone(), w]) {
            if let Some(i) = words.iter().position(|x| *x == w) {
                words.remove(i);
            }
        }
        move_notes(&doc, &mut words);
        let before = Stats {
            words,
            images: pictures.len() as u64,
            links: pages.iter().map(|p| p.links.len() as u64).sum(),
            tables: tables.len() as u64,
            cells: tables
                .iter()
                .map(|t| t.rows.iter().map(|r| r.len() as u64).sum::<u64>())
                .sum(),
            footnotes: doc.notes.len() as u64,
            ..Stats::default()
        };
        if target != "docx" {
            if let Some(w) = running_text_note(&doc) {
                warnings.push(w);
            }
        }
        (ctx.progress)(0.45);

        let mut out = if target == "docx" {
            // Written directly so fonts, sizes and colours are kept.
            let setup = PageSetup {
                width: pages.first().map_or(595.0, |p| p.width),
                height: pages.first().map_or(842.0, |p| p.height),
            };
            let bytes = write_docx(&doc, &pictures, &setup).map_err(failed)?;
            self.doc.check_output(
                ctx,
                Written {
                    target,
                    flavour,
                    out_name: "out.docx",
                    out_bytes: bytes,
                    media: Vec::new(),
                    stem: &stem,
                },
                before,
                false,
                Vec::new(),
            )?
        } else {
            let api = self.doc.ast(ctx, "markdown", "probe.md", Vec::new())?;
            let ast = to_ast(&doc, &api["pandoc-api-version"]);
            let mut files = vec![(
                "input.json".to_string(),
                serde_json::to_vec(&ast).unwrap_or_default(),
            )];
            files.extend(
                pictures
                    .into_iter()
                    .enumerate()
                    .map(|(i, p)| (image_name(i), p.png)),
            );
            self.doc.write_and_check(
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
            )?
        };
        warnings.append(&mut out.report.warnings);
        out.report.warnings = warnings;
        Ok(out)
    }
}
