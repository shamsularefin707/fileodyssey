//! Footnotes: small lines at the bottom of a page that start with a marker
//! matching a superscript in the page's text.

use super::layout::{Line, Script};

/// A note marker at the start of a line: its label and the byte length to
/// strip (marker and following space).
fn marker(l: &Line) -> Option<(String, usize)> {
    let first = l.spans.first()?;
    if first.script == Script::Super && !first.text.trim().is_empty() {
        let label = first.text.trim().to_owned();
        return Some((label, first.text.len()));
    }
    let t = l.text();
    let lead = t.len() - t.trim_start().len();
    let s = &t[lead..];
    let n = s
        .char_indices()
        .take_while(|(_, c)| c.is_ascii_digit() || "*†‡§¶".contains(*c))
        .map(|(i, c)| i + c.len_utf8())
        .last()?;
    let label = &s[..n];
    if label.chars().filter(char::is_ascii_digit).count() > 3 {
        return None;
    }
    let rest = &s[n..];
    let punct = rest.starts_with(['.', ')']) as usize;
    rest[punct..]
        .starts_with(' ')
        .then(|| (label.to_owned(), lead + n + punct + 1))
}

fn strip(l: &mut Line, len: usize) {
    let mut left = len;
    for s in &mut l.spans {
        if left == 0 {
            break;
        }
        let cut = (0..=left.min(s.text.len()))
            .rev()
            .find(|i| s.text.is_char_boundary(*i))
            .unwrap_or(0);
        s.text.drain(..cut);
        left -= cut;
    }
    l.spans.retain(|s| !s.text.is_empty());
    if let Some(s) = l.spans.first_mut() {
        s.text = s.text.trim_start().to_owned();
        s.script = Script::Normal;
    }
}

/// Moves the page's footnotes out of `lines` into `notes`, linking each to
/// the superscript that marks it (the span gets `note`). Notes whose
/// marker isn't found stay in the text.
pub fn split_notes(page_h: f32, lines: &mut Vec<Line>, body: f32, notes: &mut Vec<Vec<Line>>) {
    lines.sort_by(|a, b| a.y0.total_cmp(&b.y0).then(a.x0.total_cmp(&b.x0)));
    // The run of small lines at the bottom of the page.
    let mut start = lines.len();
    while start > 0 {
        let l = &lines[start - 1];
        if l.size <= 0.92 * body && l.y0 > 0.5 * page_h {
            start -= 1;
        } else {
            break;
        }
    }
    if start == lines.len() || marker(&lines[start]).is_none() {
        return;
    }
    // Group: a marked line starts a note, others continue it.
    let mut found: Vec<(String, Vec<usize>)> = Vec::new();
    for (i, l) in lines.iter().enumerate().skip(start) {
        match marker(l) {
            Some((label, _)) => found.push((label, vec![i])),
            None => found.last_mut().expect("first is marked").1.push(i),
        }
    }
    let mut taken = vec![false; lines.len()];
    for (label, idx) in found {
        // The marker in the text: a superscript with the same label, above.
        let mut at = None;
        'find: for (li, l) in lines[..start].iter().enumerate() {
            for (si, s) in l.spans.iter().enumerate() {
                if s.script == Script::Super && s.note.is_none() && s.text.trim() == label {
                    at = Some((li, si));
                    break 'find;
                }
            }
        }
        let Some((li, si)) = at else { continue };
        let mut note: Vec<Line> = idx.iter().map(|&i| lines[i].clone()).collect();
        if let Some((_, len)) = marker(&note[0]) {
            strip(&mut note[0], len);
        }
        lines[li].spans[si].note = Some(notes.len());
        notes.push(note);
        for i in idx {
            taken[i] = true;
        }
    }
    let mut k = 0;
    lines.retain(|_| {
        k += 1;
        !taken[k - 1]
    });
}
