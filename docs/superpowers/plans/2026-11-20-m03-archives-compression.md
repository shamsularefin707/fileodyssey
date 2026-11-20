# M3 — Archives & Compression Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:**
- Create and extract ZIP, 7z, tar, tar.gz, tar.xz and tar.zst archives safely.
- Make PDFs smaller with three presets.
- Shrink Office files (DOCX/PPTX/XLSX/ODT) and EPUBs by recompressing their images and contents.

Every result gets a fidelity report.

**Architecture:**
- New `fo-core::archive` module (`extract`, `create`, `guard`) and `fo-core::pdfopt`; Office/EPUB handling lives in `fo-core::office`.
- The runner learns two things: placing a **directory** output, and a per-tool output label (`(extracted)`, `(archive)`, `(compressed)`).
- Archive creation is a combining tool (many files → one archive). The UI's combine bar is generalised from "images → PDF" to any combining tool in the current tab.

**Tech Stack:** all pure Rust, no new C. A spike on 2026-11-20 built all of these together:
- `zip` 2.6 (deflate, deflate64, lzma)
- `tar` 0.4 (no xattr)
- `flate2` 1 (rust backend)
- `lzma-rust2` 0.25 (xz encode and decode)
- `ruzstd` 0.9 (zstd)
- `sevenz-rust2` 0.25 (compress + util, no AES)
- `lopdf` 0.47 (no default features)

**Spec:** `docs/superpowers/specs/2026-11-19-fileodyssey-toolkit-design.md` (§6.1, §7, §9) · Roadmap: `docs/superpowers/plans/2026-11-19-roadmap.md`

## Global Constraints

- All M1/M2 constraints still apply: offline, never overwrite, no leftover temp files, untrusted input, clippy/fmt clean, no C deps beyond libwebp.
- **Extraction safety (spec §9):**
  - Only plain relative paths are written. Any entry with an absolute path, `..`, a drive prefix or an empty name is skipped.
  - Links (symlinks, hard links) and special files are skipped.
  - Entry limit: **100,000**.
  - Size limit: actually written bytes may not exceed `min(16 GiB, max(1 GiB, 250 × archive size))`. Exceeding it aborts with "This archive expands to more than N GB, which looks unsafe". Bytes are counted while writing; header sizes aren't trusted.
  - Password-protected entries are skipped (ZIP) or the archive is refused (7z) with "Password-protected archives aren't supported yet".
- **RAR is out.** There is no permissively licensed Rust reader. RAR files offer no tools.
- Output names are `name (extracted)` (a folder), `first-file (archive).zip`, and `name (compressed).pdf|docx|…`.
- Every report says what was skipped or changed. Lossy image recompression is always reported as a warning with its quality.

## Rulings (made at plan time)

- PDF presets use a **pixel cap**, not true DPI. Balanced uses a 2000 px long side at JPEG quality 80; Strong uses 1400 px at quality 60. Lossless only recompresses streams and packs objects. Cost if wrong: an image shown tiny on a page keeps more pixels than a DPI rule would allow.
- PDF images that are masks, image masks, Indexed/CMYK/ICC-with-N≠1/3, JBIG2, CCITT, or not 8-bit are left untouched. Cost: some PDFs shrink less.
- A recompressed image is only swapped in when it is actually smaller.
- If the whole result isn't smaller, the original is saved unchanged with the note "Already compact; saved unchanged".
- ZIP entries using bzip2, zstd, PPMd or AES are skipped with a warning. Cost: rare ZIPs extract partially, and the report says which entries were skipped.

## Review Focus

1. **Zip-slip:** entries like `../../evil` or `/etc/x` must never be written outside the output folder. → Task 2 test.
2. **Zip bombs:** a small archive that expands to gigabytes is aborted, with no partial folder left behind. → Task 2 test (a 1 MiB budget via the test hook).
3. **EPUB/ODT recompression must keep `mimetype` first and stored**, or readers reject the file. → Task 5 test.
4. **Compressed PDFs keep every page and all text**: page count is unchanged and text operators are still present. → Task 4 test.
5. **Extract → re-create round trip keeps exact bytes and names**, including Bengali file names. → Task 3 test.

---

### Task 1: Directory outputs and output labels

**Files:** modify `crates/fo-core/src/{tool.rs,naming.rs,runner.rs}`. Tests go in `tests/naming.rs` and `tests/runner.rs`.

**Interfaces:**
- `Tool::output_label(&self) -> &'static str`. The default is `"combined"` if `combines()`, else `"converted"`.
- `output_path_with(input, out_dir, ext, label)`: an empty `ext` gives `stem (label)` with no dot. For a directory input, the stem is the whole name.
- `place_output_with` also accepts a directory `src`. It claims the destination with `create_dir` (`AlreadyExists` → next counter), moves each child with `rename`, then removes `src`.
- `ToolOutput.files` entries with ext `""` mean "this is a folder".

- [ ] **Step 1: Failing tests:**
  - `empty_ext_names_a_folder` (`a.zip`, `""`, `extracted` → `a (extracted)`)
  - `directory_output_is_placed_under_a_free_name` (a tool returns a folder with a nested file. Pre-create `x (extracted)` so the result lands at `x (extracted 2)` with the nested file intact, and no work dir is left behind)
  - `tool_label_used_in_names`
- [ ] **Steps 2–4:** FAIL → implement → PASS. **Step 5: Commit** `feat(core): folder outputs and per-tool output labels`.

### Task 2: Safe extraction

**Files:** create `crates/fo-core/src/archive/{mod.rs,guard.rs,extract.rs}`. Test: `crates/fo-core/tests/archive_extract.rs`.

**Interfaces:**
- `guard::safe_path(name: &str) -> Option<PathBuf>`: `None` for unsafe names. It normalises `\` to `/` and drops `.` components.
- `guard::Budget::new(archive_len: u64)` with `.limit_bytes()`. `#[doc(hidden)] Budget::with_limit(bytes)` is the test hook.
- `extract::extract(path: &Path, kind: &FileKind, dest: &Path, budget: Budget, cancel: &CancelToken) -> Result<ExtractReport, ToolError>`, where `pub struct ExtractReport { pub files: u64, pub bytes: u64, pub entries: u64, pub skipped_unsafe: u64, pub skipped_links: u64, pub skipped_unsupported: u64 }`. Formats: Zip, SevenZ, Tar, TarGz, TarXz, TarZst.
- Tool `arc.extract` (category Compress, target `folder`, platforms both, accepts the six archive kinds). The report has:
  - metrics `Files` (entries → written) and `Size (KB)`;
  - one warning per skip class: "Skipped N entries with unsafe paths", "Skipped N links", "Skipped N entries that use unsupported compression or a password".

- [ ] **Step 1: Failing tests:**
  - `extract_zip_tar_targz_tarxz_tarzst_7z_roundtrip`: build each format in-test with the same libraries (3 files incl. `sub/রিপোর্ট.txt`), extract, compare bytes.
  - `zip_slip_entries_are_skipped` (zip with `../evil.txt` and `/abs.txt` → neither exists outside dest, and the warning counts 2)
  - `tar_symlink_skipped`
  - `zip_bomb_aborts_and_leaves_nothing` (a 4 MiB zeros entry with `Budget::with_limit(1 MiB)` → `Failed` starting "This archive expands to more than", and the dest has no files)
  - `extract_tool_places_folder_via_runner` (`a (extracted)/…` exists)
- [ ] **Steps 2–4:** FAIL → implement → PASS. **Step 5: Commit** `feat(core): safe archive extraction (zip, 7z, tar.*)`.

### Task 3: Archive creation

**Files:** create `crates/fo-core/src/archive/create.rs`. Test: `crates/fo-core/tests/archive_create.rs`.

**Interfaces:**
- `create::create(inputs: &[PathBuf], format: &str, level: u32, out: &Path, cancel: &CancelToken) -> Result<CreateReport, ToolError>`
  - `format` ∈ `zip 7z tar tar.gz tar.xz tar.zst`.
  - `level` is 1–9, mapped to each codec's range: zstd level 1–3 via ruzstd's available levels.
  - Entry names are file names. A duplicate name becomes `name (2).ext` with the warning "Renamed N files with the same name".
- Tool `arc.create`: combines, label `archive`, category Compress, the targets above, accepts every kind including Unknown. Options: `level` Slider 1–9 default 6.
- The report has metrics `Files` (inputs → entries) and `Size (KB)` (sum of inputs → archive), plus a warning "Archive is larger than the files (they were already compressed)" when that's true.
- Extensions: `tar.gz` and friends produce `.tar.gz`. The runner extension is the target string.

- [ ] **Step 1: Failing tests:**
  - `create_every_format_then_extract_matches`, including a Bengali name and a 1 MiB random file
  - `duplicate_names_are_renamed`
  - `create_tool_names_archive_after_first_file` (`a (archive).zip`)
- [ ] **Steps 2–4:** FAIL → implement → PASS. **Step 5: Commit** `feat(core): create zip, 7z and tar archives`.

### Task 4: PDF compression

**Files:** create `crates/fo-core/src/pdfopt.rs`. Test: `crates/fo-core/tests/pdf_compress.rs` (fixtures are built with `pdf-writer` and `image::pdf::write_pdf` from M2).

**Interfaces:**
- `pdfopt::compress(input: &Path, preset: Preset, out: &Path) -> Result<FidelityReport, ToolError>`, with `pub enum Preset { Lossless, Balanced, Strong }`.
- Tool `pdf.compress`: Compress category, accepts Pdf, target `pdf`, label `compressed`. Option `level` Choice `lossless`/`balanced`/`strong`, default `balanced`.
- The report has metrics `Pages` (before → after, by reloading the output), `File size (KB)` and `Images re-encoded` (0 → n). Warnings: "N images re-encoded as JPEG at quality Q", "N images scaled down to at most P px".
- An encrypted PDF fails with "Password-protected PDFs aren't supported yet".

- [ ] **Step 1: Failing tests:**
  - `balanced_shrinks_big_flate_image_keeps_pages` (a 3000×2000 noise image PDF → smaller, 1 page, warning present)
  - `lossless_keeps_images_and_text_exact` (a PDF with a text content stream `(Hello FileOdyssey) Tj` and an uncompressed content stream → output content still contains `Hello FileOdyssey`, images untouched, verdict Exact)
  - `masks_are_not_jpeg_encoded` (an RGBA image PDF → the SMask stream keeps `FlateDecode`)
  - `not_smaller_saves_original` (an already-compressed tiny PDF → output bytes equal the input, with the note)
- [ ] **Steps 2–4:** FAIL → implement → PASS. **Step 5: Commit** `feat(core): PDF compression presets`.

### Task 5: Office and EPUB compression

**Files:** create `crates/fo-core/src/office.rs`. Test: `crates/fo-core/tests/office_compress.rs`.

**Interfaces:**
- `office::compress(input: &Path, preset: Preset, out: &Path) -> Result<FidelityReport, ToolError>`, reusing `pdfopt::Preset`.
  - Entries are rewritten in their original order with Deflate level 9.
  - `mimetype` stays first and Stored.
  - PNG images (`*.png` anywhere) go through oxipng (lossless) in every preset. JPEGs are re-encoded only for Balanced/Strong, and only when smaller.
  - Every other entry's bytes are copied unchanged.
- Tool `office.compress`: Compress category, accepts Docx/Pptx/Xlsx/Odt/Epub, target `same` (resolved to the source extension), label `compressed`. Option `level` as for PDF.
- The report has metrics `Files inside` (equal before and after), `File size (KB)`, `Images re-encoded`, plus warnings like the PDF ones.

- [ ] **Step 1: Failing tests:**
  - `docx_images_recompressed_entries_preserved` (all names are the same and in the same order, non-image bytes identical, smaller)
  - `epub_mimetype_first_and_stored`
  - `lossless_never_touches_jpeg`
- [ ] **Steps 2–4:** FAIL → implement → PASS. **Step 5: Commit** `feat(core): Office and EPUB compression`.

### Task 6: Registration and UI

**Files:**
- Modify `crates/fo-app/src/state.rs`: register the archive, PDF and office tools, with a test.
- Generalise `crates/fo-ui/src/components/combine.rs` to every combining tool in the current tab: a title per tool, a format select from `targets`, and that tool's other options via the M2 `OptionsPanel` controls (made `pub(crate)`).
- Update `strings.rs` and `mock.rs` (mirror the new tools). In `fo-ui/src/state.rs`, add `pub fn combine_candidates(entries, tab) -> Vec<(ToolInfo, Vec<String>)>`.
- Show "folder" targets as "Folder" in the menu.

- [ ] **Step 1: Failing tests:**
  - `combine_candidates_lists_archive_for_any_file_on_compress_tab` (works with 1 file)
  - `combine_candidates_needs_two_images_for_pdf`
  - `app_state_registers_m3_tools`
- [ ] **Steps 2–4:** FAIL → implement → PASS. Then trunk build and screenshots of the Compress tab: files listed, the archive bar, and options for PDF compress. Light and dark, desktop and 390px.
- [ ] **Step 5: Commit** `feat(ui): archive bar and compression tools in the Compress tab`.

### Task 7: Verify, review, PR, merge

- [ ] fmt, clippy (native and wasm), `cargo test --workspace`.
- [ ] Drive the release build with xdotool: on the Compress tab, choose 2 files → create a ZIP → then extract it. Confirm the folder exists.
- [ ] Final review by a fresh reviewer, then a fix pass.
- [ ] Push, open the PR, wait until all four CI jobs are green, then merge.
