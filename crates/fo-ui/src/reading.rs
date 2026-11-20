//! Pure helpers for the reader view (no DOM), so they can be unit-tested.

use fo_types::{FileKind, ImageFormat, PageSize};

/// Zoom levels, as multiples of "fit width".
pub const ZOOMS: [f64; 8] = [0.5, 0.67, 0.8, 1.0, 1.25, 1.5, 2.0, 3.0];

pub fn zoom_in(z: f64) -> f64 {
    ZOOMS
        .iter()
        .copied()
        .find(|s| *s > z + 1e-6)
        .unwrap_or(ZOOMS[ZOOMS.len() - 1])
}

pub fn zoom_out(z: f64) -> f64 {
    ZOOMS
        .iter()
        .rev()
        .copied()
        .find(|s| *s < z - 1e-6)
        .unwrap_or(ZOOMS[0])
}

/// Bitmap width to request for a page shown `css_width` px wide: rounded up
/// to 256 so zooming reuses cached bitmaps, and within 256…4096.
pub fn bitmap_width(css_width: f64, dpr: f64) -> u32 {
    let px = (css_width * dpr.max(1.0)).ceil() as u32;
    px.div_ceil(256).saturating_mul(256).clamp(256, 4096)
}

/// Where each page sits in the scrolling column.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    pub tops: Vec<f64>,
    pub heights: Vec<f64>,
    pub widths: Vec<f64>,
    pub total: f64,
}

/// Pages laid out `width` px wide (landscape pages too), `gap` px apart.
pub fn layout(pages: &[PageSize], width: f64, gap: f64) -> Layout {
    let mut l = Layout::default();
    let mut y = 0.0;
    for (i, p) in pages.iter().enumerate() {
        if i > 0 {
            y += gap;
        }
        let h = if p.w > 0.0 {
            width * f64::from(p.h) / f64::from(p.w)
        } else {
            width
        };
        l.tops.push(y);
        l.heights.push(h);
        l.widths.push(width);
        y += h;
    }
    l.total = y;
    l
}

/// The page that fills most of the viewport.
pub fn current_page(l: &Layout, scroll_top: f64, view_h: f64) -> usize {
    let (a, b) = (scroll_top, scroll_top + view_h);
    let mut best = (0usize, 0.0f64);
    for (i, (&t, &h)) in l.tops.iter().zip(&l.heights).enumerate() {
        let seen = (b.min(t + h) - a.max(t)).max(0.0);
        if seen > best.1 {
            best = (i, seen);
        }
    }
    if best.1 > 0.0 {
        best.0
    } else {
        l.tops.iter().rposition(|&t| t <= scroll_top).unwrap_or(0)
    }
}

/// Position (0–1) of a point `y` (page fraction) on page `page` along the
/// whole document, for the search hit map.
pub fn hit_mark(l: &Layout, page: usize, y: f64) -> f64 {
    match (l.tops.get(page), l.heights.get(page)) {
        (Some(t), Some(h)) if l.total > 0.0 => (t + h * y) / l.total,
        _ => 0.0,
    }
}

/// Chapters worth listing: those with a title.
pub fn chapter_list(titles: &[String]) -> Vec<(usize, String)> {
    titles
        .iter()
        .enumerate()
        .filter(|(_, t)| !t.trim().is_empty())
        .map(|(i, t)| (i, t.clone()))
        .collect()
}

/// Title of part `i`, or of the titled chapter it continues.
pub fn chapter_title(titles: &[String], i: usize) -> String {
    titles[..=i.min(titles.len().saturating_sub(1))]
        .iter()
        .rev()
        .find(|t| !t.trim().is_empty())
        .cloned()
        .unwrap_or_default()
}

/// Label for the button that moves past part `i`, if there is a next part.
pub fn next_label(titles: &[String], i: usize) -> Option<String> {
    let next = titles.get(i + 1)?;
    Some(if next.trim().is_empty() {
        "Continue reading".to_string()
    } else {
        format!("Next: {next}")
    })
}

/// Files the Read tab can open now.
pub fn readable(kind: &FileKind) -> bool {
    match kind {
        FileKind::Pdf
        | FileKind::Epub
        | FileKind::Markdown
        | FileKind::Html
        | FileKind::Text
        | FileKind::Csv
        | FileKind::Json => true,
        FileKind::Image(f) => !matches!(f, ImageFormat::Heic | ImageFormat::Avif),
        _ => false,
    }
}

/// What clicking a link in a sanitised document does (see `fo_core::reader::text`).
#[derive(Debug, Clone, PartialEq)]
pub enum LinkAction {
    Chapter(usize, Option<String>),
    Anchor(String),
    External(String),
    Nothing,
}

pub fn link_action(href: &str) -> LinkAction {
    if let Some(url) = href.strip_prefix("fo-ext:") {
        return LinkAction::External(url.to_owned());
    }
    if let Some(id) = href.strip_prefix('#') {
        return if id.is_empty() {
            LinkAction::Nothing
        } else {
            LinkAction::Anchor(id.to_owned())
        };
    }
    if let Some(rest) = href.strip_prefix("fo-ch:") {
        let (n, anchor) = match rest.split_once('#') {
            Some((n, a)) => (n, Some(a.to_owned()).filter(|a| !a.is_empty())),
            None => (rest, None),
        };
        return n
            .parse()
            .map(|n| LinkAction::Chapter(n, anchor))
            .unwrap_or(LinkAction::Nothing);
    }
    LinkAction::Nothing
}
