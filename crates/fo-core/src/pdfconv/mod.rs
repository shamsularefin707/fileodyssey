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
}
pub mod ast;
pub mod layout;
pub mod tool;
