//! Types for the read-only document viewer.
//!
//! Positions are fractions of the page (0.0–1.0) with the origin at the
//! top-left, so the UI can place them at any zoom level.

use serde::{Deserialize, Serialize};

/// Page size in PDF points (1/72 inch).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PageSize {
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PdfInfo {
    pub pages: Vec<PageSize>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// A run of text on a page and where it sits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextSpan {
    pub text: String,
    pub rect: Rect,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub page: u32,
    pub rects: Vec<Rect>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookInfo {
    pub title: Option<String>,
    pub chapters: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    /// Human-readable (label, value) pairs, most useful first.
    pub exif: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReaderBody {
    Pdf(PdfInfo),
    Book(BookInfo),
    /// Sanitised HTML for text, Markdown and HTML files.
    Page(String),
    Image(ImageInfo),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReaderView {
    pub doc: u64,
    pub name: String,
    pub body: ReaderBody,
}
