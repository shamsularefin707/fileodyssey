//! Developer aid: print the stream-order text and the reconstructed notes.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let pages = fo_core::reader::pdf::engine()
        .unwrap()
        .extract(std::path::Path::new(&a[1]))
        .unwrap();
    let doc = fo_core::pdfconv::layout::reconstruct(&pages);
    println!("{}", doc.stream_text);
    println!(
        "header {:?} footer {:?} page numbers {}",
        doc.header, doc.footer, doc.page_numbers
    );
    for n in &doc.notes {
        println!("note: {:?}", n.iter().map(|l| l.text()).collect::<Vec<_>>());
    }
}
