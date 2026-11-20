# M6a — PDF reconstruction I Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Turn digital PDFs into structured documents — correct reading order (incl. multi-column), paragraphs, headings, lists, bold/italic, images in place, links — and write them as Word, ODT, Markdown, HTML, EPUB or text through Pandoc, with a fidelity report.

**Architecture:** PDFium (existing worker) extracts, per page, characters in content-stream order with their boxes and font facts, image objects as PNG, and link rectangles (`reader::pdf` → `PageContent`). `fo-core::pdfconv::layout` is pure Rust: characters → line fragments (stream order kept inside a fragment, so complex scripts like Bengali aren't reordered) → recursive XY-cut for columns → blocks → classified (heading by size/weight, list, paragraph) with images placed by position. `pdfconv::ast` turns blocks into a Pandoc JSON AST; `PdfConvert` (`pdf.convert`) feeds it to the existing `PandocEngine` (`from: json`). DOCX direct via docx-rs, tables, footnotes and headers/footers are M6b.

**Tech Stack:** pdfium-render 0.9.5, Pandoc 3.10 (existing engine), headless Chrome + reportlab to build the corpus PDFs from known sources.

**Spec:** §5.2 PDF row, §7.4–7.6; roadmap M6a.

## Global Constraints
- §7.6 bar on the digital corpus: ≥ 99% normalised text similarity, correct reading order, every image present, headings detected. (Tables ≥ 90% of cells is M6b.)
- The tool is labelled best effort (spec §7.4 mock-up); the corpus gate uses ground truth, not the label.
- PDF stays read-only input; encrypted PDFs give the existing password message.
- Pages with no text layer keep their page image and the report says "no text layer, OCR not supported" (§7.5).

## Review Focus
1. Two-column page with a full-width title and a figure: title first, left column, then right column; figure between the right paragraphs.
2. Bengali text (vowel signs drawn before the consonant) keeps logical order.
3. Hyphenated line ends: PDFium-marked hyphens join the word; real hyphens stay.
4. Repeated running text (page numbers) doesn't merge into paragraphs across pages.
5. Huge PDFs: extraction streams page by page; images capped in size.

---

### Task 1: Extraction
**Files:** `crates/fo-core/src/reader/pdf.rs`, `crates/fo-core/src/pdfconv/mod.rs` (types), test `tests/pdf_extract.rs`.
**Produces:** `Glyph{text: String, x0,y0,x1,y1: f32 (pt, top-left origin), size: f32, bold, italic, mono: bool, generated: bool, hyphen: bool}`, `PageImage{x0,y0,x1,y1, png: Vec<u8>}`, `PageLink{x0,y0,x1,y1, uri}`, `PageContent{width, height, glyphs, images, links}`; `PdfEngine::extract(path) -> Result<Vec<PageContent>, String>`; shared `pdf::engine()`.
- [ ] Tests on a pdf-writer fixture: Helvetica-Bold glyphs flagged bold, sizes 24/11, an image XObject at its rect, a URI link annotation.

### Task 2: Layout analysis
**Files:** `crates/fo-core/src/pdfconv/layout.rs`, test `tests/pdf_layout.rs` (synthetic glyphs).
**Produces:** `Block{kind: Heading(u8)|Para|ListItem{ordered: bool}|Image(usize), lines: Vec<Line>}`, `Line{spans: Vec<Span{text, bold, italic, link: Option<String>}>}`, `layout(pages: &[PageContent]) -> Vec<Block>`.
- [ ] Tests: two columns + full-width title order; paragraph split by gap; heading levels by size; bullets "•"/"1." become list items; generated hyphen joins; image between paragraphs; Bengali stream order kept; page-number-only lines dropped when repeated on most pages.

### Task 3: AST and tool
**Files:** `crates/fo-core/src/pdfconv/{ast.rs,tool.rs}`, tests `tests/pdf_convert.rs`.
**Produces:** `to_ast(&[Block], images) -> (Value, Vec<(String, Vec<u8>)>)`; `PdfConvert::new(Arc<dyn PandocEngine>)` id `pdf.convert`, targets docx/odt/md/html/epub/txt, best effort.
- [ ] Tests: fixture PDF → md has heading, bold, link, image file; scanned (image-only) PDF → page images + warning.

### Task 4: Corpus and bar
**Files:** `tests/pdf-corpus/` (HTML sources + Chrome-printed PDFs + a reportlab PDF), `scripts/make-pdf-corpus.sh`, `crates/fo-core/tests/pdf_corpus.rs`, `tests/pdf-corpus/baseline.json`.
- [ ] Ground truth = Pandoc's reading of each source. For each PDF: order-sensitive similarity, image count, heading count vs truth; fail below baseline; print the §7.6 bar status.

### Task 5: App, UI, CI, verification
- [ ] Register in fo-app; mock targets for PDFs; real-app e2e (two-column PDF → DOCX, report); CI unchanged except new tests.
