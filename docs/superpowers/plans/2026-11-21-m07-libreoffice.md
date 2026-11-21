# M7 — LibreOffice integration (desktop) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** On desktop, when LibreOffice is installed, convert legacy Office files (.doc/.xls/.ppt) and Office/ODF files to modern formats and to PDF with exact layout, and give Pandoc formats a PDF target; when it isn't, show those targets as "Install LibreOffice to enable" with a link.

**Architecture:** `fo-core/src/libre/mod.rs` finds `soffice` (env override, PATH, standard install folders) and runs `soffice --headless --convert-to` on a copy of the input in the job's work folder, with its own throwaway user profile, a time limit and cancel. `libre/tool.rs` is the `office.convert` tool (desktop only): targets depend on the source kind; Pandoc sources reach PDF through Pandoc → DOCX → LibreOffice. Its report reads the source with Pandoc where Pandoc can (DOCX/ODT/RTF/PPTX/XLSX), else LibreOffice's own modern-format copy (and says so); PDF results are read with PDFium and scored by word recall (layout reorders running text and list numbers). `ToolInfo` gains `needs: Option<Needs>`; the UI shows such targets disabled with the reason and a link that opens only the LibreOffice download page.

**Tech Stack:** LibreOffice (optional, user-installed), Pandoc 3.10, PDFium.

**Spec:** §2 table (LibreOffice row), §5.2 LibreOffice row, §6.3, §7.1, §7.4, §9 (temp files, untrusted input), §11 step 7.

## Global Constraints
- LibreOffice stays optional; nothing is bundled. Missing → "Install LibreOffice to enable" with a link (§5.2).
- Desktop only (§2): the tool's platforms are `[Desktop]`.
- No temp files left behind: everything (input copy, profile, outputs) lives in the job's work folder (§9).
- Every conversion gets a fidelity report (§7.1); nothing fails silently.
- Untrusted input: a time limit on LibreOffice, killed on timeout or cancel.

## Review Focus
1. LibreOffice already open on the desktop: a conversion must still work (own profile) and must not touch the user's profile.
2. A file name with spaces, non-ASCII or a leading dash reaches soffice safely (copied to a fixed name).
3. LibreOffice hangs or the user cancels: the process is killed and the job ends with a clear message.
4. LibreOffice exits 0 but writes nothing (unsupported or broken input): a clear error, not a missing file.
5. LibreOffice installed while the app runs: the targets turn on without restarting (detection on each listing).

---

### Task 1: Find and run LibreOffice
**Files:** `crates/fo-core/src/libre/mod.rs`, `crates/fo-core/src/lib.rs`, `crates/fo-core/tests/libre_run.rs`.
**Produces:** `pub fn find_soffice() -> Option<PathBuf>` (FO_SOFFICE, then PATH `soffice`/`libreoffice`, then install folders); `pub fn find_in(env_override, path_var, extra: &[PathBuf]) -> Option<PathBuf>`; `pub fn convert(soffice: &Path, input: &Path, filter: &str, ext: &str, work: &Path, cancel: &CancelToken, limit: Duration) -> Result<PathBuf, String>`.
- [ ] Tests (a fake `soffice` shell script): finds from PATH / override / install folder; convert passes `--headless --convert-to <filter> --outdir <dir>` and a `-env:UserInstallation=file://…` inside `work`, on a copy named `input.<ext>`; returns the written file; exits 0 writing nothing → error; sleeping script → killed at the limit with a time-limit message; cancel → killed. Real soffice (when installed): DOCX → PDF.

### Task 2: ODS/ODP kinds
**Files:** `fo-types/src/lib.rs`, `fo-core/src/detect.rs`, `tests/detect.rs`, every exhaustive `FileKind` match.
- [ ] Test: `.ods`/`.odp` (zip with mimetype) detect as `Ods`/`Odp`; labels "OpenDocument spreadsheet"/"OpenDocument presentation".

### Task 3: `office.convert` and the unavailable state
**Files:** `fo-types/src/lib.rs` (`Needs { message, url }`, `ToolInfo.needs`), `libre/tool.rs`, `fo-app/src/state.rs`, `fo-app/src/commands.rs` (`open_libreoffice_download`).
**Targets:** doc→docx, odt, rtf, pdf · docx/odt/rtf→pdf · xls→xlsx, ods, csv, pdf · xlsx→ods, pdf · ods→xlsx, pdf · ppt→pptx, odp, pdf · pptx→odp, pdf · odp→pptx, pdf · md/html/txt/latex/rst/org/adoc/epub→pdf.
- [ ] Tests: info lists those targets per kind, `platforms == [Desktop]`; with no soffice, `needs` is set ("Install LibreOffice to enable", libreoffice.org/download) and `run` fails with that message; with soffice, `needs` is None.

### Task 4: Conversion and report
**Files:** `libre/tool.rs`, `tests/libre_convert.rs` (skip with a printed note when soffice is missing).
- [ ] Tests (real soffice): DOCX (heading, table, image) → PDF: words recall 1.0, images 1/1, report Exact; DOC (made by LibreOffice from that DOCX) → DOCX: tables and images kept, warning that only LibreOffice reads .doc; XLSX → PDF keeps every cell value; PPTX → PDF; Markdown → PDF via Pandoc; ODS → XLSX; broken .docx → clear error.

### Task 5: UI
**Files:** `fo-ui/src/components/file_list.rs`, `fo-ui/src/strings.rs`, `fo-ui/src/mock.rs`, styles.
- [ ] Unavailable targets render as disabled options ("PDF — needs LibreOffice"); a file whose every target is unavailable shows "Install LibreOffice to enable" with a "Get LibreOffice" link (calls `open_libreoffice_download`). The mock shows both states.

### Task 6: Ship
- [ ] fmt, clippy (native + wasm), workspace tests; release e2e under Xvfb (DOCX → PDF with LibreOffice); CI installs nothing new (tests skip without soffice; the fake-soffice tests always run); PR, CI green, merge.
