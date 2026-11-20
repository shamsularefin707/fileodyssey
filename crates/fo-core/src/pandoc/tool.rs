//! `doc.convert`: any Pandoc-readable document to any supported format,
//! with a fidelity report built from the source and result ASTs.

use super::ast::{normalise_words, report, stats, strip_note_markers, Stats};
use super::formats::{reader_for, reader_for_target, targets_for, writer_for, TARGETS};
use super::{PandocEngine, PandocRequest, PandocResult};
use crate::reader::text::{decode_text, resolve};
use crate::tool::{Ctx, Tool, ToolError, ToolOutput};
use fo_types::{Category, FileKind, OptionKind, OptionSpec, Platform, ToolInfo};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Largest image pulled in from next to a Markdown/HTML/… source.
const MAX_RESOURCE: u64 = 50 * 1024 * 1024;
const MAX_RESOURCES_TOTAL: u64 = 200 * 1024 * 1024;

pub struct DocConvert {
    engine: Arc<dyn PandocEngine>,
}

impl DocConvert {
    pub fn new(engine: Arc<dyn PandocEngine>) -> Self {
        Self { engine }
    }

    fn pandoc(
        &self,
        ctx: &Ctx,
        options: Value,
        files: Vec<(String, Vec<u8>)>,
    ) -> Result<PandocResult, ToolError> {
        if ctx.cancel.is_cancelled() {
            return Err(ToolError::Cancelled);
        }
        self.engine
            .run(PandocRequest { options, files }, ctx.cancel)
            .map_err(|e| {
                if ctx.cancel.is_cancelled() {
                    ToolError::Cancelled
                } else {
                    ToolError::Failed(clean_error(&e))
                }
            })
    }

    fn ast(&self, ctx: &Ctx, from: &str, name: &str, bytes: Vec<u8>) -> Result<Value, ToolError> {
        let r = self.pandoc(
            ctx,
            // Standalone, so readers promote document titles (e.g. RST) into metadata.
            json!({"from": from, "to": "json", "standalone": true, "input-files": [name]}),
            vec![(name.to_owned(), bytes)],
        )?;
        serde_json::from_slice(&r.stdout)
            .map_err(|_| ToolError::Failed("Pandoc returned an unreadable document".into()))
    }
}

fn clean_error(e: &str) -> String {
    let e = e.trim();
    if e.is_empty() {
        "Pandoc couldn't convert this file".into()
    } else {
        format!(
            "Pandoc couldn't convert this file: {}",
            e.lines().take(6).collect::<Vec<_>>().join(" ")
        )
    }
}

fn ext_of(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Markdown => "md",
        FileKind::Html => "html",
        FileKind::Docx => "docx",
        FileKind::Odt => "odt",
        FileKind::Rtf => "rtf",
        FileKind::Epub => "epub",
        FileKind::Latex => "tex",
        FileKind::Rst => "rst",
        FileKind::Org => "org",
        FileKind::AsciiDoc => "adoc",
        _ => "json",
    }
}

fn is_text_source(kind: FileKind) -> bool {
    matches!(
        kind,
        FileKind::Markdown
            | FileKind::Html
            | FileKind::Latex
            | FileKind::Rst
            | FileKind::Org
            | FileKind::AsciiDoc
            | FileKind::Text
    )
}

/// Plain text as a Pandoc document: blank lines separate paragraphs, and
/// nothing in the text is read as markup.
pub fn text_ast(text: &str, api: &Value) -> Value {
    let blocks: Vec<Value> = text
        .replace("\r\n", "\n")
        .split("\n\n")
        .map(|p| p.trim_matches('\n'))
        .filter(|p| !p.trim().is_empty())
        .map(|para| {
            let mut inl = Vec::new();
            for (i, line) in para.lines().enumerate() {
                if i > 0 {
                    inl.push(json!({"t": "LineBreak"}));
                }
                for (j, w) in line.split_whitespace().enumerate() {
                    if j > 0 {
                        inl.push(json!({"t": "Space"}));
                    }
                    inl.push(json!({"t": "Str", "c": w}));
                }
            }
            json!({"t": "Para", "c": inl})
        })
        .collect();
    json!({"pandoc-api-version": api, "meta": {}, "blocks": blocks})
}

/// Relative image paths used by the document.
fn image_targets(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Array(a) => a.iter().for_each(|x| image_targets(x, out)),
        Value::Object(o) => {
            if o.get("t").and_then(Value::as_str) == Some("Image") {
                if let Some(t) = o
                    .get("c")
                    .and_then(|c| c.get(2))
                    .and_then(|t| t.get(0))
                    .and_then(Value::as_str)
                {
                    out.push(t.to_owned());
                }
            }
            o.values().for_each(|x| image_targets(x, out));
        }
        _ => {}
    }
}

/// Targets whose Pandoc reader (used only to check the result) is known to
/// miss content Pandoc's writer did produce.
fn weak_checker(target: &str) -> Option<&'static str> {
    match target {
        "rtf" => Some("RTF"),
        "tex" => Some("LaTeX"),
        "adoc" => Some("AsciiDoc"),
        "odt" => Some("OpenDocument"),
        "org" => Some("Org"),
        "rst" => Some("reStructuredText"),
        _ => None,
    }
}

fn title_of(doc: &Value) -> bool {
    doc.get("meta").and_then(|m| m.get("title")).is_some()
}

impl Tool for DocConvert {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            id: "doc.convert".into(),
            label: "Convert document".into(),
            category: Category::Convert,
            targets: TARGETS.iter().map(|t| t.to_string()).collect(),
            best_effort: false,
            platforms: vec![Platform::Desktop, Platform::Android],
            options: vec![OptionSpec {
                key: "flavour".into(),
                label: "Markdown style".into(),
                kind: OptionKind::Choice {
                    choices: vec![
                        ("markdown".into(), "Pandoc (keeps the most)".into()),
                        ("gfm".into(), "GitHub".into()),
                        ("commonmark".into(), "CommonMark".into()),
                    ],
                    default: "markdown".into(),
                },
                targets: vec!["md".into()],
                show_if: None,
            }],
            combines: false,
        }
    }

    fn info_for(&self, kind: &FileKind) -> ToolInfo {
        let mut i = self.info();
        i.targets = targets_for(*kind).into_iter().map(str::to_owned).collect();
        i
    }

    fn accepts(&self, kind: &FileKind) -> bool {
        reader_for(*kind).is_some()
    }

    fn run(&self, input: &Path, ctx: &Ctx) -> Result<ToolOutput, ToolError> {
        let kind = crate::detect::detect_file(input)?.kind;
        let from = reader_for(kind)
            .ok_or_else(|| ToolError::Failed("This kind of file can't be converted".into()))?;
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

        let raw = std::fs::read(input)?;
        let bytes = if is_text_source(kind) {
            decode_text(&raw).into_bytes()
        } else {
            raw
        };
        let in_name = format!("input.{}", ext_of(kind));
        (ctx.progress)(0.05);

        // 1. What the source contains.
        let before_ast = if kind == FileKind::Text {
            let api = self.ast(ctx, "markdown", "probe.md", Vec::new())?;
            text_ast(&String::from_utf8_lossy(&bytes), &api["pandoc-api-version"])
        } else {
            self.ast(ctx, from, &in_name, bytes.clone())?
        };
        let before = stats(&before_ast);
        (ctx.progress)(0.25);

        // 2. Convert, with images that sit next to text sources.
        let mut files = vec![(
            in_name.clone(),
            if kind == FileKind::Text {
                serde_json::to_vec(&before_ast).unwrap_or_default()
            } else {
                bytes
            },
        )];
        if is_text_source(kind) {
            let dir = input.parent().unwrap_or(Path::new("."));
            let mut wanted = Vec::new();
            image_targets(&before_ast, &mut wanted);
            let mut total = 0u64;
            for t in wanted {
                let Some(rel) = resolve("", &t) else { continue };
                if files.iter().any(|(n, _)| *n == rel) {
                    continue;
                }
                let p = dir.join(&rel);
                let Ok(meta) = std::fs::metadata(&p) else {
                    continue;
                };
                if !meta.is_file()
                    || meta.len() > MAX_RESOURCE
                    || total + meta.len() > MAX_RESOURCES_TOTAL
                {
                    continue;
                }
                if let Ok(b) = std::fs::read(&p) {
                    total += meta.len();
                    files.push((rel, b));
                }
            }
        }
        let out_name = format!("out.{target}");
        let mut opts =
            json!({"from": from, "to": to, "input-files": [in_name], "output-file": out_name});
        match target {
            "html" => {
                opts["standalone"] = json!(true);
                opts["embed-resources"] = json!(true);
                if !title_of(&before_ast) {
                    opts["metadata"] = json!({"pagetitle": stem});
                }
            }
            // Standalone keeps the title block (and RTF/LaTeX need it to be valid).
            "rtf" | "tex" | "md" | "rst" | "org" | "adoc" => opts["standalone"] = json!(true),
            "epub" if !title_of(&before_ast) => opts["metadata"] = json!({"title": stem}),
            _ => {}
        }
        if matches!(target, "md" | "rst" | "org" | "adoc" | "tex") {
            opts["extract-media"] = json!("media");
        }
        let converted = self.pandoc(ctx, opts, files)?;
        let out_bytes = converted
            .file(&out_name)
            .ok_or_else(|| ToolError::Failed("Pandoc didn't produce a file".into()))?
            .to_vec();
        let media: Vec<(String, Vec<u8>)> = converted
            .files
            .iter()
            .filter(|(n, _)| n.starts_with("media/"))
            .cloned()
            .collect();
        (ctx.progress)(0.7);

        // 3. What survived.
        let (after, structure) = match reader_for_target(target) {
            Some(_) => {
                let back = if target == "md" {
                    flavour
                } else {
                    reader_for_target(target).unwrap_or("markdown")
                };
                (
                    stats(&self.ast(ctx, back, &out_name, out_bytes.clone())?),
                    true,
                )
            }
            None => (
                Stats {
                    words: normalise_words(&strip_note_markers(&String::from_utf8_lossy(
                        &out_bytes,
                    ))),
                    ..Stats::default()
                },
                false,
            ),
        };
        let mut rep = report(&before, &after, structure);
        if rep.verdict() == fo_types::Verdict::Warn {
            if let Some(f) = weak_checker(target) {
                rep.warnings.push(format!(
                    "Checked by reading the result back with Pandoc's {f} reader, which can miss titles, captions, tables or notes; the file itself may be complete"
                ));
            }
        }
        let mut seen = std::collections::HashSet::new();
        for w in converted.warnings {
            if rep.warnings.len() < 10 && seen.insert(w.clone()) {
                rep.warnings.push(w);
            }
        }

        // 4. One file, or a folder when images came out alongside it.
        let file_name = format!("{stem}.{target}");
        let files: Vec<(PathBuf, String)> = if media.is_empty() {
            let p = ctx.work_dir.join(&out_name);
            std::fs::write(&p, &out_bytes)?;
            vec![(p, target.to_owned())]
        } else {
            let dir = ctx.work_dir.join("bundle");
            std::fs::create_dir_all(dir.join("media"))?;
            std::fs::write(dir.join(&file_name), &out_bytes)?;
            for (n, b) in media {
                let Some(rel) = resolve("", &n) else { continue };
                let p = dir.join(rel);
                if let Some(parent) = p.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(p, b)?;
            }
            vec![(dir, String::new())]
        };
        (ctx.progress)(1.0);
        Ok(ToolOutput { files, report: rep })
    }
}
