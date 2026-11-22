# M8 — Android scanning Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** On Android, the Scan tab scans QR codes and barcodes with the camera (result sheet with safe actions), reads codes from image files, and scans paper documents with ML Kit (review, reorder, save as PDF and/or JPEGs, then read or convert), with a local, deletable scan history.

**Architecture:** Pure logic lives in `fo-core/src/scan/` and is tested natively: `payload` (what a code holds), `history` (JSON store), `document` (assemble reviewed pages into a PDF with JPEGs embedded unchanged, write JPEG copies). Camera QR uses the official `tauri-plugin-barcode-scanner` in windowed mode (camera behind a transparent webview, our own frame and Cancel). Documents use a new Kotlin plugin `crates/tauri-plugin-fo-android` (ML Kit Document Scanner via `startIntentSenderForResult`, plus adding a Wi-Fi network through the system dialog). `fo-app` adds Scan commands; both plugins are registered on Android only, with an Android-only capability. The UI's Scan tab drives the flows; the browser preview simulates them.

**Tech Stack:** tauri-plugin-barcode-scanner 2.5, ML Kit Document Scanner (`play-services-mlkit-document-scanner`), Kotlin, rxing, pdf-writer.

**Spec:** §2 table (scanner rows), §4 layout (Android-only Kotlin plugin), §5 table (barcode / document scanner rows), §5.3 (Scan tab), §6.5, §10 (ML Kit exception), §11 step 8.

## Global Constraints
- URLs are never opened automatically: shown first, opened on a tap, http(s) only (§6.5).
- Document scanner needs Google Play services; without it the UI says so (§5).
- Scan history is kept locally and is deletable (§6.5).
- Scanning is Android-only; desktop builds don't include either plugin.
- Outputs never overwrite: `Scan 2026-11-22 14.05.pdf`, then ` (2)` etc.

## Review Focus
1. A QR code with a `javascript:`, `intent:` or `file:` link must never be openable.
2. Wi-Fi strings with escaped `;`, `:` or `\` (`WIFI:S:My\;Net;…`) parse to the right name and password.
3. Deleting a scan from history removes its files only inside the app's scan store, never a file the user saved elsewhere.
4. Reordering then deleting pages keeps the PDF in the order shown; zero pages can't be saved.
5. The user cancels the camera or ML Kit: no error toast, back to the Scan tab.

---

### Task 1: What a code holds
**Files:** `fo-core/src/scan/mod.rs`, `fo-core/src/scan/payload.rs`, `fo-types` (`Payload`), `tests/scan_payload.rs`.
**Produces:** `pub fn classify(content: &str) -> Payload`; `enum Payload { Url { url }, Wifi { ssid, password, security, hidden }, Contact { name, phones, emails, org, vcard }, Email { to, subject, body }, Phone { number }, Sms { number, body }, Geo { lat, lon }, Text { text } }`.
- [ ] Tests: http/https URL (and bare `www.` → https); `javascript:`/`intent:`/`file:` → Text; WIFI with escapes, nopass, hidden; vCard 3.0 and MECARD → Contact (with the original vCard kept for saving); mailto with subject; tel; SMSTO; geo; plain text.

### Task 2: Scan history
**Files:** `fo-core/src/scan/history.rs`, `fo-types` (`ScanRecord`), `tests/scan_history.rs`.
**Produces:** `History::open(dir) -> History`, `add(kind) -> ScanRecord`, `list() -> Vec<ScanRecord>` (newest first), `delete(id)`, `clear()`; `ScanKind::Code { content, format } | Document { files: Vec<PathBuf> }`.
- [ ] Tests: add/list order, survives reopen, delete removes the record and files under the store's own folder only (a file outside it stays), clear, a corrupt file starts empty, at most 500 records (oldest dropped).

### Task 3: Assemble scanned pages
**Files:** `fo-core/src/scan/document.rs`, `tests/scan_document.rs`.
**Produces:** `save_scan(pages: &[PathBuf], order: &[usize], out_dir: &Path, stem: &str, pdf: bool, jpegs: bool) -> Result<Vec<PathBuf>, String>`.
- [ ] Tests: three JPEGs of different sizes, order [2,0,1] → PDF pages in that order, JPEG bytes embedded unchanged, A4-fitted pages; JPEG copies named `stem (page 1).jpg`…; existing names get ` (2)`; empty order → error; an index out of range → error.

### Task 4: Android plugin and app commands
**Files:** `crates/tauri-plugin-fo-android/{Cargo.toml,build.rs,src/lib.rs,permissions/,android/…/FoAndroidPlugin.kt}`, workspace `Cargo.toml`, `fo-app/Cargo.toml` (android-only deps), `fo-app/src/lib.rs`, `fo-app/src/scan.rs` (commands), `fo-app/capabilities/android.json`, CI.
**Commands:** `scan_classify(content) -> Payload`, `scan_codes_in_image(path) -> Vec<String>`, `scan_record_code`, `scan_save_document(pages, order, pdf, jpegs) -> Vec<String>`, `scan_history`, `scan_delete(id)`, `scan_clear`, `scan_open_url(url)` (http(s) only), `scan_save_contact(vcard) -> String`; plugin commands `fo-android|scan_document {pageLimit}`, `fo-android|scanner_available`, `fo-android|add_wifi {ssid,password,security}`.
- [ ] Tests (native): `scan_open_url` refuses non-http(s); `scan_codes_in_image` on a generated QR PNG; `scan_save_contact` writes a .vcf that doesn't overwrite. CI builds the APK with both plugins.

### Task 5: Scan tab UI
**Files:** `fo-ui/src/components/scan.rs`, `fo-ui/src/strings.rs`, `fo-ui/src/mock.rs`, `fo-ui/style/*`, `e2e-scan.js`.
- [ ] Scan tab: "Scan a document", "Scan a QR code", "Read a code from an image"; QR overlay (transparent page, frame, Cancel); result sheet per payload (URL shown with Open/Copy; Wi-Fi name and password with Join/Copy password; contact with Save contact; text/phone/email Copy); review screen (page thumbnails, move up/down, remove, PDF/JPEG choices, Save); after saving: Open in reader / Convert; history list with delete and Clear history; "needs Google Play services" message. Browser e2e drives each with the mock.

### Task 6: Ship
- [ ] fmt, clippy (native + wasm), workspace tests, browser e2e; CI APK green (Kotlin compiles); PR, merge.
