//! Reconstructed document → DOCX, written directly (spec §5.2) so fonts,
//! sizes and colours are kept: headings as Word heading styles, lists with
//! Word numbering, tables with merged cells, footnotes, images, links, and
//! the running header and footer as the page header and footer.

use super::ast::joined_spans;
use super::layout::{BlockKind, Doc, Line, Script, Span, Table};
use docx_rs::{
    AbstractNumbering, AlignmentType, Docx, Footer, Footnote, Header, Hyperlink, HyperlinkType,
    IndentLevel, Level, LevelJc, LevelText, NumberFormat, Numbering, NumberingId, PageNum,
    Paragraph, Pic, Run, RunFonts, SpecialIndentType, Start, Style, StyleType, TableCell, TableRow,
    VMergeType, VertAlignType,
};

/// An image to place: PNG bytes and its size on the page in points.
pub struct Picture {
    pub png: Vec<u8>,
    pub width: f32,
    pub height: f32,
}

const EMU_PER_PT: f32 = 12700.0;

/// Page and body defaults for the document.
pub struct PageSetup {
    /// Page size in points.
    pub width: f32,
    pub height: f32,
}

fn fonts(f: &str) -> RunFonts {
    RunFonts::new().ascii(f).hi_ansi(f).east_asia(f).cs(f)
}

fn hex(c: [u8; 3]) -> String {
    format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

fn half_points(size: f32) -> usize {
    (size * 2.0).round().max(2.0) as usize
}

/// One span as a run with its look.
fn run(s: &Span, text: &str, plain_bold: bool) -> Run {
    let mut r = Run::new().add_text(text);
    if s.bold && !plain_bold {
        r = r.bold();
    }
    if s.italic {
        r = r.italic();
    }
    if !s.font.is_empty() {
        r = r.fonts(fonts(&s.font));
    } else if s.mono {
        r = r.fonts(fonts("Courier New"));
    }
    if s.size > 0.0 {
        r = r.size(half_points(s.size));
    }
    if s.color != [0, 0, 0] {
        r = r.color(hex(s.color));
    }
    match s.script {
        Script::Super => r.run_property = r.run_property.vert_align(VertAlignType::SuperScript),
        Script::Sub => r.run_property = r.run_property.vert_align(VertAlignType::SubScript),
        Script::Normal => {}
    }
    r
}

/// A paragraph's runs, links and footnote references from its lines.
/// `plain_bold` drops bold that is the look of the whole block (headings).
fn fill(mut p: Paragraph, lines: &[Line], notes: &[Vec<Line>], plain_bold: bool) -> Paragraph {
    let spans = joined_spans(lines);
    let mut i = 0;
    while i < spans.len() {
        let s = &spans[i];
        if let Some(note) = s.note.and_then(|n| notes.get(n)) {
            let mut f = Footnote::new();
            f.add_content(fill(Paragraph::new(), note, &[], false));
            p = p.add_run(Run::new().add_footnote_reference(f));
            if s.text.ends_with(char::is_whitespace) {
                p = p.add_run(Run::new().add_text(" "));
            }
            i += 1;
            continue;
        }
        match &s.link {
            Some(url) => {
                let mut h = Hyperlink::new(url.clone(), HyperlinkType::External);
                while i < spans.len() && spans[i].link.as_deref() == Some(url.as_str()) {
                    h = h.add_run(run(&spans[i], &spans[i].text, plain_bold));
                    i += 1;
                }
                p = p.add_hyperlink(h);
            }
            None => {
                p = p.add_run(run(s, &s.text, plain_bold));
                i += 1;
            }
        }
    }
    p
}

fn table(t: &Table, notes: &[Vec<Line>], text_width: f32) -> docx_rs::Table {
    // Cells spanning down from rows above: (first col, colspan, rows end).
    let mut down: Vec<(usize, usize, usize)> = Vec::new();
    let mut rows = Vec::new();
    for (r, row) in t.rows.iter().enumerate() {
        let mut cells = Vec::new();
        let mut it = row.iter().peekable();
        let mut col = 0;
        while col < t.cols {
            if let Some(&(_, span, _)) = down.iter().find(|d| d.0 == col && d.2 > r) {
                cells.push(
                    TableCell::new()
                        .add_paragraph(Paragraph::new())
                        .grid_span(span)
                        .vertical_merge(VMergeType::Continue),
                );
                col += span;
                continue;
            }
            match it.peek() {
                Some(c) if c.col == col => {
                    let mut cell = TableCell::new().add_paragraph(fill(
                        Paragraph::new(),
                        &c.lines,
                        notes,
                        false,
                    ));
                    if c.colspan > 1 {
                        cell = cell.grid_span(c.colspan);
                    }
                    if c.rowspan > 1 {
                        cell = cell.vertical_merge(VMergeType::Restart);
                        down.push((col, c.colspan, r + c.rowspan));
                    }
                    cells.push(cell);
                    col += c.colspan.max(1);
                    it.next();
                }
                _ => {
                    cells.push(TableCell::new().add_paragraph(Paragraph::new()));
                    col += 1;
                }
            }
        }
        rows.push(TableRow::new(cells));
    }
    let w = (text_width * 20.0 / t.cols.max(1) as f32) as usize;
    docx_rs::Table::new(rows).set_grid(vec![w; t.cols])
}

fn bullet_levels(mut a: AbstractNumbering, ordered: bool) -> AbstractNumbering {
    for lvl in 0..4 {
        let (fmt, text) = if ordered {
            ("decimal", format!("%{}.", lvl + 1))
        } else {
            ("bullet", "•".to_string())
        };
        a = a.add_level(
            Level::new(
                lvl,
                Start::new(1),
                NumberFormat::new(fmt),
                LevelText::new(text),
                LevelJc::new("left"),
            )
            .indent(
                Some(720 * (lvl as i32 + 1)),
                Some(SpecialIndentType::Hanging(360)),
                None,
                None,
            ),
        );
    }
    a
}

/// Writes the document; returns the .docx bytes.
pub fn write_docx(doc: &Doc, pictures: &[Picture], page: &PageSetup) -> Result<Vec<u8>, String> {
    let body = super::layout::body_style(doc);
    let mut d = Docx::new()
        .page_size(
            (page.width * 20.0).round() as u32,
            (page.height * 20.0).round() as u32,
        )
        .default_size(half_points(body.1));
    if !body.0.is_empty() {
        d = d.default_fonts(fonts(&body.0));
    }
    for n in 1..=6 {
        d = d.add_style(
            Style::new(format!("Heading{n}"), StyleType::Paragraph)
                .name(format!("Heading {n}"))
                .based_on("Normal")
                .bold(),
        );
    }
    // Margins of 1 inch on each side.
    let text_width = (page.width - 144.0).max(144.0);
    let mut next_num = 1usize;
    let mut k = 0;
    let blocks = &doc.blocks;
    while k < blocks.len() {
        let b = &blocks[k];
        match &b.kind {
            BlockKind::Heading(n) => {
                let all_bold = b
                    .lines
                    .iter()
                    .flat_map(|l| &l.spans)
                    .filter(|s| !s.text.trim().is_empty())
                    .all(|s| s.bold);
                d = d.add_paragraph(fill(
                    Paragraph::new().style(&format!("Heading{n}")),
                    &b.lines,
                    &doc.notes,
                    all_bold,
                ));
            }
            BlockKind::Para => {
                d = d.add_paragraph(fill(Paragraph::new(), &b.lines, &doc.notes, false));
            }
            BlockKind::Image(i) => {
                if let Some(p) = pictures.get(*i) {
                    let w = (p.width.min(text_width) * EMU_PER_PT) as u32;
                    let h =
                        (p.height * p.width.min(text_width) / p.width.max(1.0) * EMU_PER_PT) as u32;
                    let pic = Pic::new(&p.png).size(w, h);
                    d = d.add_paragraph(Paragraph::new().add_run(Run::new().add_image(pic)));
                }
            }
            BlockKind::Table(t) => {
                d = d.add_table(table(t, &doc.notes, text_width));
            }
            BlockKind::ListItem { .. } => {
                // One numbering per list, so numbered lists restart at 1.
                let ordered = matches!(b.kind, BlockKind::ListItem { ordered: true, .. });
                let id = next_num;
                next_num += 1;
                d = d
                    .add_abstract_numbering(bullet_levels(AbstractNumbering::new(id), ordered))
                    .add_numbering(Numbering::new(id, id));
                let min = blocks[k..]
                    .iter()
                    .map_while(|b| match b.kind {
                        BlockKind::ListItem { level, .. } => Some(level),
                        _ => None,
                    })
                    .min()
                    .unwrap_or(0);
                while let Some(BlockKind::ListItem { level, .. }) = blocks.get(k).map(|b| &b.kind) {
                    let p = Paragraph::new().numbering(
                        NumberingId::new(id),
                        IndentLevel::new((level - min) as usize),
                    );
                    d = d.add_paragraph(fill(p, &blocks[k].lines, &doc.notes, false));
                    k += 1;
                }
                continue;
            }
        }
        k += 1;
    }
    if let Some(h) = &doc.header {
        d = d.header(Header::new().add_paragraph(Paragraph::new().add_run(Run::new().add_text(h))));
    }
    if doc.footer.is_some() || doc.page_numbers {
        let mut p = Paragraph::new().align(AlignmentType::Center);
        if let Some(f) = &doc.footer {
            p = p.add_run(Run::new().add_text(format!("{f} ")));
        }
        if doc.page_numbers {
            p = p.add_page_num(PageNum::new());
        }
        d = d.footer(Footer::new().add_paragraph(p));
    }
    let mut out = std::io::Cursor::new(Vec::new());
    d.build()
        .pack(&mut out)
        .map_err(|e| format!("Couldn't write the Word file: {e}"))?;
    Ok(out.into_inner())
}
