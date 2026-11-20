//! What a document contains, measured on Pandoc's JSON AST, and the
//! fidelity report comparing a source with its conversion.

use fo_types::{FidelityReport, Metric};
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stats {
    pub words: Vec<String>,
    pub headings: u64,
    pub paragraphs: u64,
    pub tables: u64,
    pub cells: u64,
    pub images: u64,
    pub links: u64,
    pub footnotes: u64,
}

const BLOCKS: [&str; 14] = [
    "Para",
    "Plain",
    "Header",
    "CodeBlock",
    "BlockQuote",
    "OrderedList",
    "BulletList",
    "DefinitionList",
    "Div",
    "Table",
    "Figure",
    "LineBlock",
    "HorizontalRule",
    "RawBlock",
];

fn rows_cells(rows: &Value) -> u64 {
    rows.as_array()
        .map(|rs| {
            rs.iter()
                .map(|r| {
                    r.get(1)
                        .and_then(Value::as_array)
                        .map_or(0, |c| c.len() as u64)
                })
                .sum()
        })
        .unwrap_or(0)
}

/// Cells in a Pandoc ≥ 2.10 table: `[attr, caption, colspecs, head, bodies, foot]`.
fn table_cells(c: &Value) -> u64 {
    let head = c.get(3).and_then(|h| h.get(1)).map_or(0, rows_cells);
    let bodies: u64 = c
        .get(4)
        .and_then(Value::as_array)
        .map(|bs| {
            bs.iter()
                .map(|b| b.get(2).map_or(0, rows_cells) + b.get(3).map_or(0, rows_cells))
                .sum()
        })
        .unwrap_or(0);
    let foot = c.get(5).and_then(|f| f.get(1)).map_or(0, rows_cells);
    head + bodies + foot
}

struct Walker {
    s: Stats,
    text: String,
}

impl Walker {
    fn walk(&mut self, v: &Value) {
        match v {
            Value::Array(items) => items.iter().for_each(|i| self.walk(i)),
            Value::Object(o) => {
                let Some(t) = o.get("t").and_then(Value::as_str) else {
                    o.values().for_each(|i| self.walk(i));
                    return;
                };
                let c = o.get("c").unwrap_or(&Value::Null);
                let block = BLOCKS.contains(&t);
                if block {
                    self.text.push(' ');
                }
                match t {
                    "Str" => self.text.push_str(c.as_str().unwrap_or("")),
                    "Space" | "SoftBreak" | "LineBreak" => self.text.push(' '),
                    "Code" | "Math" => {
                        self.text
                            .push_str(c.get(1).and_then(Value::as_str).unwrap_or(""));
                    }
                    "CodeBlock" => self
                        .text
                        .push_str(c.get(1).and_then(Value::as_str).unwrap_or("")),
                    "RawInline" | "RawBlock" => {}
                    "Header" => {
                        self.s.headings += 1;
                        self.walk(c.get(2).unwrap_or(&Value::Null));
                    }
                    "Para" | "Plain" => {
                        self.s.paragraphs += 1;
                        self.walk(c);
                    }
                    "Table" => {
                        self.s.tables += 1;
                        self.s.cells += table_cells(c);
                        self.walk(c);
                    }
                    "Image" => {
                        self.s.images += 1;
                        self.walk(c.get(1).unwrap_or(&Value::Null));
                    }
                    "Link" => {
                        self.s.links += 1;
                        self.walk(c.get(1).unwrap_or(&Value::Null));
                    }
                    "Note" => {
                        self.s.footnotes += 1;
                        self.text.push(' ');
                        self.walk(c);
                        self.text.push(' ');
                    }
                    _ => self.walk(c),
                }
                if block {
                    self.text.push(' ');
                }
            }
            _ => {}
        }
    }
}

/// Counts and words of a Pandoc JSON document (metadata is ignored).
pub fn stats(doc: &Value) -> Stats {
    let mut w = Walker {
        s: Stats::default(),
        text: String::new(),
    };
    w.walk(doc.get("blocks").unwrap_or(doc));
    w.s.words = normalise_words(&w.text);
    w.s
}

/// Words for comparison: Unicode NFKC, typographic quotes/dashes unified,
/// punctuation at word edges dropped. Case is kept.
pub fn normalise_words(text: &str) -> Vec<String> {
    let unified: String = text
        .nfkc()
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' | '\u{02BC}' => '\'',
            '\u{2010}'..='\u{2015}' | '\u{2212}' => '-',
            c => c,
        })
        .collect();
    unified
        .split_whitespace()
        .map(|w| {
            let mut w = w
                .trim_matches(|c: char| c.is_ascii_punctuation() || is_edge_punct(c))
                .to_owned();
            while w.contains("--") {
                w = w.replace("--", "-");
            }
            w
        })
        .filter(|w| !w.is_empty())
        .collect()
}

fn is_edge_punct(c: char) -> bool {
    matches!(
        c,
        '“' | '”'
            | '„'
            | '«'
            | '»'
            | '‹'
            | '›'
            | '…'
            | '।'
            | '॥'
            | '¡'
            | '¿'
            | '、'
            | '。'
            | '，'
    ) || c.is_whitespace()
}

/// Share of words kept in order (0–1), via a word-level diff.
pub fn similarity(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let a: Vec<&str> = a.iter().map(String::as_str).collect();
    let b: Vec<&str> = b.iter().map(String::as_str).collect();
    similar::TextDiff::configure()
        .algorithm(similar::Algorithm::Myers)
        .timeout(std::time::Duration::from_secs(10))
        .diff_slices(&a, &b)
        .ratio() as f64
}

/// Fidelity report. With `compare_structure` false (plain-text output, which
/// Pandoc can't read back) only words are compared.
pub fn report(before: &Stats, after: &Stats, compare_structure: bool) -> FidelityReport {
    let m = |name: &str, b: u64, a: u64| Metric {
        name: name.into(),
        before: b,
        after: a,
        informational: false,
    };
    let mut metrics = vec![m(
        "Words",
        before.words.len() as u64,
        after.words.len() as u64,
    )];
    let mut warnings = Vec::new();
    if compare_structure {
        metrics.extend([
            m("Headings", before.headings, after.headings),
            m("Paragraphs", before.paragraphs, after.paragraphs),
            m("Tables", before.tables, after.tables),
            m("Table cells", before.cells, after.cells),
            m("Images", before.images, after.images),
            m("Links", before.links, after.links),
            m("Footnotes", before.footnotes, after.footnotes),
        ]);
        // Only list what the source has, so the report stays short.
        metrics.retain(|x| x.name == "Words" || x.before > 0 || x.after > 0);
    } else if before.headings + before.tables + before.images + before.links + before.footnotes > 0
    {
        warnings
            .push("Plain text keeps the words but not headings, tables, images or links".into());
    }
    FidelityReport {
        metrics,
        text_similarity: Some(similarity(&before.words, &after.words)),
        warnings,
    }
}
