# M6b — PDF reconstruction II Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Finish PDF → document reconstruction to the spec §7.6 bar: tables (ruled and whitespace grids, merged cells), footnotes, running headers/footers kept separate, super/subscripts, and DOCX written directly with fonts, sizes and colours.

**Architecture:** Extraction gains ruling segments (from path objects) and per-glyph font family and fill colour. `layout` returns a `Doc { blocks, notes, header, footer }`: before XY-cut, each page's table regions (ruled grids from rulings, whitespace grids from aligned fragments) become one `Item::Table`; page-bottom footnotes are split off and linked to superscript markers in the body; repeated top/bottom margin text becomes the running header/footer instead of being dropped. Spans carry font, size, colour and script. Two writers read the same `Doc`: `ast` (Pandoc JSON → ODT/MD/HTML/EPUB/TXT, with Table spans, Note, Superscript) and `docx` (docx-rs: heading styles, numbering, tables with gridSpan/vMerge, footnotes, header/footer with PAGE field, images, links, run fonts/sizes/colours). Both are checked by reading the output back with Pandoc.

**Tech Stack:** pdfium-render 0.9.5, docx-rs 0.4.22 (MIT), Pandoc 3.10, Chrome + reportlab for the corpus, LibreOffice for the by-eye check.

**Spec:** §5.2 PDF row, §7.1, §7.2, §7.5, §7.6; roadmap M6b.

## Global Constraints
- §7.6 on every digital corpus file, for both the DOCX and Markdown outputs: ≥ 99% normalised text similarity, reading order, every image, headings detected, ≥ 90% of table cells in the right row and column.
- DOCX keeps fonts, sizes and colours (§5.2); other targets go through Pandoc.
- Headers/footers are detected and kept separate: in DOCX they are the page header/footer; other targets leave them out and the report says so.
- The fidelity report counts tables and footnotes for PDFs now (§7.1); a drop marks ⚠.
- pdf.convert stays labelled best effort; the corpus gate is ground truth plus a stored baseline.

## Review Focus
1. Two-column prose must never become a two-column "table" (whitespace grids need ≥ 3 aligned columns, or 2 where one is mostly numeric).
2. Side-by-side address blocks (invoice From/To) stay paragraphs in reading order.
3. A superscript that isn't a note marker (x², 1st) stays a superscript, not a broken note.
4. A ruled box around a single paragraph (a callout) isn't a 1×1 table that loses its paragraph breaks — 1×1 grids are ignored.
5. A table split across a page break stays two tables, each correct; no text is dropped.

---

### Task 1: Extraction II — rulings, fonts, colours
**Files:** `crates/fo-core/src/pdfconv/mod.rs`, `crates/fo-core/src/reader/pdf.rs`, `tests/common/pdf_fixture.rs`, `tests/pdf_extract.rs`.
**Produces:** `Glyph { .., font: String /* family, "" if unknown */, color: [u8; 3] }`; `Rule { x0, y0, x1, y1 }` (axis-aligned, top-left origin, a thin box), `PageContent.rules: Vec<Rule>`.
- [ ] Test (fixture `ruled_pdf()`: a 2×2 stroked grid, a filled thin rect, red Times text): 3 horizontal + 3 vertical rules found (stroked rect edges split into segments), glyph colour `[255,0,0]`, font family contains "Times".
- [ ] Implement: path objects → segments (after the object's matrix) that are axis-aligned and long, plus filled rects thinner than 2pt; fill colour from the char; family from the char's font (subset prefix stripped).

### Task 2: Spans with style and script
**Files:** `pdfconv/layout.rs`, `pdfconv/ast.rs`, `tests/pdf_layout.rs`.
**Produces:** `Span { .., font: String, size: f32, color: [u8; 3], script: Script }`, `enum Script { Normal, Super, Sub }`.
- [ ] Tests: a smaller raised glyph run inside a line is `Super` and stays at its place ("water.¹" order); AST emits `Superscript`.
- [ ] Implement: spans split on font/size/colour/script change; script by glyph bottom vs line bottom and size ratio.

### Task 3: Tables
**Files:** `pdfconv/table.rs` (new), `pdfconv/layout.rs`, `tests/pdf_table.rs`.
**Produces:** `Table { rows: Vec<Vec<Cell>>, header_rows: usize }`, `Cell { lines: Vec<Line>, colspan: usize, rowspan: usize }` (covered positions hold no `Cell`), `BlockKind::Table(Table)`; `fn find_tables(page, frags, body) -> Vec<(bbox, Table)>` taking the fragments it uses.
- [ ] Tests: ruled 3×3 with text → cells in place; a missing interior vertical → colspan 2; missing interior horizontal → rowspan 2; whitespace grid 4 rows × 3 columns with right-aligned numbers → table; a spanning header fragment → colspan; two-column prose → no table; two side-by-side 4-line blocks → no table; 1×1 ruled box → no table.
- [ ] Implement: rulings clustered into grids (connected segments, ≥ 2×2 cells); cell edges present/absent decide spans; whitespace grids from runs of ≥ 3 rows with consistent column gaps; header rows = leading rows that are all bold (or the first row when ruled with a thicker rule / bold).

### Task 4: Footnotes, headers and footers
**Files:** `pdfconv/layout.rs`, `pdfconv/notes.rs` (new), `tests/pdf_layout.rs`.
**Produces:** `pub struct Doc { blocks: Vec<Block>, notes: Vec<Vec<Line>>, header: Option<Vec<Span>>, footer: Option<Vec<Span>> , page_numbers: bool }`; `layout(pages) -> Doc`; `Span.note: Option<usize>` (a reference into `notes`).
- [ ] Tests: bottom-of-page small lines "1 Text…" with a body superscript "1" → one note linked at the marker; a small bottom line with no matching marker stays a paragraph; repeated top-margin text on ≥ 2 pages → header, removed from body; "Page N" bottom → footer page numbers.
- [ ] Implement.

### Task 5: Pandoc AST for the new structure
**Files:** `pdfconv/ast.rs`, `pdfconv/tool.rs`, `tests/pdf_convert.rs`.
- [ ] Tests: Table with spans → Pandoc `Table` (cells with row/col span), notes → `Note`; report "before" counts tables, cells and footnotes and the md result keeps them; header/footer left-out warning.
- [ ] Implement.

### Task 6: Direct DOCX
**Files:** `pdfconv/docx.rs` (new), `pdfconv/tool.rs`, `pandoc/tool.rs` (split `write_and_check` into write + `check_output`), `Cargo.toml` (docx-rs).
- [ ] Tests: DOCX from a Doc with heading, list, table with colspan, note, image, link, coloured Times run, header text and page-number footer; read back by Pandoc: same words, 1 table, 1 note, 1 image, 1 link, heading; `word/document.xml` has the font, `w:color`, `w:sz`; `word/header1.xml` has the header text.
- [ ] Implement; pdf.convert routes docx here.

### Task 7: Corpus to the bar
**Files:** `tests/pdf-corpus/src/*` (new: `grid.html` borderless tables with merged cells, `notes_reportlab.py` gains `footnotes.pdf` with page-bottom notes and running header/footer), `tests/pdf_corpus.rs`, `baseline.json`.
- [ ] Corpus test runs md and docx, adds table-cell score (truth tables from the source AST, grids expanded by spans) and footnote count; §7.6 bar includes cells ≥ 90%.
- [ ] Iterate until every file meets the bar on both outputs; LibreOffice renders of the DOCX checked by eye.

### Task 8: Ship
- [ ] fmt, clippy (native + wasm), workspace tests; release app e2e (report.pdf → DOCX under Xvfb); PR, CI green, merge.
