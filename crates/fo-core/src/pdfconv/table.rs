//! Tables: ruled grids (from drawn lines) and whitespace grids (from text
//! aligned in columns), with merged cells.

use super::layout::{join_same_baseline, Line};
use super::{PageContent, Rule};

/// One cell. Rows list only the cells that start in them (like HTML): a
/// cell spanning down is not repeated in the rows below.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    /// First grid column the cell covers.
    pub col: usize,
    pub colspan: usize,
    pub rowspan: usize,
    pub lines: Vec<Line>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub rows: Vec<Vec<Cell>>,
    /// Number of grid columns.
    pub cols: usize,
    /// Leading rows that are the header.
    pub header_rows: usize,
}

pub type BBox = [f32; 4];

fn center(l: &Line) -> (f32, f32) {
    ((l.x0 + l.x1) / 2.0, (l.y0 + l.y1) / 2.0)
}

/// Leading rows whose text is all bold, while some row below isn't.
fn header_rows(rows: &[Vec<Cell>]) -> usize {
    let bold = |r: &Vec<Cell>| {
        let mut any = false;
        for c in r {
            for l in &c.lines {
                any = true;
                if !l.bold {
                    return false;
                }
            }
        }
        any
    };
    let n = rows.iter().take_while(|r| bold(r)).count();
    if n < rows.len() {
        n.min(3)
    } else {
        0
    }
}

// ------------------------------------------------------------------ ruled

#[derive(Debug, Clone, Copy)]
struct Seg {
    /// y of a horizontal segment, x of a vertical one.
    at: f32,
    lo: f32,
    hi: f32,
}

fn merge_segs(mut v: Vec<Seg>) -> Vec<Seg> {
    v.sort_by(|a, b| a.at.total_cmp(&b.at).then(a.lo.total_cmp(&b.lo)));
    let mut out: Vec<Seg> = Vec::new();
    for s in v {
        if let Some(last) = out.last_mut() {
            if (s.at - last.at).abs() < 1.0 && s.lo <= last.hi + 1.5 {
                last.hi = last.hi.max(s.hi);
                continue;
            }
        }
        out.push(s);
    }
    out
}

/// Values within `tol` of each other, averaged.
fn cluster(mut v: Vec<f32>, tol: f32) -> Vec<f32> {
    v.sort_by(f32::total_cmp);
    let mut out: Vec<(f32, usize)> = Vec::new();
    for x in v {
        match out.last_mut() {
            Some((m, n)) if x - *m / *n as f32 <= tol => {
                *m += x;
                *n += 1;
            }
            _ => out.push((x, 1)),
        }
    }
    out.into_iter().map(|(m, n)| m / n as f32).collect()
}

/// How much of [lo, hi] the segments near `at` cover.
fn covered(segs: &[Seg], at: f32, lo: f32, hi: f32) -> bool {
    let len: f32 = segs
        .iter()
        .filter(|s| (s.at - at).abs() < 2.0)
        .map(|s| (s.hi.min(hi) - s.lo.max(lo)).max(0.0))
        .sum();
    len >= 0.5 * (hi - lo)
}

fn find(p: &mut [usize], i: usize) -> usize {
    let mut i = i;
    while p[i] != i {
        p[i] = p[p[i]];
        i = p[i];
    }
    i
}

fn ruled(rules: &[Rule], lines: &mut Vec<Line>) -> Vec<(BBox, Table)> {
    let hs = merge_segs(
        rules
            .iter()
            .filter(|r| r.horizontal())
            .map(|r| Seg {
                at: (r.y0 + r.y1) / 2.0,
                lo: r.x0,
                hi: r.x1,
            })
            .collect(),
    );
    let vs = merge_segs(
        rules
            .iter()
            .filter(|r| !r.horizontal())
            .map(|r| Seg {
                at: (r.x0 + r.x1) / 2.0,
                lo: r.y0,
                hi: r.y1,
            })
            .collect(),
    );
    // Connected groups of crossing or touching lines.
    let n = hs.len() + vs.len();
    let mut parent: Vec<usize> = (0..n).collect();
    for (i, h) in hs.iter().enumerate() {
        for (j, v) in vs.iter().enumerate() {
            let t = 2.0;
            if v.at >= h.lo - t && v.at <= h.hi + t && h.at >= v.lo - t && h.at <= v.hi + t {
                let (a, b) = (find(&mut parent, i), find(&mut parent, hs.len() + j));
                parent[a] = b;
            }
        }
    }
    let mut groups: std::collections::BTreeMap<usize, (Vec<Seg>, Vec<Seg>)> = Default::default();
    for i in 0..n {
        let g = groups.entry(find(&mut parent, i)).or_default();
        if i < hs.len() {
            g.0.push(hs[i]);
        } else {
            g.1.push(vs[i - hs.len()]);
        }
    }
    let mut out = Vec::new();
    for (gh, gv) in groups.into_values() {
        if gh.len() < 2 || gv.len() < 2 {
            continue;
        }
        let ys = cluster(gh.iter().map(|s| s.at).collect(), 3.0);
        let xs = cluster(gv.iter().map(|s| s.at).collect(), 3.0);
        let (nr, nc) = (ys.len().saturating_sub(1), xs.len().saturating_sub(1));
        if nr * nc < 2 {
            continue;
        }
        // Merge cells across missing interior lines.
        let id = |r: usize, c: usize| r * nc + c;
        let mut p: Vec<usize> = (0..nr * nc).collect();
        for r in 0..nr {
            for c in 0..nc {
                if c + 1 < nc && !covered(&gv, xs[c + 1], ys[r], ys[r + 1]) {
                    let (a, b) = (find(&mut p, id(r, c)), find(&mut p, id(r, c + 1)));
                    p[a] = b;
                }
                if r + 1 < nr && !covered(&gh, ys[r + 1], xs[c], xs[c + 1]) {
                    let (a, b) = (find(&mut p, id(r, c)), find(&mut p, id(r + 1, c)));
                    p[a] = b;
                }
            }
        }
        // Each group's extent; its top-left position holds the cell.
        let mut ext: std::collections::HashMap<usize, (usize, usize, usize, usize)> =
            Default::default();
        for r in 0..nr {
            for c in 0..nc {
                let e = ext.entry(find(&mut p, id(r, c))).or_insert((r, c, r, c));
                e.0 = e.0.min(r);
                e.1 = e.1.min(c);
                e.2 = e.2.max(r);
                e.3 = e.3.max(c);
            }
        }
        if ext.len() < 2 {
            continue;
        }
        let bbox = [xs[0], ys[0], xs[nc], ys[nr]];
        let mut cell_lines: std::collections::HashMap<usize, Vec<Line>> = Default::default();
        let mut keep = Vec::new();
        for l in lines.drain(..) {
            let (cx, cy) = center(&l);
            let r = (0..nr).find(|&r| cy >= ys[r] && cy <= ys[r + 1]);
            let c = (0..nc).find(|&c| cx >= xs[c] && cx <= xs[c + 1]);
            match (r, c) {
                (Some(r), Some(c)) => cell_lines
                    .entry(find(&mut p, id(r, c)))
                    .or_default()
                    .push(l),
                _ => keep.push(l),
            }
        }
        *lines = keep;
        let mut rows: Vec<Vec<Cell>> = vec![Vec::new(); nr];
        let mut starts: Vec<(usize, (usize, usize, usize, usize))> = ext.into_iter().collect();
        starts.sort_by_key(|(_, e)| (e.0, e.1));
        for (g, (r0, c0, r1, c1)) in starts {
            rows[r0].push(Cell {
                col: c0,
                colspan: c1 - c0 + 1,
                rowspan: r1 - r0 + 1,
                lines: join_same_baseline(cell_lines.remove(&g).unwrap_or_default()),
            });
        }
        let header_rows = header_rows(&rows);
        out.push((
            bbox,
            Table {
                rows,
                cols: nc,
                header_rows,
            },
        ));
    }
    out
}

// ------------------------------------------------------------- whitespace

fn numeric(t: &str) -> bool {
    let t = t.trim();
    !t.is_empty()
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars()
            .all(|c| c.is_ascii_digit() || " .,%$€£৳+-–()".contains(c))
}

/// Gaps between merged [lo, hi] intervals, as (size, middle).
fn x_gaps(mut iv: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
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

/// Runs of at least three rows whose fragments line up in columns.
fn whitespace(lines: &mut Vec<Line>, body: f32) -> Vec<(BBox, Table)> {
    lines.sort_by(|a, b| a.y0.total_cmp(&b.y0).then(a.x0.total_cmp(&b.x0)));
    // Rows: fragments sharing a band.
    let mut rows: Vec<Vec<usize>> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let joins = rows.last().is_some_and(|r| {
            let f = &lines[r[0]];
            let overlap = f.y1.min(l.y1) - f.y0.max(l.y0);
            overlap > 0.5 * (f.y1 - f.y0).min(l.y1 - l.y0)
        });
        if joins {
            rows.last_mut().expect("non-empty").push(i);
        } else {
            rows.push(vec![i]);
        }
    }
    for r in &mut rows {
        r.sort_by(|a, b| lines[*a].x0.total_cmp(&lines[*b].x0));
    }
    let band = |r: &Vec<usize>| {
        let y0 = r.iter().map(|&i| lines[i].y0).fold(f32::MAX, f32::min);
        let y1 = r.iter().map(|&i| lines[i].y1).fold(f32::MIN, f32::max);
        (y0, y1)
    };
    let mut used = vec![false; lines.len()];
    let mut out = Vec::new();
    let mut i = 0;
    while i < rows.len() {
        let mut j = i;
        while j < rows.len()
            && rows[j].len() >= 2
            && (j == i || band(&rows[j]).0 - band(&rows[j - 1]).1 < 1.2 * body)
        {
            j += 1;
        }
        if j - i >= 3 {
            if let Some(t) = grid_of(lines, &rows[i..j], body) {
                for r in &rows[i..j] {
                    for &k in r {
                        used[k] = true;
                    }
                }
                out.push(t);
            }
        }
        i = j.max(i + 1);
    }
    let mut k = 0;
    lines.retain(|_| {
        k += 1;
        !used[k - 1]
    });
    out
}

fn grid_of(lines: &[Line], rows: &[Vec<usize>], body: f32) -> Option<(BBox, Table)> {
    let kmax = rows.iter().map(Vec::len).max()?;
    let full: Vec<&Vec<usize>> = rows.iter().filter(|r| r.len() == kmax).collect();
    if full.len() < 2 {
        return None;
    }
    let cuts: Vec<f32> = x_gaps(
        full.iter()
            .flat_map(|r| r.iter().map(|&i| (lines[i].x0, lines[i].x1)))
            .collect(),
    )
    .into_iter()
    .filter(|(g, _)| *g >= 0.8 * body)
    .map(|(_, at)| at)
    .collect();
    if cuts.len() + 1 != kmax {
        return None;
    }
    let texts: Vec<String> = rows.iter().flatten().map(|&i| lines[i].text()).collect();
    // Prose in columns has long lines; table cells are short.
    let mut lens: Vec<usize> = texts.iter().map(|t| t.trim().chars().count()).collect();
    lens.sort_unstable();
    if lens[lens.len() / 2] > 40 {
        return None;
    }
    if kmax == 2 {
        let num = full.iter().filter(|r| numeric(&lines[r[1]].text())).count();
        if num * 5 < full.len() * 3 {
            return None;
        }
    }
    let col_of = |x: f32| cuts.iter().filter(|c| x > **c).count();
    // Each column's content extent in the full rows. A fragment belongs to
    // the columns whose content it overlaps (a header over right-aligned
    // numbers can start left of the gap's middle); one in a gap goes by
    // the gap's middle.
    let extent: Vec<(f32, f32)> = (0..kmax)
        .map(|k| {
            full.iter()
                .map(|r| &lines[r[k]])
                .fold((f32::MAX, f32::MIN), |(lo, hi), l| {
                    (lo.min(l.x0), hi.max(l.x1))
                })
        })
        .collect();
    let cols_of = |l: &Line| {
        let over: Vec<usize> = (0..kmax)
            .filter(|&k| l.x1 > extent[k].0 + 1.0 && l.x0 < extent[k].1 - 1.0)
            .collect();
        // It starts at the first column it overlaps, and ends at the last
        // column it overlaps or reaches past a gap's middle into.
        let reach = col_of(l.x1 - 1.0);
        match over.first() {
            Some(&a) => (a, over.last().copied().unwrap_or(a).max(reach).max(a)),
            None => (col_of(l.x0 + 1.0), reach),
        }
    };
    let mut table_rows = Vec::new();
    for r in rows {
        let mut cells: Vec<Cell> = Vec::new();
        for &i in r {
            let l = &lines[i];
            let (c0, c1) = cols_of(l);
            match cells.last_mut() {
                // Two fragments in one column are one cell.
                Some(last) if last.col + last.colspan > c0 => {
                    last.colspan = (c1 + 1).max(last.col + last.colspan) - last.col;
                    last.lines.push(l.clone());
                }
                _ => cells.push(Cell {
                    col: c0,
                    colspan: c1 - c0 + 1,
                    rowspan: 1,
                    lines: vec![l.clone()],
                }),
            }
        }
        // Empty positions become empty cells.
        let mut filled = Vec::new();
        let mut at = 0;
        for c in cells {
            while at < c.col {
                filled.push(Cell {
                    col: at,
                    colspan: 1,
                    rowspan: 1,
                    lines: vec![],
                });
                at += 1;
            }
            at = c.col + c.colspan;
            filled.push(c);
        }
        while at < kmax {
            filled.push(Cell {
                col: at,
                colspan: 1,
                rowspan: 1,
                lines: vec![],
            });
            at += 1;
        }
        for c in &mut filled {
            c.lines = join_same_baseline(std::mem::take(&mut c.lines));
        }
        table_rows.push(filled);
    }
    let all = rows.iter().flatten().map(|&i| &lines[i]);
    let bbox = all.fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, l| {
        [
            b[0].min(l.x0),
            b[1].min(l.y0),
            b[2].max(l.x1),
            b[3].max(l.y1),
        ]
    });
    let header_rows = header_rows(&table_rows);
    Some((
        bbox,
        Table {
            rows: table_rows,
            cols: kmax,
            header_rows,
        },
    ))
}

/// Tables on one page; the lines they hold are taken out of `lines`.
pub fn find_tables(page: &PageContent, lines: &mut Vec<Line>, body: f32) -> Vec<(BBox, Table)> {
    let mut out = ruled(&page.rules, lines);
    out.extend(whitespace(lines, body));
    out
}
