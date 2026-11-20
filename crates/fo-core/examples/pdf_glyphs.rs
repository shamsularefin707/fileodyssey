//! Developer aid: dump a page's extracted rules and glyphs.
//! `cargo run -p fo-core --example pdf_glyphs -- file.pdf [from] [count] [page]`
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let pages = fo_core::reader::pdf::engine()
        .unwrap()
        .extract(std::path::Path::new(&a[1]))
        .unwrap();
    let from: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let n: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(40);
    let p = &pages[a.get(4).and_then(|s| s.parse().ok()).unwrap_or(0)];
    println!("{} rules, {} spaces", p.rules.len(), p.spaces.len());
    for r in &p.rules {
        println!("  rule x {:.1}-{:.1} y {:.1}-{:.1}", r.x0, r.x1, r.y0, r.y1);
    }
    for (i, g) in p.glyphs.iter().enumerate().skip(from).take(n) {
        println!(
            "{i:4} {:?} x {:.1}-{:.1} y {:.1}-{:.1} size {:.1} gen {} bold {} font {:?} color {:?}",
            g.text, g.x0, g.x1, g.y0, g.y1, g.size, g.generated, g.bold, g.font, g.color
        );
    }
}
