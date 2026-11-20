//! Table detection: ruled grids and whitespace grids (synthetic pages).
use fo_core::pdfconv::layout::{layout, BlockKind, Table};
use fo_core::pdfconv::{Glyph, PageContent, Rule};

/// Glyphs for `text` with its top-left at (x, y), 0.5em per char.
fn run(out: &mut Vec<Glyph>, text: &str, x: f32, y: f32, size: f32, bold: bool) {
    let mut cx = x;
    for c in text.chars() {
        let w = size * 0.5;
        out.push(Glyph {
            text: c.to_string(),
            x0: cx,
            y0: y,
            x1: cx + w,
            y1: y + size,
            size,
            bold,
            italic: false,
            mono: false,
            generated: false,
            hyphen: false,
            font: String::new(),
            color: [0, 0, 0],
        });
        cx += w;
    }
}

fn page(glyphs: Vec<Glyph>, rules: Vec<Rule>) -> PageContent {
    PageContent {
        width: 600.0,
        height: 800.0,
        glyphs,
        rules,
        ..PageContent::default()
    }
}

fn h(y: f32, x0: f32, x1: f32) -> Rule {
    Rule {
        x0,
        y0: y - 0.25,
        x1,
        y1: y + 0.25,
    }
}

fn v(x: f32, y0: f32, y1: f32) -> Rule {
    Rule {
        x0: x - 0.25,
        y0,
        x1: x + 0.25,
        y1,
    }
}

/// Body text so 10pt is the body size, below y 500.
fn filler(g: &mut Vec<Glyph>) {
    for i in 0..5 {
        run(
            g,
            "some ordinary body text in a paragraph",
            50.0,
            500.0 + 14.0 * i as f32,
            10.0,
            false,
        );
    }
}

fn tables(p: PageContent) -> Vec<Table> {
    layout(&[p])
        .into_iter()
        .filter_map(|b| match b.kind {
            BlockKind::Table(t) => Some(t),
            _ => None,
        })
        .collect()
}

/// Cell texts per row as (col, text, colspan, rowspan).
fn grid(t: &Table) -> Vec<Vec<(usize, String, usize, usize)>> {
    t.rows
        .iter()
        .map(|r| {
            r.iter()
                .map(|c| {
                    let text = c
                        .lines
                        .iter()
                        .map(|l| l.text())
                        .collect::<Vec<_>>()
                        .join(" ");
                    (c.col, text.trim().to_owned(), c.colspan, c.rowspan)
                })
                .collect()
        })
        .collect()
}

/// A 3×3 ruled grid: columns at x 50|150|250|350, rows at y 100|120|140|160.
fn ruled_3x3(skip_v: Option<(usize, usize)>, skip_h: Option<(usize, usize)>) -> Vec<Rule> {
    let xs = [50.0, 150.0, 250.0, 350.0];
    let ys = [100.0, 120.0, 140.0, 160.0];
    let mut r = Vec::new();
    for (i, y) in ys.iter().enumerate() {
        for c in 0..3 {
            if skip_h == Some((i, c)) {
                continue;
            }
            r.push(h(*y, xs[c], xs[c + 1]));
        }
    }
    for (i, x) in xs.iter().enumerate() {
        for row in 0..3 {
            if skip_v == Some((row, i)) {
                continue;
            }
            r.push(v(*x, ys[row], ys[row + 1]));
        }
    }
    r
}

fn cells_3x3(g: &mut Vec<Glyph>, texts: [[&str; 3]; 3], bold_first: bool) {
    for (r, row) in texts.iter().enumerate() {
        for (c, t) in row.iter().enumerate() {
            if !t.is_empty() {
                run(
                    g,
                    t,
                    55.0 + 100.0 * c as f32,
                    104.0 + 20.0 * r as f32,
                    10.0,
                    bold_first && r == 0,
                );
            }
        }
    }
}

#[test]
fn ruled_grid_cells_in_place() {
    let mut g = Vec::new();
    cells_3x3(
        &mut g,
        [
            ["Month", "Level", "Note"],
            ["March", "2.1", "Clear"],
            ["April", "2.9", "Rain"],
        ],
        true,
    );
    filler(&mut g);
    let t = tables(page(g, ruled_3x3(None, None)));
    assert_eq!(t.len(), 1);
    assert_eq!(
        grid(&t[0]),
        vec![
            vec![
                (0, "Month".into(), 1, 1),
                (1, "Level".into(), 1, 1),
                (2, "Note".into(), 1, 1)
            ],
            vec![
                (0, "March".into(), 1, 1),
                (1, "2.1".into(), 1, 1),
                (2, "Clear".into(), 1, 1)
            ],
            vec![
                (0, "April".into(), 1, 1),
                (1, "2.9".into(), 1, 1),
                (2, "Rain".into(), 1, 1)
            ],
        ]
    );
    assert_eq!(t[0].header_rows, 1);
}

#[test]
fn missing_interior_vertical_merges_columns() {
    let mut g = Vec::new();
    // Row 0's line between columns 1 and 2 (x 250) is missing.
    cells_3x3(
        &mut g,
        [
            ["Month", "Spring readings", ""],
            ["March", "2.1", "Clear"],
            ["April", "2.9", "Rain"],
        ],
        false,
    );
    filler(&mut g);
    let t = tables(page(g, ruled_3x3(Some((0, 2)), None)));
    let rows = grid(&t[0]);
    assert_eq!(
        rows[0],
        vec![
            (0, "Month".into(), 1, 1),
            (1, "Spring readings".into(), 2, 1)
        ]
    );
    assert_eq!(rows[1].len(), 3);
}

#[test]
fn missing_interior_horizontal_merges_rows() {
    let mut g = Vec::new();
    // Column 0's line between rows 1 and 2 (y 140) is missing.
    cells_3x3(
        &mut g,
        [
            ["Month", "Level", "Note"],
            ["Spring", "2.1", "Clear"],
            ["", "2.9", "Rain"],
        ],
        false,
    );
    filler(&mut g);
    let t = tables(page(g, ruled_3x3(None, Some((2, 0)))));
    let rows = grid(&t[0]);
    assert_eq!(rows[1][0], (0, "Spring".into(), 1, 2));
    assert_eq!(
        rows[2],
        vec![(1, "2.9".into(), 1, 1), (2, "Rain".into(), 1, 1)]
    );
}

#[test]
fn whitespace_grid_with_numbers_and_a_spanning_header() {
    let mut g = Vec::new();
    // Columns start at x 50, 200, 300; numbers are right-aligned at 260/360.
    run(&mut g, "Region", 50.0, 100.0, 10.0, true);
    run(&mut g, "Spring and summer", 230.0, 100.0, 10.0, true); // spans both number columns
    let rows = [
        ("North bank", "12", "140"),
        ("South bank", "7", "96"),
        ("Mill pool", "104", "3"),
    ];
    for (i, (a, b, c)) in rows.iter().enumerate() {
        let y = 114.0 + 14.0 * i as f32;
        run(&mut g, a, 50.0, y, 10.0, false);
        run(&mut g, b, 260.0 - 5.0 * b.len() as f32, y, 10.0, false);
        run(&mut g, c, 360.0 - 5.0 * c.len() as f32, y, 10.0, false);
    }
    filler(&mut g);
    let t = tables(page(g, vec![]));
    assert_eq!(t.len(), 1, "one table");
    let rows = grid(&t[0]);
    assert_eq!(
        rows[0],
        vec![
            (0, "Region".into(), 1, 1),
            (1, "Spring and summer".into(), 2, 1)
        ]
    );
    assert_eq!(
        rows[1],
        vec![
            (0, "North bank".into(), 1, 1),
            (1, "12".into(), 1, 1),
            (2, "140".into(), 1, 1)
        ]
    );
    assert_eq!(rows.len(), 4);
    assert_eq!(t[0].header_rows, 1);
}

#[test]
fn two_column_prose_is_not_a_table() {
    let mut g = Vec::new();
    for i in 0..8 {
        let y = 100.0 + 14.0 * i as f32;
        run(
            &mut g,
            "left column prose that runs on",
            50.0,
            y,
            10.0,
            false,
        );
        run(
            &mut g,
            "right column prose runs on too",
            320.0,
            y,
            10.0,
            false,
        );
    }
    filler(&mut g);
    assert!(tables(page(g, vec![])).is_empty());
}

#[test]
fn side_by_side_address_blocks_are_not_a_table() {
    let mut g = Vec::new();
    let left = [
        "From",
        "Riverside Survey Group",
        "12 Mill Lane",
        "Valley Town",
    ];
    let right = ["To", "County Archive", "3 Market Square", "Shire Hall"];
    for i in 0..4 {
        let y = 100.0 + 14.0 * i as f32;
        run(&mut g, left[i], 50.0, y, 10.0, i == 0);
        run(&mut g, right[i], 300.0, y, 10.0, i == 0);
    }
    filler(&mut g);
    let p = page(g, vec![]);
    assert!(tables(p.clone()).is_empty());
    let t: Vec<String> = layout(&[p]).iter().map(|b| b.text()).collect();
    assert!(
        t.iter().position(|s| s.contains("Valley Town"))
            < t.iter().position(|s| s.contains("County Archive")),
        "{t:?}"
    );
}

#[test]
fn ruled_box_around_one_paragraph_is_not_a_table() {
    let mut g = Vec::new();
    run(&mut g, "A note inside a box.", 60.0, 110.0, 10.0, false);
    filler(&mut g);
    let rules = vec![
        h(100.0, 50.0, 350.0),
        h(130.0, 50.0, 350.0),
        v(50.0, 100.0, 130.0),
        v(350.0, 100.0, 130.0),
    ];
    assert!(tables(page(g, rules)).is_empty());
}
