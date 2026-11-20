//! PDF → structured document ("reconstruction").
//!
//! `reader::pdf` extracts each page's characters, images and links
//! (`PageContent`); `layout` works out reading order and structure; `ast`
//! turns that into a Pandoc document; `tool` is the `pdf.convert` tool.

/// One extracted character. Coordinates are PDF points with the origin at
/// the top-left of the page; `text` is usually one char but can be a
/// ligature or a cluster.
#[derive(Debug, Clone, PartialEq)]
pub struct Glyph {
    pub text: String,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub mono: bool,
    /// Inserted by PDFium (word spaces, line breaks), not drawn.
    pub generated: bool,
    /// A hyphen PDFium recognised as breaking a word across lines.
    pub hyphen: bool,
    /// Font family ("Times New Roman"), "" when unknown.
    pub font: String,
    /// Fill colour, RGB.
    pub color: [u8; 3],
}

/// An axis-aligned line drawn on the page (a table border, a footnote
/// separator), as a thin box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rule {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Rule {
    pub fn horizontal(&self) -> bool {
        self.x1 - self.x0 > self.y1 - self.y0
    }
}

/// Family name from a PDF font name: "ABCDEF+TimesNewRomanPS-BoldMT" →
/// "Times New Roman".
pub fn family_of(name: &str) -> String {
    let n = match name.split_once('+') {
        Some((tag, rest)) if tag.len() == 6 && tag.chars().all(|c| c.is_ascii_uppercase()) => rest,
        _ => name,
    };
    let n = n.split([',', '-']).next().unwrap_or(n);
    let mut n = n.to_owned();
    for suffix in ["PSMT", "PS", "MT"] {
        if n.len() > suffix.len() + 2 && n.ends_with(suffix) {
            n.truncate(n.len() - suffix.len());
            break;
        }
    }
    if n.contains(' ') {
        return n.trim().to_owned();
    }
    // CamelCase → words, except families that are written joined.
    if ["FreeSerif", "FreeSans", "FreeMono"].contains(&n.as_str()) {
        return n;
    }
    let chars: Vec<char> = n.chars().collect();
    let mut out = String::new();
    for (i, c) in chars.iter().enumerate() {
        let in_dejavu = out.ends_with("Deja") && *c == 'V';
        if i > 0 && c.is_uppercase() && chars[i - 1].is_lowercase() && !in_dejavu {
            out.push(' ');
        }
        out.push(*c);
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageImage {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub png: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageLink {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub uri: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PageContent {
    pub width: f32,
    pub height: f32,
    pub glyphs: Vec<Glyph>,
    pub images: Vec<PageImage>,
    pub links: Vec<PageLink>,
    /// Boxes `[x0, y0, x1, y1]` of drawing operations that draw only spaces.
    /// Some producers (Chrome with complex scripts) draw each word gap this
    /// way; when present they are the reliable word boundaries.
    pub spaces: Vec<[f32; 4]>,
    pub rules: Vec<Rule>,
}
pub mod ast;
pub mod layout;
pub mod notes;
pub mod table;
pub mod tool;
