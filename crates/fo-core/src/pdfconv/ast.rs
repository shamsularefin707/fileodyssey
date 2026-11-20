//! Reconstructed blocks → Pandoc JSON AST.

use super::layout::{strip_hyphen, BlockKind, Cell, Doc, Line, Script, Span, Table};
use serde_json::{json, Value};

/// File name the AST uses for image `i` (0-based, document-wide).
pub fn image_name(i: usize) -> String {
    format!("media/image-{}.png", i + 1)
}

fn words(text: &str, out: &mut Vec<Value>) {
    let mut first = true;
    for w in text.split_whitespace() {
        if !first {
            out.push(json!({"t": "Space"}));
        }
        out.push(json!({"t": "Str", "c": w}));
        first = false;
    }
}

fn push_space(out: &mut Vec<Value>) {
    if !out.is_empty() && out.last().and_then(|v| v["t"].as_str()) != Some("Space") {
        out.push(json!({"t": "Space"}));
    }
}

/// One styled run; whitespace at its edges goes outside the styling.
fn styled(s: &Span, out: &mut Vec<Value>) {
    if s.text.starts_with(char::is_whitespace) {
        push_space(out);
    }
    let core = s.text.trim();
    if !core.is_empty() {
        let inner = if s.mono {
            vec![json!({"t": "Code", "c": [["", [], []], core]})]
        } else {
            let mut v = Vec::new();
            words(core, &mut v);
            v
        };
        let inner = if s.italic && !s.mono {
            vec![json!({"t": "Emph", "c": inner})]
        } else {
            inner
        };
        let inner = if s.bold && !s.mono {
            vec![json!({"t": "Strong", "c": inner})]
        } else {
            inner
        };
        let inner = match s.script {
            Script::Super => vec![json!({"t": "Superscript", "c": inner})],
            Script::Sub => vec![json!({"t": "Subscript", "c": inner})],
            Script::Normal => inner,
        };
        out.extend(inner);
    }
    if s.text.ends_with(char::is_whitespace) && !core.is_empty() {
        push_space(out);
    }
}

/// Inline content of a block: lines joined, hyphenated words rejoined,
/// runs of one link grouped into a single Link.
pub fn inlines(lines: &[Line]) -> Vec<Value> {
    inlines_in(lines, &[])
}

/// As `inlines`; spans marking a footnote become a Note holding it.
fn inlines_in(lines: &[Line], notes: &[Vec<Line>]) -> Vec<Value> {
    let spans = joined_spans(lines);
    let mut out = Vec::new();
    let mut i = 0;
    while i < spans.len() {
        match spans[i].link.clone() {
            Some(url) => {
                let mut inner = Vec::new();
                let mut trailing = false;
                while i < spans.len() && spans[i].link.as_deref() == Some(url.as_str()) {
                    styled(&spans[i], &mut inner);
                    i += 1;
                }
                if inner.first().and_then(|v| v["t"].as_str()) == Some("Space") {
                    inner.remove(0);
                    push_space(&mut out);
                }
                if inner.last().and_then(|v| v["t"].as_str()) == Some("Space") {
                    inner.pop();
                    trailing = true;
                }
                out.push(json!({"t": "Link", "c": [["", [], []], inner, [url, ""]]}));
                if trailing {
                    push_space(&mut out);
                }
            }
            None => {
                match spans[i].note.and_then(|n| notes.get(n)) {
                    Some(note) => {
                        // The marker sits right after its word.
                        if out.last().and_then(|v| v["t"].as_str()) == Some("Space") {
                            out.pop();
                        }
                        let body = inlines_in(note, &[]);
                        out.push(json!({"t": "Note", "c": [{"t": "Para", "c": body}]}));
                        if spans[i].text.ends_with(char::is_whitespace) {
                            push_space(&mut out);
                        }
                    }
                    None => styled(&spans[i], &mut out),
                }
                i += 1;
            }
        }
    }
    while out.last().and_then(|v| v["t"].as_str()) == Some("Space") {
        out.pop();
    }
    out
}

/// A block's spans with its lines joined: a space between lines, and
/// hyphenated words rejoined.
pub fn joined_spans(lines: &[Line]) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let mut ls = l.spans.clone();
        if l.hyphen_end {
            if let Some(last) = ls.last_mut() {
                last.text = strip_hyphen(&last.text).to_owned();
            }
        } else if i + 1 < lines.len() {
            if let Some(last) = ls.last_mut() {
                last.text.push(' ');
            }
        }
        spans.extend(ls);
    }
    spans
}

/// A Pandoc table: header rows in the head, the rest in one body; cells
/// keep their row and column spans.
pub fn table(t: &Table) -> Value {
    let attr = json!(["", [], []]);
    let row = |r: &Vec<Cell>| {
        let cells: Vec<Value> = r
            .iter()
            .map(|c| {
                let inl = inlines(&c.lines);
                let blocks = if inl.is_empty() {
                    json!([])
                } else {
                    json!([{"t": "Plain", "c": inl}])
                };
                json!([attr, {"t": "AlignDefault"}, c.rowspan, c.colspan, blocks])
            })
            .collect();
        json!([attr, cells])
    };
    // Header cells are bold by look, like headings.
    let plain: Vec<Vec<Cell>> = t.rows[..t.header_rows]
        .iter()
        .map(|r| {
            r.iter()
                .cloned()
                .map(|mut c| {
                    for s in c.lines.iter_mut().flat_map(|l| l.spans.iter_mut()) {
                        s.bold = false;
                    }
                    c
                })
                .collect()
        })
        .collect();
    let head: Vec<Value> = plain.iter().map(row).collect();
    let body: Vec<Value> = t.rows[t.header_rows..].iter().map(row).collect();
    let specs: Vec<Value> = (0..t.cols)
        .map(|_| json!([{"t": "AlignDefault"}, {"t": "ColWidthDefault"}]))
        .collect();
    json!({"t": "Table", "c": [
        attr,
        [null, []],
        specs,
        [attr, head],
        [[attr, 0, [], body]],
        [attr, []]
    ]})
}

/// Nested lists from consecutive list items, by indent level.
fn list(items: &[(bool, u8, Vec<Value>)], level: u8, i: &mut usize) -> Value {
    let ordered = items[*i].0;
    let mut entries: Vec<Vec<Value>> = Vec::new();
    while *i < items.len() && items[*i].1 >= level {
        if items[*i].1 == level || entries.is_empty() {
            entries.push(vec![json!({"t": "Plain", "c": items[*i].2.clone()})]);
            *i += 1;
        } else {
            let nested = list(items, items[*i].1, i);
            entries.last_mut().expect("non-empty").push(nested);
        }
    }
    if ordered {
        json!({"t": "OrderedList", "c": [[1, {"t": "Decimal"}, {"t": "Period"}], entries]})
    } else {
        json!({"t": "BulletList", "c": entries})
    }
}

pub fn to_ast(doc: &Doc, api: &Value) -> Value {
    let blocks = &doc.blocks;
    let notes = &doc.notes;
    let mut out = Vec::new();
    let mut k = 0;
    while k < blocks.len() {
        let b = &blocks[k];
        match &b.kind {
            BlockKind::Heading(n) => {
                // Headings are usually set in bold; that is the heading's
                // look, not emphasis inside it.
                let mut lines = b.lines.clone();
                if lines.iter().flat_map(|l| &l.spans).filter(|s| !s.text.trim().is_empty()).all(|s| s.bold) {
                    for s in lines.iter_mut().flat_map(|l| l.spans.iter_mut()) {
                        s.bold = false;
                    }
                }
                out.push(json!({"t": "Header", "c": [n, ["", [], []], inlines_in(&lines, notes)]}));
            }
            BlockKind::Para => {
                let inl = inlines_in(&b.lines, notes);
                if !inl.is_empty() {
                    out.push(json!({"t": "Para", "c": inl}));
                }
            }
            BlockKind::Table(t) => out.push(table(t)),
            BlockKind::Image(i) => out.push(json!({"t": "Para", "c": [{"t": "Image", "c": [["", [], []], [], [image_name(*i), ""]]}]})),
            BlockKind::ListItem { .. } => {
                let mut items = Vec::new();
                while let Some(BlockKind::ListItem { ordered, level }) = blocks.get(k).map(|b| &b.kind) {
                    items.push((*ordered, *level, inlines_in(&blocks[k].lines, notes)));
                    k += 1;
                }
                let min = items.iter().map(|x| x.1).min().unwrap_or(0);
                let mut i = 0;
                while i < items.len() {
                    out.push(list(&items, min, &mut i));
                }
                continue;
            }
        }
        k += 1;
    }
    json!({"pandoc-api-version": api, "meta": {}, "blocks": out})
}
