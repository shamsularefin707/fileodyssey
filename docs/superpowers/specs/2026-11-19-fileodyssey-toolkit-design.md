# FileOdyssey — Offline File & Image Toolkit (Design)

**Date:** 2026-11-19
**Owner:** Arefin (shamsularefin707)
**Status:** Approved 2026-11-19
**Home:** a NEW GitHub repo `fileodyssey`; product name **FileOdyssey** [decided]. The existing
`file-compression-and-converter` web app repo is left completely untouched.

Markers used below: **[decided]** = Arefin decided it in conversation.
**[proposal]** = designer's call, open to override. **[builder]** = left to
whoever builds it.

---

## 1. Intent — what and why

The current app, FileForge, is a React/Vite web app on Vercel with 18 conversion tools.
Arefin wants a different product: **one installable, standalone app that is
a complete file and image toolkit**, instead of "yet another online
converter."

- Standalone, not web-based. Installed software that does all processing
  on the user's own device and works offline. **[decided]**
- Written in Rust (Python was the other candidate) instead of the
  React/Vite stack. **[decided]**
- Public audience, not just Arefin and friends. Distributed through
  **GitHub Releases only** for now. No Play Store. **[decided]**
- Conversions must keep the content of the original "the same", with near-zero
  data loss. This is the product's core promise. **[decided]**

What makes it different from iLovePDF/Smallpdf-style tools: files never leave
the device, there are no size or upload limits, it works offline, and every
tool lives in one app on desktop and phone.

## 2. Platforms and scope

Platforms: **Windows and Linux desktop + Android**. **[decided]**
(iOS and macOS are out: building for Apple needs a Mac, and they weren't asked for.)

| Capability | Desktop (Win/Linux) | Android |
|---|---|---|
| File compression (archives, PDF, Office files) | ✓ | ✓ |
| File reader (PDF, EPUB, DOCX, ODT, TXT, MD, HTML, images) | ✓ | ✓ |
| Document conversion via Pandoc (MD, DOCX, HTML, EPUB, ODT, TXT, RTF, LaTeX, …) | ✓ | ✓ |
| PDF → DOCX / MD / TXT / HTML | ✓ | ✓ |
| Conversions that need LibreOffice (legacy .doc/.xls/.ppt, Office → PDF with exact layout) | ✓ (if LibreOffice is installed) | — |
| Image compression and resizing | ✓ | ✓ |
| Image format conversion (every format in §6.4) | ✓ | ✓ |
| Document scanner (edge detect, perspective fix, multi-page → PDF) | — | ✓ |
| QR / barcode scanning with the camera | — | ✓ |
| QR / barcode decoding from an image file | ✓ **[proposal]** | ✓ |

The split was decided as "phone does everything it can, offline; only
LibreOffice-dependent conversions stay on desktop". **[decided]**
Scans move from phone to desktop through the normal Android share sheet or
by saving files. There is no built-in phone↔PC sync. **[proposal]**

## 3. Goals, non-goals, anti-goals

**Goals**
1. All five capability groups above, offline, in one app.
2. Conversions with near-zero content loss. Text, headings, lists, tables,
   images, links and footnotes survive when the target format can represent
   them.
3. A single public v1.0.0 release on GitHub when everything works: Windows installer, Linux AppImage + .deb, and
   a signed Android APK.
4. Batch operations: drop many files, run one tool over all of them. **[proposal]**

**Non-goals (v1)**
- Play Store / app-store distribution, iOS, macOS.
- Accounts, cloud storage, any server, telemetry.
- OCR (turning scans into editable text). It wasn't asked for and can be added later. **[proposal]**
- Editing documents (the reader is read-only). PDF form filling.
- Raster → vector tracing (e.g. PNG → SVG as real vectors).
- Video/audio conversion.

**Anti-goals: things that make it a failure even if it "works"**
- **Silent data loss.** A conversion that drops a table or image without
  telling the user is worse than refusing. Every conversion reports what it
  could not preserve (§7).
- Any file content leaving the device.
- Making LibreOffice a mandatory multi-hundred-MB install. It stays optional.
- Pretending a weak conversion is perfect. Known-lossy paths are labeled as
  such in the UI.

## 4. Approaches considered

| | Approach | Trade-off | Verdict |
|---|---|---|---|
| **A** | **Rust core + Tauri 2, UI in Rust with Leptos** | One codebase covers Win/Linux/Android installers, with official plugin support for camera/barcode. Fewer ready-made UI components than React, so screens take more hand-building. PDF→DOCX has no strong Rust library. | **Chosen [decided]** |
| B | All-Rust with Dioxus | Simpler setup, but Android support and camera plugins are immature, so both scanners would be built from scratch. | Rejected: the scanners are the riskiest part |
| C | Python (PySide6 desktop + Kivy/Flet Android) | Has the best PDF→DOCX library (pdf2docx), but needs two separate UIs. Heavy native libraries (MuPDF, OpenCV) are a struggle on Android, and installers get large. | Rejected |

Earlier options that were dropped: keeping the web app (Arefin wants
standalone), and a Python server behind a Rust app (it needs a server, so it
isn't offline).

## 5. Architecture

### 5.1 Repository layout **[proposal]**

Cargo workspace in the new repo:

```
fileodyssey/
  crates/
    fo-core/        # pure Rust engines, no UI; all conversion/compression logic
                    #   + the fidelity report types. Fully unit-testable on CI.
    fo-app/         # Tauri 2 app (src-tauri): commands, file dialogs, job queue,
                    #   platform glue, LibreOffice detection, updater
    fo-ui/          # Leptos (client-side rendered, built with Trunk) frontend
  plugins/
    tauri-plugin-docscan/   # Android-only Kotlin plugin wrapping ML Kit
                            #   Document Scanner
  assets/pandoc/    # pandoc.wasm (fetched at build time, not committed)
  tests/corpus/     # sample input files + expected-fidelity fixtures
  docs/superpowers/specs/   # this document
  .github/workflows/        # CI + release builds
```

Rule: **every engine lives in `fo-core`, behind one trait**, so the UI never
knows which library did the work:

```rust
trait Tool {
    fn id(&self) -> &'static str;
    fn accepts(&self, input: &FileKind) -> bool;
    fn platforms(&self) -> Platforms;          // Desktop | Android | Both
    fn run(&self, input: &Path, opts: &Options, out_dir: &Path,
           progress: &dyn Progress) -> Result<Outcome>;
}
struct Outcome { outputs: Vec<PathBuf>, report: FidelityReport }
```

Jobs run on a background thread pool so the UI never freezes, and each job
can be cancelled. **[proposal]**

### 5.2 Engines and libraries **[proposal unless marked]**

| Area | Library | Notes |
|---|---|---|
| Document conversion | **pandoc.wasm 3.9+** (official WASM build), run inside Rust via **wasmtime** (WASI) | The same engine on desktop and Android. About 58 MB uncompressed, ~16 MB compressed. Fallback if wasmtime on Android is a problem: run it in the webview through pandoc's JS bridge. **[builder]** picks after a spike. |
| PDF render / text / structure | **PDFium** via `pdfium-render` (prebuilt binaries for Win/Linux/Android) | Rendering the reader, extracting text with positions and fonts, extracting images. See license note §10. |
| PDF write / compress | `lopdf` | Recompress and downsample embedded images, object streams, strip unused objects. |
| PDF → DOCX/MD/HTML | Custom reconstruction in `fo-core`: PDFium structured text → reading order (multi-column detection), paragraphs, headings (by font size and weight), lists, tables (from ruling lines **and** whitespace/alignment grids, including merged cells), images at their position, links, footnotes, headers/footers (detected and kept separate) → an intermediate document → `docx-rs` writes DOCX directly (so fonts, sizes and colours are kept) and Pandoc writes MD/HTML | **Must be high quality in v1 [decided].** This is the largest single phase. The quality bar is in §7.6. |
| LibreOffice (desktop only) | Detect `soffice` on PATH or in standard install folders, then run `soffice --headless --convert-to` into a temp dir | If it's missing, those tools show "Install LibreOffice to enable" with a link. **[decided: optional]** |
| Images: decode | `image` crate (PNG, JPEG, GIF, BMP, ICO, TIFF, WebP, PNM, TGA, DDS, HDR, EXR, QOI, farbfeld), AVIF via dav1d, `libheif-rs` (HEIC/HEIF), `resvg` (SVG → raster), `rawler` (camera RAW: CR2/CR3/NEF/ARW/DNG/ORF/RW2…), `jxl-oxide` (JPEG XL), `psd` (flattened PSD) | HEIC: because of HEVC patents, Android uses the system decoder (`ImageDecoder`, API 28+) instead of bundling libde265. Desktop bundles libheif + libde265, as many open-source viewers do, and notes it in the README. **[proposal]** |
| Images: encode | `mozjpeg` (JPEG), `oxipng` (PNG lossless optimise), `webp` (lossy/lossless), `ravif` (AVIF), `jpegxl-rs` (JXL), `image` for the rest | RAW and PSD are **read-only** (no format writes them back sensibly). SVG output = the raster embedded in an SVG wrapper, clearly labeled. |
| Image resize | `fast_image_resize` | By pixels, by percent, fit-within box, or target file size (binary search on quality). |
| Metadata | Keep EXIF/ICC by default, with a "strip metadata" option | Keeping colour profiles matters for "no data loss". |
| Archives | `zip`, `sevenz-rust`, `tar` + `flate2` / `xz2` / `zstd` | Create **and** extract. |
| Office-file compression | DOCX/PPTX/XLSX/EPUB are zips: recompress embedded images and re-zip at max deflate | |
| EPUB reader | `epub` crate → sanitised XHTML rendered in the webview, with chapter navigation | |
| DOCX/ODT reader | Convert to HTML with pandoc.wasm in memory, then render | |
| QR/barcode from file | `rxing` (Rust port of ZXing) | |
| QR/barcode camera (Android) | **official `tauri-plugin-barcode-scanner`** | QR, EAN, UPC, Code128, DataMatrix, PDF417, Aztec. Detects payload type: URL (open), Wi-Fi (show credentials), vCard, text (copy). |
| Document scanner (Android) | Custom Kotlin plugin over **Google ML Kit Document Scanner** | It provides edge detection, perspective correction, filters and multi-page capture. Output: JPEGs + an assembled PDF. Requires Google Play services, so it won't work on phones without them (e.g. Huawei) and the UI says so. |
| UI | Leptos (CSR) + Trunk; styling with plain CSS or the Tailwind standalone CLI (no Node) | **[builder]** chooses the styling approach. |

### 5.3 UI shape **[proposal]**

```
┌ FileOdyssey───────────────────────────────────────────────┐
│ ▸ Convert   ▸ Compress   ▸ Images   ▸ Read   ▸ Scan*     │  *Android only
├──────────────────────────────────────────────────────────┤
│   Drop files here (or tap to pick)                       │
│   ─ report.pdf      PDF → [DOCX ▾]   ⚠ best effort       │
│   ─ notes.md        MD  → [DOCX ▾]   ✓                   │
│   [Output folder: ~/Documents/FileOdyssey]   [ Run all ]   │
├──────────────────────────────────────────────────────────┤
│ Recent jobs: ✓ notes.docx  (open · show in folder · report)│
└──────────────────────────────────────────────────────────┘
```

- Drop or pick files first. The app detects the type (by content, not just the
  extension) and offers only the targets that are valid for it.
- Every result has a **"Report"** link that shows the fidelity report.
- Light and dark theme that follows the system. English UI for v1. Strings go
  in a resource file so Bengali can be added later. **[proposal]**
- Android: the same tabs as a bottom navigation bar, and "Open with
  FileOdyssey" / share-sheet targets for incoming files.
- Outputs never overwrite inputs. The default name is `name.ext` →
  `name (converted).newext`.

## 6. Feature details

### 6.1 File compression
- Archives: create ZIP / 7z / tar.gz / tar.xz / tar.zst with a level
  slider; extract all of these plus RAR **extract-only** if a pure-Rust or
  permissively licensed reader exists. **[builder]** checks that; otherwise
  RAR is out.
- PDF compression presets: Lossless (structure only), Balanced (images
  ≤150 dpi, JPEG q80), Strong (≤96 dpi, q60). Shows before and after sizes.
- Office/EPUB compression: recompress embedded images.

### 6.2 Reader
PDF (zoom, page thumbnails, text search, copy text), EPUB (chapters, font size),
DOCX/ODT/RTF (rendered via HTML), TXT/MD (MD rendered), HTML (sanitised,
no scripts, no network), images (zoom, EXIF panel). Read-only.

### 6.3 Document conversion
Supported targets per source: everything Pandoc supports among
MD, CommonMark/GFM, HTML, DOCX, ODT, EPUB, RTF, TXT, LaTeX, RST, Org, AsciiDoc,
plus PDF as a **source** (custom path, §5.2). PDF as a **target**:
- desktop with LibreOffice → exact layout via LibreOffice;
- everywhere else → Pandoc → HTML → PDF rendered by the platform webview's
  print-to-PDF, or by Typst (`typst` crate, pure Rust) — **[builder]** picks
  after comparing output quality.

Legacy and Office-layout formats (.doc, .xls, .ppt, .pptx→pdf, .xlsx→pdf):
LibreOffice, desktop only.

Data formats from the old web app (CSV↔JSON, CSV→XLSX) are **carried over**
because they're cheap and existing users know them. **[proposal]**

### 6.4 Images
"Any and all types" is defined as:
- **Read:** PNG, JPEG, GIF (first frame or all frames), BMP, ICO, TIFF
  (multi-page), WebP, AVIF, HEIC/HEIF, JPEG XL, SVG, PSD (flattened),
  camera RAW, TGA, DDS, HDR, EXR, QOI, PNM, PDF pages (rasterised).
- **Write:** PNG, JPEG, WebP, AVIF, JPEG XL, GIF, BMP, ICO (multi-size),
  TIFF, TGA, QOI, PNM, PDF (image → PDF, multiple images → one PDF).
- Compression: quality slider + "target size" mode + lossless optimise for PNG.
- Resize: pixels, percent, fit box, with aspect lock; batch.
- Animated GIF/WebP: kept animated when the target supports it, otherwise
  first frame + a warning in the report.

### 6.5 Android scanning
- **Documents:** ML Kit flow → the user reviews and reorders pages → save as
  PDF and/or JPEGs → open in the reader or pass straight into the converter.
- **QR/barcodes:** live camera scan → result sheet with actions
  (open URL after showing it, copy, join Wi-Fi, save contact). URLs are never
  auto-opened. **[proposal]**
- Scan history kept locally and deletable. **[proposal]**

## 7. Fidelity: how "near-zero data loss" is enforced

1. **Fidelity report on every conversion.** Before and after counts of
   characters (normalised text), headings, paragraphs, tables, images, links,
   footnotes and pages, plus a list of warnings ("2 tables converted to plain text",
   "font 'X' substituted", "animation dropped"). The report sits under each
   result. If text similarity < 99% or any element count drops, the result
   is marked ⚠ instead of ✓.
2. **Golden test corpus** in `tests/corpus/`: real-world samples per format
   (with tables, images, footnotes, RTL/Bengali text, multi-column PDFs, scanned
   PDFs). CI runs every conversion pair and fails if fidelity metrics drop
   below the stored baseline.
3. **Round-trip tests** where both directions exist (MD→DOCX→MD,
   HTML→DOCX→HTML): normalised text must match.
4. Paths known to be lossy (scanned PDFs, raster→SVG, RAW/PSD→anything, and
   DOCX/ODT→PDF when LibreOffice is not available) are labeled "best effort"
   in the UI before the user runs them. **[proposal]** For best-effort paths
   the CI gate compares against their own stored baseline (catching
   regressions), not against the 99% bar.
5. Scanned (image-only) PDFs as a conversion source: output keeps the page
   images and the report says "no text layer, OCR not supported". Nothing
   fails silently. **[proposal]**
6. **PDF → DOCX/MD/HTML quality bar for v1 [decided: high quality].** On
   the digital-PDF part of the corpus (single- and multi-column articles,
   reports with tables, invoices/forms, academic papers with footnotes, Bengali
   and English text), the result must have:
   - ≥ 99% normalised text similarity;
   - correct reading order;
   - every image present;
   - headings detected;
   - ≥ 90% of table cells in the right row and column.

   Each corpus file is also opened in Word and LibreOffice and checked by eye
   before release. v1 does not ship until this bar is met.

## 8. Distribution **[decided: GitHub Releases only]**

- GitHub Actions release workflow on tag `v*`:
  - Windows: NSIS `.exe` installer (+ `.msi`).
  - Linux: `.AppImage` + `.deb`.
  - Android: APK for arm64-v8a (+ armeabi-v7a, x86_64 **[builder]**),
    signed with a release keystore kept in GitHub Actions secrets.
- **Updates:** desktop uses the Tauri updater plugin with signed update
  manifests published as release assets. Android shows "new version
  available" with a link to the release (no silent self-install). **[proposal]**
- Windows code signing: not in v1 (costs money). Users will see a SmartScreen
  warning, and the README explains it. **[proposal]**
- Expected sizes: desktop about 90–130 MB, Android APK about 80–110 MB,
  dominated by pandoc.wasm and PDFium. Acceptable for a GitHub release.

## 9. Constraints

- Fully offline. The only network use is the optional update check, which can
  be turned off.
- No temp files left behind. Work in a per-job temp dir, deleted afterwards
  (LibreOffice needs real files).
- Safety: inputs are untrusted. Run parsers with size and time limits, sanitise
  HTML/EPUB before rendering (no scripts, no remote loads), and guard against
  zip bombs and path traversal when extracting archives.
- Android minimum SDK per Tauri 2 / ML Kit requirements (≈ API 24+).
- Large files (hundreds of MB) must stream or chunk where the engine allows
  and must not crash on low-RAM phones. Above a threshold, show a warning.

## 10. Licensing — needs Arefin's decision

The old repo is MIT. Several of the best engines are copyleft:
- **Pandoc: GPL-2.0+.** Shipping pandoc.wasm inside the app is safest under a
  GPL-compatible app license.
- libheif / rawler: LGPL (fine if dynamically linked or source is offered).
- PDFium: BSD/Apache (permissive). MuPDF was deliberately avoided because it's AGPL.

**Decision [decided]:** FileOdyssey is licensed **GPL-3.0-or-later with
an additional permission** (GPLv3 §7) allowing it to be combined with Google
Play services / ML Kit libraries. This keeps Pandoc and the ML Kit scanners.
The exception text goes in `LICENSE` and the README. The old repo stays MIT.

## 11. Build order

**Release strategy [decided]:** one public release, v1.0.0, when all five
capability groups work on their platforms. No public pre-releases. Internal
CI builds are made for every phase so progress can still be tested
on real devices. **[proposal]**

Suggested order **[proposal]**:

1. Workspace + Tauri/Leptos shell running on Win, Linux, Android; CI building all three.
2. Images (convert, compress, resize) — exercises the job queue, report and batch UI.
3. Archives + PDF/Office compression.
4. Reader (PDF, EPUB, DOCX, text, images).
5. Pandoc conversions + fidelity reports + golden corpus.
6. PDF → DOCX/MD/HTML reconstruction (largest phase; iterate against §7.6).
7. LibreOffice integration (desktop).
8. Android: QR scanner, then the document-scanner plugin.
9. Release pipeline, updater, README/download docs → v1.0.0 on GitHub.

## 12. Left to the builder

- Exact crate versions; wasmtime vs webview for pandoc.wasm (after a spike);
  Typst vs webview print for PDF output; UI styling approach; extra Android ABIs;
  whether RAR extraction is possible under the license constraints; temp-dir
  strategy and limits; component structure inside Leptos.
