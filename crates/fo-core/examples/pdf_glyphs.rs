//! Developer aid: dump extracted glyphs whose text contains a pattern.
//! `cargo run -p fo-core --example pdf_glyphs -- file.pdf [from] [count]`
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let pages = fo_core::reader::pdf::engine()
        .unwrap()
        .extract(std::path::Path::new(&a[1]))
        .unwrap();
    let from: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let n: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(40);
    println!(
        "spaces: {:?}",
        &pages[0].spaces[..pages[0].spaces.len().min(6)]
    );
    for (i, g) in pages[0].glyphs.iter().enumerate().skip(from).take(n) {
        println!(
            "{i:4} {:?} x {:.1}-{:.1} y {:.1}-{:.1} size {:.1} gen {} bold {}",
            g.text, g.x0, g.x1, g.y0, g.y1, g.size, g.generated, g.bold
        );
    }
}
