# Golden corpus

Sources for `crates/fo-core/tests/corpus.rs`, which converts each file to
every format it offers and compares the fidelity of each pair with
`baseline.json` (similarity and element counts must not drop; a pair that
was exact must stay exact). After an intended change, regenerate with
`FO_UPDATE_BASELINE=1 cargo test -p fo-core --test corpus` and review the diff.

Where the files come from:

- `notes.md`, `article.html`, `guide.rst`, `todo.org`, `paper.tex`,
  `manual.adoc`, `memo.rtf`, `fieldbook.txt` — written by hand, with English
  and Bengali text, tables, links, lists, footnotes and an image.
- `report.docx` — made with python-docx 1.2 (not Pandoc), with a title,
  headings, a table, a hyperlink, an image and bullets.
- `letter.odt` (from `notes.md`) and `book.epub` (from `article.html`) — made
  with Pandoc 3.10.
- `pics/river.png` — a 48×24 gradient.

Similarity is measured on words read back by Pandoc. Pandoc's RTF, LaTeX,
AsciiDoc, ODT, Org and reStructuredText *readers* miss some titles, captions,
table cells and notes that its writers do produce, so pairs into those
formats can score lower than the files themselves deserve; the app's report
says so on those results.
