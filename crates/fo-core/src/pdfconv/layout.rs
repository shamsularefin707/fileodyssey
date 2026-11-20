//! Reading order and structure from positioned characters.
//!
//! 1. **Fragments.** Characters are taken in content-stream order and cut
//!    into line pieces where the text jumps to another line or across a wide
//!    gap. Inside a piece the stream order is kept: shaping can draw a
//!    character to the left of the one before it (Bengali ি), so sorting by
//!    position would scramble words.
//! 2. **Margins.** Page numbers and lines repeated in the top/bottom margin
//!    of most pages (running headers/footers) are dropped.
//! 3. **XY-cut.** Each page is split recursively: at a whitespace gap that
//!    runs the full height of the region (columns, read left to right), else
//!    at its topmost wide horizontal gap (read top to bottom). Leaves are
//!    single-column runs.
//! 4. **Blocks.** In each leaf, pieces on one baseline join into lines;
//!    lines group into paragraphs by spacing, size, weight and list markers;
//!    blocks are then classified as headings (larger or bold-only short
//!    lines), list items or paragraphs. Images take part in the cut, so they
//!    land between the paragraphs around them.

use super::notes::split_notes;
use super::table::find_tables;
pub use super::table::{Cell, Table};
use super::{Glyph, PageContent};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Script {
    #[default]
    Normal,
    Super,
    Sub,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub mono: bool,
    pub link: Option<String>,
    /// Font family, "" when unknown.
    pub font: String,
    /// Size in points.
    pub size: f32,
    pub color: [u8; 3],
    pub script: Script,
    /// Index of the footnote this span marks (it is the note's reference).
    pub note: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Line {
    pub spans: Vec<Span>,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    /// Largest character size on the line.
    pub size: f32,
    /// Ends with a hyphen PDFium marked as breaking a word.
    pub hyphen_end: bool,
    /// Every drawn character is bold.
    pub bold: bool,
}

impl Line {
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlockKind {
    Heading(u8),
    Para,
    ListItem {
        ordered: bool,
        level: u8,
    },
    /// Index into all images, numbered page by page.
    Image(usize),
    Table(Table),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub kind: BlockKind,
    pub lines: Vec<Line>,
}

impl Block {
    /// Plain text with lines joined (hyphenated words rejoined).
    pub fn text(&self) -> String {
        let mut out = String::new();
        for (i, l) in self.lines.iter().enumerate() {
            let t = l.text();
            if i > 0 && !self.lines[i - 1].hyphen_end && !out.ends_with(' ') {
                out.push(' ');
            }
            out.push_str(if l.hyphen_end {
                strip_hyphen(&t)
            } else {
                t.trim_end()
            });
        }
        out.trim().to_owned()
    }
}

pub fn strip_hyphen(t: &str) -> &str {
    t.trim_end()
        .trim_end_matches(['-', '\u{ad}', '\u{2}', '\u{2010}'])
}

// ---------------------------------------------------------------- fragments

#[derive(Clone)]
struct Frag {
    line: Line,
    /// Gaps (in em) at each `MAYBE_SPACE` placeholder, in order.
    gaps: Vec<f32>,
    /// Bottom edge of each span's glyphs (spans don't mix sizes).
    bottoms: Vec<f32>,
    last_x0: f32,
    last_x1: f32,
    last_ymid: f32,
    last_size: f32,
}

#[derive(Clone)]
enum Item {
    Text(Line),
    Image {
        idx: usize,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
    },
    Table(Box<Table>, [f32; 4]),
}

impl Item {
    fn bbox(&self) -> (f32, f32, f32, f32) {
        match self {
            Item::Text(l) => (l.x0, l.y0, l.x1, l.y1),
            Item::Image { x0, y0, x1, y1, .. } => (*x0, *y0, *x1, *y1),
            Item::Table(_, b) => (b[0], b[1], b[2], b[3]),
        }
    }
}

fn link_at(page: &PageContent, g: &Glyph) -> Option<String> {
    let (cx, cy) = ((g.x0 + g.x1) / 2.0, (g.y0 + g.y1) / 2.0);
    page.links
        .iter()
        .find(|l| cx >= l.x0 && cx <= l.x1 && cy >= l.y0 && cy <= l.y1)
        .map(|l| l.uri.clone())
}

fn is_hyphen_glyph(g: &Glyph) -> bool {
    g.hyphen || matches!(g.text.as_str(), "\u{ad}" | "\u{2}")
}

fn fragments(page: &PageContent) -> Vec<Line> {
    let mut out = Vec::new();
    let mut cur: Option<Frag> = None;
    let mut space = false;
    let mut real_space = false;
    // A generated line break was seen since the last drawn glyph.
    let mut brk = false;
    let mut recent: Vec<String> = Vec::new();
    // Index in `recent` of the last glyph a mark copy repeated (see below).
    let mut dup: Option<usize> = None;
    // Drawn space objects decide word gaps only on pages whose text has no
    // real space characters (Chrome drawing a complex script); elsewhere a
    // stray space object says nothing about the rest of the page.
    let drawn_spaces =
        !page.spaces.is_empty() && !page.glyphs.iter().any(|g| !g.generated && g.text == " ");
    let finish = |cur: &mut Option<Frag>, out: &mut Vec<Line>| {
        if let Some(mut f) = cur.take() {
            resolve_spaces(&mut f);
            mark_scripts(&mut f);
            if !f.line.text().trim().is_empty() {
                out.push(f.line);
            }
        }
    };
    for g in &page.glyphs {
        let blank = g.text.trim().is_empty() && !matches!(g.text.as_str(), "\u{ad}" | "\u{2}");
        if g.generated && (g.text.contains('\n') || g.text.contains('\r')) {
            // Provisional: PDFium also breaks before a raised superscript
            // that carries on the same line.
            brk = true;
            space = false;
            continue;
        }
        if blank {
            space = true;
            real_space |= !g.generated;
            continue;
        }
        if g.x1 <= g.x0 && g.y1 <= g.y0 {
            continue;
        }
        let ymid = (g.y0 + g.y1) / 2.0;
        // A tiny glyph repeating text just drawn is a mark (e.g. a Bengali
        // vowel sign below its letter) whose text map repeats the cluster.
        // Its text map repeats the cluster: the glyphs of the cluster drawn
        // just after it, at full size, repeat theirs too (in order), and are
        // dropped while they keep matching what followed the repeated part.
        let next_dup = dup.filter(|&p| recent.get(p + 1) == Some(&g.text));
        if g.y1 - g.y0 < 0.45 * g.size {
            if let Some(p) = next_dup {
                dup = Some(p + 1);
                continue;
            }
            if let Some(i) = recent.iter().rposition(|t| *t == g.text) {
                dup = Some(i);
                continue;
            }
            if (1..=recent.len()).any(|k| recent[recent.len() - k..].concat() == g.text) {
                dup = Some(recent.len() - 1);
                continue;
            }
        } else if let Some(p) = next_dup {
            dup = Some(p + 1);
            continue;
        }
        dup = None;
        let mut gap_em = 0.0;
        if let Some(f) = &cur {
            gap_em = (g.x0 - f.last_x1) / g.size.max(1.0);
            if drawn_spaces {
                // Drawn spaces decide: is one between the last glyph and this?
                let (a, b) = (f.last_x1.min(g.x0) - 1.0, f.last_x1.max(g.x0) + 1.0);
                let drawn = page.spaces.iter().any(|s| {
                    let cx = (s[0] + s[2]) / 2.0;
                    let pad = 0.3 * g.size;
                    cx >= a && cx <= b && s[1] <= g.y1 + pad && s[3] >= g.y0 - pad
                });
                gap_em = if drawn { 1.0 } else { 0.0 };
            }
            let fs = g.size.max(f.last_size).max(1.0);
            // Superscripts and subscripts are smaller and shifted but belong to
            // the line they sit on.
            let script = g.size < 0.85 * f.last_size || f.last_size < 0.85 * g.size;
            let vert = (ymid - f.last_ymid).abs() > if script { 0.9 } else { 0.55 } * fs;
            let gap = g.x0 - f.last_x1;
            let back = g.x1 < f.last_x0 - g.size.max(1.0);
            let carries_on = !vert && !back && gap.abs() < 0.3 * fs;
            if vert || back || gap > 1.0 * fs || (brk && !carries_on) {
                finish(&mut cur, &mut out);
                space = false;
            } else if gap > 0.3 * g.size {
                // A visible gap with no space character (some producers draw none).
                space = true;
            }
        }
        let link = link_at(page, g);
        let f = cur.get_or_insert_with(|| Frag {
            line: Line {
                x0: g.x0,
                y0: g.y0,
                x1: g.x1,
                y1: g.y1,
                bold: true,
                ..Line::default()
            },
            last_x0: g.x0,
            last_x1: g.x1,
            last_ymid: ymid,
            last_size: g.size,
            gaps: Vec::new(),
            bottoms: Vec::new(),
        });
        let l = &mut f.line;
        let style = |s: &Span| {
            s.bold == g.bold
                && s.italic == g.italic
                && s.mono == g.mono
                && s.link == link
                && s.font == g.font
                && s.color == g.color
                && (s.size - g.size).abs() < 0.25
        };
        if space && !l.spans.is_empty() {
            if real_space {
                l.spans.last_mut().expect("non-empty").text.push(' ');
            } else {
                l.spans
                    .last_mut()
                    .expect("non-empty")
                    .text
                    .push(MAYBE_SPACE);
                f.gaps.push(gap_em);
            }
        }
        match l.spans.last_mut() {
            Some(s) if style(s) => {
                s.text.push_str(&g.text);
                if let Some(b) = f.bottoms.last_mut() {
                    *b = b.max(g.y1);
                }
            }
            _ => {
                l.spans.push(Span {
                    text: g.text.clone(),
                    bold: g.bold,
                    italic: g.italic,
                    mono: g.mono,
                    link,
                    font: g.font.clone(),
                    size: g.size,
                    color: g.color,
                    ..Span::default()
                });
                f.bottoms.push(g.y1);
            }
        }
        l.x0 = l.x0.min(g.x0);
        l.y0 = l.y0.min(g.y0);
        l.x1 = l.x1.max(g.x1);
        l.y1 = l.y1.max(g.y1);
        l.size = l.size.max(g.size);
        l.bold &= g.bold;
        l.hyphen_end = is_hyphen_glyph(g);
        f.last_x0 = g.x0;
        f.last_x1 = f.last_x1.max(g.x1);
        f.last_ymid = ymid;
        f.last_size = g.size;
        space = false;
        real_space = false;
        brk = false;
        recent.push(g.text.clone());
        if recent.len() > 8 {
            recent.remove(0);
        }
    }
    finish(&mut cur, &mut out);
    out
}

/// Smaller spans raised above (or dropped below) the line's main text are
/// superscripts (subscripts). The main text is the size with the most
/// characters.
fn mark_scripts(f: &mut Frag) {
    let mut by_size: Vec<(f32, usize, f32)> = Vec::new();
    for (s, b) in f.line.spans.iter().zip(&f.bottoms) {
        let n = s.text.trim().chars().count();
        match by_size.iter_mut().find(|e| (e.0 - s.size).abs() < 0.25) {
            Some(e) => {
                e.1 += n;
                e.2 = e.2.max(*b);
            }
            None => by_size.push((s.size, n, *b)),
        }
    }
    let Some(&(main, _, bottom)) = by_size.iter().max_by_key(|e| e.1) else {
        return;
    };
    for (s, b) in f.line.spans.iter_mut().zip(&f.bottoms) {
        if s.size < 0.85 * main && !s.text.trim().is_empty() {
            if *b < bottom - 0.2 * main {
                s.script = Script::Super;
            } else if *b > bottom + 0.1 * main {
                s.script = Script::Sub;
            }
        }
    }
}

/// Placeholder for "maybe a word space", decided per line by `resolve_spaces`.
const MAYBE_SPACE: char = '\u{E000}';

/// Some producers (Chrome with Bengali, for one) draw no spaces and place
/// every cluster separately, so PDFium proposes a space between clusters
/// too. Within one line those gaps fall in two groups — small ones inside
/// words, larger ones between words — so split at the biggest jump. Lines
/// whose gaps are all alike (Latin, justified or not) keep them all.
fn resolve_spaces(f: &mut Frag) {
    if f.gaps.is_empty() {
        return;
    }
    let mut g = f.gaps.clone();
    g.sort_by(f32::total_cmp);
    let mut threshold = 0.1;
    if g.len() >= 3 {
        // Only a jump just above the inside-word range counts; jumps among
        // word gaps (0.3em vs 0.7em) are not a split.
        let best = (0..g.len() - 1)
            .filter(|&i| g[i] <= 0.2)
            .map(|i| (i, g[i + 1] / g[i].max(0.02)))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, ratio)) = best {
            if ratio >= 1.35 {
                threshold = (g[i] + g[i + 1]) / 2.0;
            }
        }
    } else {
        threshold = 0.15;
    }

    let mut k = 0;
    for s in &mut f.line.spans {
        let mut out = String::with_capacity(s.text.len());
        for c in s.text.chars() {
            if c == MAYBE_SPACE {
                if f.gaps[k] > threshold {
                    out.push(' ');
                }
                k += 1;
            } else {
                out.push(c);
            }
        }
        s.text = out;
    }
}

// ------------------------------------------------------------------ margins

fn page_number_like(t: &str) -> bool {
    let t = t.trim().to_lowercase();
    let t = t.trim_start_matches("page").trim();
    !t.is_empty()
        && t.chars()
            .all(|c| c.is_ascii_digit() || " /-–of".contains(c))
        && t.chars().any(|c| c.is_ascii_digit())
}

fn margin_key(l: &Line) -> String {
    l.text()
        .chars()
        .map(|c| if c.is_ascii_digit() { '#' } else { c })
        .collect::<String>()
        .trim()
        .to_owned()
}

/// What the page margins held: the running header and footer text, and
/// whether pages are numbered.
#[derive(Debug, Default)]
struct Margins {
    header: Option<String>,
    footer: Option<String>,
    page_numbers: bool,
    set_aside: Vec<String>,
}

fn drop_margins(pages: &[PageContent], frags: &mut [Vec<Line>]) -> Margins {
    let in_margin = |l: &Line, h: f32| l.y1 < 0.08 * h || l.y0 > 0.92 * h;
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (p, fs) in pages.iter().zip(frags.iter()) {
        let mut keys: Vec<String> = fs
            .iter()
            .filter(|l| in_margin(l, p.height))
            .map(margin_key)
            .collect();
        keys.sort();
        keys.dedup();
        for k in keys {
            *seen.entry(k).or_default() += 1;
        }
    }
    let n = pages.len();
    let mut m = Margins::default();
    let mut best: [(usize, Option<String>); 2] = [(0, None), (0, None)];
    for (p, fs) in pages.iter().zip(frags.iter_mut()) {
        fs.retain(|l| {
            if !in_margin(l, p.height) {
                return true;
            }
            let count = seen.get(&margin_key(l)).copied().unwrap_or(0);
            let repeated = n >= 2 && (count == n || (n >= 3 && count * 2 >= n));
            let text = l.text().trim().to_owned();
            if page_number_like(&text) {
                m.page_numbers = true;
                m.set_aside.push(text);
                return false;
            }
            if repeated {
                m.set_aside.push(text.clone());
                let slot = &mut best[(l.y0 > 0.5 * p.height) as usize];
                if count > slot.0 {
                    *slot = (count, Some(text));
                }
                return false;
            }
            true
        });
    }
    let [(_, header), (_, footer)] = best;
    m.header = header;
    m.footer = footer;
    m
}

// ------------------------------------------------------------------- xy-cut

/// Gaps between the merged [start, end] intervals, as (gap_size, cut_at).
fn gaps(mut iv: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    iv.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = Vec::new();
    let mut end = f32::MIN;
    for (s, e) in iv {
        if end > f32::MIN && s > end {
            out.push((s - end, (s + end) / 2.0));
        }
        end = end.max(e);
    }
    out
}

fn xycut(items: Vec<Item>, body: f32, leaves: &mut Vec<Vec<Item>>) {
    if items.len() <= 1 {
        if !items.is_empty() {
            leaves.push(items);
        }
        return;
    }
    // Columns: a gap no item crosses, from the top of the region to the bottom.
    let v = gaps(items.iter().map(|i| (i.bbox().0, i.bbox().2)).collect());
    if let Some((_, at)) = v
        .into_iter()
        .filter(|(g, _)| *g >= (1.0 * body).max(8.0))
        .max_by(|a, b| a.0.total_cmp(&b.0))
    {
        let (left, right): (Vec<Item>, Vec<Item>) =
            items.into_iter().partition(|i| i.bbox().2 <= at);
        xycut(left, body, leaves);
        xycut(right, body, leaves);
        return;
    }
    // Otherwise the topmost wide horizontal gap: it peels off full-width
    // content (a title, a spanning figure) so the rest can split into
    // columns. The widest gap could lie inside one column below where the
    // other one ends, which would interleave the columns.
    let h = gaps(items.iter().map(|i| (i.bbox().1, i.bbox().3)).collect());
    if let Some((_, at)) = h
        .into_iter()
        .filter(|(g, _)| *g >= 0.9 * body)
        .min_by(|a, b| a.1.total_cmp(&b.1))
    {
        let (top, bottom): (Vec<Item>, Vec<Item>) =
            items.into_iter().partition(|i| i.bbox().3 <= at);
        xycut(top, body, leaves);
        xycut(bottom, body, leaves);
        return;
    }
    leaves.push(items);
}

// ------------------------------------------------------------------- blocks

pub(crate) fn join_same_baseline(mut lines: Vec<Line>) -> Vec<Line> {
    lines.sort_by(|a, b| a.y0.total_cmp(&b.y0).then(a.x0.total_cmp(&b.x0)));
    let mut out: Vec<Line> = Vec::new();
    for l in lines {
        if let Some(prev) = out.last_mut() {
            let overlap = prev.y1.min(l.y1) - prev.y0.max(l.y0);
            let minh = (prev.y1 - prev.y0).min(l.y1 - l.y0).max(0.1);
            if overlap > 0.5 * minh && l.x0 >= prev.x1 - 1.0 {
                if l.x0 - prev.x1 > 0.22 * l.size {
                    if let Some(s) = prev.spans.last_mut() {
                        s.text.push(' ');
                    }
                }
                prev.spans.extend(l.spans);
                prev.x1 = prev.x1.max(l.x1);
                prev.y0 = prev.y0.min(l.y0);
                prev.y1 = prev.y1.max(l.y1);
                prev.size = prev.size.max(l.size);
                prev.bold &= l.bold;
                prev.hyphen_end = l.hyphen_end;
                continue;
            }
        }
        out.push(l);
    }
    out
}

/// `(ordered, marker length in bytes)` if the line starts like a list item.
fn list_marker(t: &str) -> Option<(bool, usize)> {
    let lead = t.len() - t.trim_start().len();
    let s = &t[lead..];
    let mut chars = s.char_indices();
    let (_, c0) = chars.next()?;
    if "•◦▪‣∙●○■□–—*·-".contains(c0) {
        let rest = &s[c0.len_utf8()..];
        return rest
            .starts_with(' ')
            .then(|| (false, lead + c0.len_utf8() + 1));
    }
    let s2 = s.strip_prefix('(').unwrap_or(s);
    let paren = s.len() - s2.len();
    let n = s2.find(['.', ')'])?;
    let label = &s2[..n];
    let numeric =
        !label.is_empty() && label.len() <= 3 && label.chars().all(|c| c.is_ascii_digit());
    let letter = label.len() == 1 && label.chars().all(|c| c.is_ascii_alphabetic());
    let roman = !label.is_empty() && label.len() <= 5 && label.chars().all(|c| "ivxlc".contains(c));
    let after = &s2[n + 1..];
    ((numeric || letter || roman) && after.starts_with(' ')).then(|| (true, lead + paren + n + 2))
}

fn strip_marker(l: &mut Line, len: usize) {
    let mut left = len;
    for s in &mut l.spans {
        if left == 0 {
            break;
        }
        let cut = left.min(s.text.len());
        let cut = (0..=cut)
            .rev()
            .find(|i| s.text.is_char_boundary(*i))
            .unwrap_or(0);
        s.text.drain(..cut);
        left -= cut;
    }
    l.spans.retain(|s| !s.text.is_empty());
    if let Some(s) = l.spans.first_mut() {
        let t = s.text.trim_start().to_owned();
        s.text = t;
    }
}

struct Proto {
    lines: Vec<Line>,
    list: Option<bool>,
    image: Option<usize>,
    /// The list marker taken off the first line ("a. ").
    marker: String,
    table: Option<Table>,
}

fn leaf_blocks(items: Vec<Item>, out: &mut Vec<Proto>) {
    let mut texts = Vec::new();
    let mut rest: Vec<(f32, Proto)> = Vec::new();
    for i in items {
        match i {
            Item::Text(l) => texts.push(l),
            Item::Image { idx, y0, .. } => rest.push((
                y0,
                Proto {
                    lines: vec![],
                    list: None,
                    image: Some(idx),
                    marker: String::new(),
                    table: None,
                },
            )),
            Item::Table(t, b) => rest.push((
                b[1],
                Proto {
                    lines: vec![],
                    list: None,
                    image: None,
                    marker: String::new(),
                    table: Some(*t),
                },
            )),
        }
    }
    let lines = join_same_baseline(texts);
    // A line that stops well short of the column and ends a sentence ends
    // its paragraph (list items whose bullets are drawn, not text).
    let col_x0 = lines.iter().map(|l| l.x0).fold(f32::MAX, f32::min);
    let col_x1 = lines.iter().map(|l| l.x1).fold(f32::MIN, f32::max);
    let col_w = (col_x1 - col_x0).max(1.0);
    let lines_in_leaf = lines.len();
    let mut cur: Option<Proto> = None;
    let mut prev: Option<Line> = None;
    for l in lines {
        let marker = list_marker(&l.text());
        let new = match (&cur, &prev) {
            (Some(c), Some(p)) => {
                let gap = l.y0 - p.y1;
                let size_change = (l.size - p.size).abs() > 0.12 * p.size.max(l.size);
                let weight_change = l.bold != p.bold;
                let sentence_end = p.text().trim_end().ends_with(['.', '!', '?', ':', '।']);
                let indent = l.x0 > c.lines[0].x0 + 1.5 * l.size && sentence_end;
                let short_end = sentence_end && (p.x1 - col_x0) < 0.7 * col_w && lines_in_leaf > 2;
                marker.is_some()
                    || gap > 0.6 * l.size
                    || size_change
                    || weight_change
                    || indent
                    || short_end
                    || gap < -0.5 * l.size
            }
            _ => true,
        };
        if new {
            if let Some(c) = cur.take() {
                rest.push((c.lines[0].y0, c));
            }
            let mut l = l.clone();
            let text = l.text();
            let list = marker.map(|(ordered, len)| {
                strip_marker(&mut l, len);
                ordered
            });
            let taken = text.len() - text.trim_start().len() + marker.map_or(0, |m| m.1);
            cur = Some(Proto {
                lines: vec![l],
                list,
                image: None,
                marker: text.get(..taken).unwrap_or("").trim_start().to_owned(),
                table: None,
            });
        } else if let Some(c) = cur.as_mut() {
            c.lines.push(l.clone());
        }
        prev = Some(l);
    }
    if let Some(c) = cur.take() {
        rest.push((c.lines[0].y0, c));
    }
    rest.sort_by(|a, b| a.0.total_cmp(&b.0));
    out.extend(rest.into_iter().map(|(_, p)| p));
}

/// Most common character size, weighted by character count.
fn body_size(pages: &[PageContent]) -> f32 {
    let mut counts: std::collections::HashMap<i32, usize> = std::collections::HashMap::new();
    for g in pages.iter().flat_map(|p| &p.glyphs) {
        if !g.generated && !g.text.trim().is_empty() {
            *counts.entry((g.size * 2.0).round() as i32).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(s, _)| s as f32 / 2.0)
        .unwrap_or(10.0)
}

/// A reconstructed document: the body, its footnotes (referenced by
/// `Span::note`), and the running header and footer kept apart.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Doc {
    pub blocks: Vec<Block>,
    pub notes: Vec<Vec<Line>>,
    pub header: Option<String>,
    pub footer: Option<String>,
    pub page_numbers: bool,
    /// Text set aside as running header/footer or page numbers, every
    /// occurrence (the report doesn't count it as lost).
    pub set_aside: Vec<String>,
}

/// The body blocks only (see `reconstruct`).
pub fn layout(pages: &[PageContent]) -> Vec<Block> {
    reconstruct(pages).blocks
}

pub fn reconstruct(pages: &[PageContent]) -> Doc {
    let body = body_size(pages);
    let mut frags: Vec<Vec<Line>> = pages.iter().map(fragments).collect();
    let margins = drop_margins(pages, &mut frags);
    let mut notes = Vec::new();
    let mut protos = Vec::new();
    let mut image_base = 0;
    for (p, fs) in pages.iter().zip(frags) {
        let mut fs = fs;
        let tables = find_tables(p, &mut fs, body);
        split_notes(p.height, &mut fs, body, &mut notes);
        let mut items: Vec<Item> = fs.into_iter().map(Item::Text).collect();
        items.extend(tables.into_iter().map(|(b, t)| Item::Table(Box::new(t), b)));
        for (i, im) in p.images.iter().enumerate() {
            items.push(Item::Image {
                idx: image_base + i,
                x0: im.x0,
                y0: im.y0,
                x1: im.x1,
                y1: im.y1,
            });
        }
        image_base += p.images.len();
        let mut leaves = Vec::new();
        xycut(items, body, &mut leaves);
        for leaf in leaves {
            leaf_blocks(leaf, &mut protos);
        }
    }
    Doc {
        blocks: classify(protos, body),
        notes,
        header: margins.header,
        footer: margins.footer,
        page_numbers: margins.page_numbers,
        set_aside: margins.set_aside,
    }
}

fn classify(mut protos: Vec<Proto>, body: f32) -> Vec<Block> {
    // A lettered "item" with no neighbour lettered one before or after it is
    // an initial ("R. Hossain"), not a list.
    let letter = |p: &Proto| {
        p.marker
            .trim_start_matches('(')
            .chars()
            .next()
            .filter(|c| c.is_alphabetic() && p.list == Some(true))
            .map(|c| c.to_ascii_lowercase() as i32)
    };
    let lone: Vec<usize> = (0..protos.len())
        .filter(|&i| {
            let Some(c) = letter(&protos[i]) else {
                return false;
            };
            let next_to = |j: Option<usize>, d: i32| {
                j.and_then(|j| protos.get(j)).and_then(letter) == Some(c + d)
            };
            !next_to(i.checked_sub(1), -1) && !next_to(Some(i + 1), 1)
        })
        .collect();
    for i in lone {
        let p = &mut protos[i];
        p.list = None;
        if let Some(s) = p.lines.first_mut().and_then(|l| l.spans.first_mut()) {
            s.text.insert_str(0, &p.marker);
        }
    }
    let size_of = |p: &Proto| {
        let mut s: Vec<f32> = p.lines.iter().map(|l| l.size).collect();
        s.sort_by(f32::total_cmp);
        s.get(s.len() / 2).copied().unwrap_or(body)
    };
    let text_of = |p: &Proto| {
        Block {
            kind: BlockKind::Para,
            lines: p.lines.clone(),
        }
        .text()
    };
    let is_big = |p: &Proto| {
        p.image.is_none() && p.list.is_none() && size_of(p) >= body * 1.12 && p.lines.len() <= 4
    };
    let is_bold_head = |p: &Proto| {
        let t = text_of(p);
        p.image.is_none()
            && p.list.is_none()
            && p.lines.len() <= 2
            && p.lines.iter().all(|l| l.bold)
            && t.chars().count() <= 150
            && !t.ends_with(['.', ',', ';'])
    };
    let mut sizes: Vec<i32> = protos
        .iter()
        .filter(|p| is_big(p))
        .map(|p| (size_of(p) * 2.0).round() as i32)
        .collect();
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    sizes.dedup();
    let list_x: Vec<f32> = protos
        .iter()
        .filter(|p| p.list.is_some())
        .map(|p| p.lines[0].x0)
        .collect();
    let min_x = list_x.iter().copied().fold(f32::MAX, f32::min);
    protos
        .into_iter()
        .map(|p| {
            let kind = if let Some(t) = p.table.clone() {
                BlockKind::Table(t)
            } else if let Some(i) = p.image {
                BlockKind::Image(i)
            } else if is_big(&p) {
                let key = (size_of(&p) * 2.0).round() as i32;
                BlockKind::Heading(
                    (sizes.iter().position(|s| *s == key).unwrap_or(0) + 1).min(6) as u8,
                )
            } else if is_bold_head(&p) {
                BlockKind::Heading((sizes.len() + 1).min(6) as u8)
            } else if let Some(ordered) = p.list {
                let indent = ((p.lines[0].x0 - min_x) / (2.0 * body))
                    .round()
                    .clamp(0.0, 3.0) as u8;
                BlockKind::ListItem {
                    ordered,
                    level: indent,
                }
            } else {
                BlockKind::Para
            };
            Block {
                kind,
                lines: p.lines,
            }
        })
        .collect()
}
