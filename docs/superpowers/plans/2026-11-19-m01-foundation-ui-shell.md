# M1 — Foundation & UI Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Cargo workspace that builds a Tauri 2 + Leptos app for Windows/Linux/Android. It detects dropped files by content, runs jobs through a tool registry with progress, cancel and safe output naming, and shows them in the organic UI shell. CI builds all three platforms.

**Architecture:** `ff-types` holds serde types shared by Rust and wasm. `ff-core` holds the engines: detection, naming, the `Tool` trait, the registry and the job runner, with no UI. `ff-app` is the Tauri shell: commands, events, dialogs and drag-drop. `ff-ui` is the Leptos CSR frontend built with Trunk, with a mock backend for browser preview. No real conversion tools yet; M2+ register them.

**Tech Stack:** Rust 1.97, Tauri 2.12 (stable, NOT 3.0-alpha), Leptos 0.8 (stable, NOT 0.9-rc), Trunk, wasm-bindgen, `infer` 0.22, `zip`, `serde`/`serde_json`, `uuid`, `tauri-plugin-dialog` 2.8, `tauri-plugin-opener` 2.7, `tempfile` (tests).

**Spec:** `docs/superpowers/specs/2026-11-19-fileforge-native-toolkit-design.md` · Roadmap: `docs/superpowers/plans/2026-11-19-roadmap.md`

## Global Constraints

- Platforms: Windows, Linux, Android. No iOS/macOS targets.
- Fully offline. No network calls anywhere in M1. Fonts are bundled, never loaded from a CDN.
- Outputs never overwrite inputs or existing files. Default name `name (converted).ext`, then `name (converted 2).ext`, and so on.
- No temp files left behind. Each job uses a temp dir that is deleted on success, failure and cancel.
- Inputs are untrusted. Detection reads at most the first 8 KiB plus the zip central directory.
- Licence: GPL-3.0-or-later with an additional permission (GPLv3 §7) for Google Play services / ML Kit.
- English UI, with every user-facing string in one module (`ff-ui/src/strings.rs`) so Bengali can be added later.
- Scan tab only on Android.
- `cargo clippy --workspace -- -D warnings` and `cargo fmt --check` clean.

## Design Direction (from frontend-design pass; binding for all modules)

The subject is a private, offline workbench for files. "Organic" means natural, unhurried forms: water and stone, not a SaaS dashboard.

| Token | Light | Dark | Role |
|---|---|---|---|
| `--lichen` | `#E4EAE0` | `#15241D` | app background |
| `--pine` | `#1E3A2F` | `#DCE6D6` | text, icons |
| `--stone` | `#C3C9BC` | `#33463C` | rows, dividers, inactive |
| `--river` | `#5F8794` | `#7FA7B3` | interactive / focus / progress |
| `--ochre` | `#B8862B` | `#D3A54A` | "best effort" and warnings only |
| `--moss` | `#4E7A4A` | `#86B07F` | success ✓ |

- **Type:** headings in *Young Serif* (soft, rounded serif), with only a few headings per screen. All UI text in *Atkinson Hyperlegible Next*. Both are OFL and bundled as woff2 from `@fontsource`. Scale 1.25: 13 / 16 / 20 / 25 / 31 px. Sentence case everywhere. No all-caps labels, no monospace labels, no middle-dot meta strings, no "→" on buttons.
- **Layout:** left vertical rail of tabs on desktop, bottom bar on Android (<720px). The main column is left-aligned with a max line length of 72ch. At the top sits the **pond**, the drop zone: one large asymmetric organic shape with a hand-tuned `border-radius: 58% 42% 47% 53% / 45% 55% 45% 55%`, filled `--stone` at 35% with a 2px `--river` dashed edge on dragover. Below it the file list is plain rows on hairline `--stone` dividers (not cards). The report drawer slides in from the right on desktop and from the bottom on Android.
- **The one bold thing:** the pond. When files are dropped, a single ripple (concentric ring, 600 ms) spreads from the drop point and the shape morphs slightly. That is the only ambient-feeling motion. It is user-triggered and disabled under `prefers-reduced-motion`. Everything else stays quiet: no hover lift on rows, no entrance animations.
- **Copy:** plain verbs. Empty pond: "Drop files here, or choose files". No tools yet: "No tools for this file type yet". Errors say what happened and what to do.
- **Quality floor:** visible `--river` focus ring (2px, offset 2px), WCAG AA contrast in both themes (verify `--ochre` on `--lichen`; darken if under 4.5:1 for text), keyboard reachable, theme follows `prefers-color-scheme`.

```
┌──────┬───────────────────────────────────────────────┬───────────┐
│ ◌    │  Convert                                      │ Report    │
│ Conv │      ╭────────────────────────────╮           │ (drawer,  │
│ Comp │     (   Drop files here, or         )         │  hidden   │
│ Img  │      ╲   choose files              ╱          │  until    │
│ Read │        ╰──────────────────────────╯           │  opened)  │
│ Scan*│  report.pdf     PDF   to [DOCX ▾]  ⚠ best effort│           │
│      │  notes.md       Markdown to [DOCX ▾]          │           │
│      │  Save to ~/Documents/FileForge  [Change] [Run all]│        │
│      │  ─ notes (converted).docx  ✓  Open · Show · Report│        │
└──────┴───────────────────────────────────────────────┴───────────┘
```

## Review Focus

1. **Wrong or missing extension** (a PNG named `.txt`, a DOCX renamed `.zip`): the kind comes from content, so the right tools are offered. → Task 2 tests.
2. **Output name collision / output dir = input dir:** never overwrites, picks the next free `(converted N)` name. → Task 3 tests.
3. **Unicode and spaces in paths** (`রিপোর্ট ফাইল.pdf`): detection, naming and job events keep the exact name. → Tasks 2, 3 tests.
4. **Cancel or failure mid-job:** no partial output in the out dir and no leftover temp dir. → Task 3 tests.
5. **Zero-byte, unreadable or missing files:** `Unknown`/error result, a `Failed` event with a clear message, never a panic. → Tasks 2, 3, 4 tests.

---

### Task 1: Workspace, shared types, licence

**Files:**
- Create: `Cargo.toml` (workspace: `crates/ff-types`, `crates/ff-core`, `crates/ff-app`, `crates/ff-ui`), `rust-toolchain.toml` (channel `stable`, targets `wasm32-unknown-unknown`, `aarch64-linux-android`, `armv7-linux-androideabi`, `x86_64-linux-android`), `.gitignore`, `LICENSE` (GPL-3.0 text + "Additional permission under GNU GPL version 3 section 7" paragraph naming Google Play services and Google ML Kit libraries), `README.md` (one-paragraph description + build instructions), `crates/ff-types/src/lib.rs`
- Test: `crates/ff-types/tests/serde_roundtrip.rs`

**Interfaces:**
- Produces (all `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]`, `#[serde(rename_all = "snake_case")]`):
  - `enum FileKind { Pdf, Docx, Odt, Rtf, Epub, Markdown, Html, Text, Csv, Json, Xlsx, Pptx, Doc, Xls, Ppt, Image(ImageFormat), Archive(ArchiveFormat), Unknown }`, with `fn label(&self) -> &'static str` ("PDF", "Word document", "Markdown", "PNG image", …)
  - `enum ImageFormat { Png, Jpeg, Gif, Bmp, Ico, Tiff, Webp, Avif, Heic, Jxl, Svg, Psd, Raw, Tga, Dds, Hdr, Exr, Qoi, Pnm }`
  - `enum ArchiveFormat { Zip, SevenZ, Tar, TarGz, TarXz, TarZst, Rar }`
  - `enum Category { Convert, Compress, Images, Read, Scan }`; `enum Platform { Desktop, Android }`
  - `struct ToolInfo { id: String, label: String, category: Category, targets: Vec<String>, best_effort: bool, platforms: Vec<Platform> }`
  - `struct DetectedFile { path: String, name: String, size: u64, kind: FileKind }`
  - `struct Metric { name: String, before: u64, after: u64 }`; `enum Verdict { Exact, Warn }`
  - `struct FidelityReport { metrics: Vec<Metric>, text_similarity: Option<f64>, warnings: Vec<String> }` with `fn verdict(&self) -> Verdict`: `Warn` if any `after < before`, or `text_similarity < 0.99`, or `warnings` non-empty; otherwise `Exact`
  - `struct JobRequest { tool_id: String, inputs: Vec<String>, target: Option<String>, options: serde_json::Value, out_dir: String }`
  - `enum JobStatus { Queued, Running { fraction: f32 }, Done { outputs: Vec<String>, report: FidelityReport }, Failed { message: String }, Cancelled }`
  - `struct JobEvent { job_id: String, input: String, status: JobStatus }`

- [ ] **Step 1: Write failing tests** `roundtrip_job_event_done` (serialize → deserialize equals; JSON contains `"done"`), `verdict_exact_when_nothing_lost`, `verdict_warn_on_dropped_metric` (`before 3, after 2`), `verdict_warn_on_similarity_0_985`, `verdict_warn_on_any_warning`, `image_kind_serializes_as {"image":"png"}`.
- [ ] **Step 2:** `cargo test -p ff-types` → FAIL (types missing).
- [ ] **Step 3:** Implement the types above in `crates/ff-types/src/lib.rs` (deps: `serde`, `serde_json` only, so it stays wasm-safe). Create the workspace files; `ff-core`/`ff-app`/`ff-ui` as empty lib/bin crates so the workspace compiles.
- [ ] **Step 4:** `cargo test -p ff-types` → PASS; `cargo build --workspace` → OK.
- [ ] **Step 5: Commit** `feat: workspace, shared types, GPL-3.0 licence with ML Kit permission` (include the spec, roadmap and this plan already in `docs/`).

### Task 2: File-type detection by content

**Files:**
- Create: `crates/ff-core/src/lib.rs`, `crates/ff-core/src/detect.rs`
- Test: `crates/ff-core/tests/detect.rs` (fixtures generated in-test into a `tempfile::TempDir`: magic bytes, and small zips built with the `zip` crate)

**Interfaces:**
- Consumes: `ff_types::{FileKind, ImageFormat, ArchiveFormat, DetectedFile}`
- Produces: `pub fn detect_kind(path: &Path) -> std::io::Result<FileKind>`; `pub fn detect_file(path: &Path) -> std::io::Result<DetectedFile>`

Rules, in order:
1. A zero-byte file → `Unknown`.
2. `infer` on the first 8 KiB for binary formats (pdf, images, 7z, rar, gz/xz/zst, doc/xls/ppt via CFB → map by extension, else `Doc`).
3. ZIP containers are inspected by entries:
   - `mimetype` == `application/epub+zip` → `Epub`; `application/vnd.oasis.opendocument.text` → `Odt`.
   - `[Content_Types].xml` plus `word/` → `Docx`, `ppt/` → `Pptx`, `xl/` → `Xlsx`.
   - Otherwise `Archive(Zip)`.
4. `.tar` by the `ustar` magic at offset 257.
5. Text formats by extension (`md|markdown`, `html|htm`, `csv`, `json`, `rtf` also by `{\rtf` prefix, `svg` also by `<svg` in the first 8 KiB). Any other valid UTF-8 → `Text`; otherwise → `Unknown`.

- [ ] **Step 1: Write failing tests:** `png_bytes_named_txt_is_png`, `docx_renamed_zip_is_docx`, `epub_detected_by_mimetype_entry`, `plain_zip_is_archive`, `pdf_magic`, `markdown_by_extension`, `svg_by_content`, `zero_byte_is_unknown`, `binary_garbage_is_unknown`, `bengali_name_with_spaces` (file `রিপোর্ট ফাইল.pdf` → `DetectedFile.name == "রিপোর্ট ফাইল.pdf"`, kind `Pdf`), `missing_file_is_io_error`.
- [ ] **Step 2:** `cargo test -p ff-core --test detect` → FAIL.
- [ ] **Step 3:** Implement `detect.rs` per the rules (deps `infer`, `zip`).
- [ ] **Step 4:** Tests → PASS.
- [ ] **Step 5: Commit** `feat(core): content-based file type detection`.

### Task 3: Tool trait, registry, safe naming, job runner

**Files:**
- Create: `crates/ff-core/src/{tool.rs,registry.rs,naming.rs,runner.rs}`
- Test: `crates/ff-core/tests/{naming.rs,runner.rs}` (a `TestTool` defined in the test: it writes the input uppercased; options `{"sleep_ms":n,"fail":true}`)

**Interfaces:**
- Consumes: Task 1 types; `detect_kind`.
- Produces:
  - `pub struct CancelToken(Arc<AtomicBool>)`: `new()`, `cancel()`, `is_cancelled() -> bool`, `Clone`
  - `pub struct Ctx<'a> { pub work_dir: &'a Path, pub target: Option<&'a str>, pub options: &'a serde_json::Value, pub cancel: &'a CancelToken, pub progress: &'a dyn Fn(f32) }`
  - `pub trait Tool: Send + Sync { fn info(&self) -> ToolInfo; fn accepts(&self, kind: &FileKind) -> bool; fn run(&self, input: &Path, ctx: &Ctx) -> Result<ToolOutput, ToolError>; }`
  - `pub struct ToolOutput { pub files: Vec<(PathBuf, String)>, pub report: FidelityReport }`: (file in `work_dir`, extension for the final name)
  - `pub enum ToolError { Cancelled, Failed(String) }` (+ `From<std::io::Error>`)
  - `pub struct Registry`: `new()`, `register(Arc<dyn Tool>)`, `tools_for(&FileKind, Platform) -> Vec<ToolInfo>` (sorted by label), `get(&str) -> Option<Arc<dyn Tool>>`
  - `pub fn output_path(input: &Path, out_dir: &Path, ext: &str) -> PathBuf`: `stem (converted).ext`, then ` (converted 2)`, ` (converted 3)`…; skips any path that exists **or** equals `input`
  - `pub struct Runner`: `new(Arc<Registry>, workers: usize)`; `submit(JobRequest, sink: Arc<dyn Fn(JobEvent) + Send + Sync>) -> String` (job id, uuid v4; one event stream per input, in order `Queued → Running* → Done|Failed|Cancelled`); `cancel(&str)`

Runner algorithm: for each input, create the work dir `out_dir/.fileforge-<jobid>-<n>`. Call `tool.run`. On `Ok`, move each file to `output_path(...)` with `fs::rename` (fallback copy+remove), then emit `Done` with the final paths. On `Err` or cancel, emit `Failed`/`Cancelled`. **Always** remove the work dir. An unknown `tool_id` → `Failed { "This tool isn't available" }`. A missing input → `Failed { "File not found: <name>" }`. Workers are a fixed pool of `std::thread`s fed by an `std::sync::mpsc` channel.

- [ ] **Step 1: Write failing naming tests:** `first_name_is_converted_suffix` (`a.pdf`, ext `docx` → `a (converted).docx`), `existing_name_gets_counter` (→ `a (converted 2).docx`), `never_returns_input_path` (input `x (converted).md`, same dir, ext `md` → `x (converted) (converted).md`), `unicode_stem_preserved`.
- [ ] **Step 2: Write failing runner tests:** `done_event_has_final_output_and_report`, `events_in_order_queued_running_done`, `cancel_mid_job_leaves_no_output_and_no_workdir` (`sleep_ms:500`, cancel after 50 ms → `Cancelled`, out dir contains only the input), `failing_tool_emits_failed_and_cleans_up`, `unknown_tool_fails_clearly`, `missing_input_fails_clearly`, `registry_filters_by_kind_and_platform`.
- [ ] **Step 3:** `cargo test -p ff-core` → FAIL.
- [ ] **Step 4:** Implement `tool.rs`, `registry.rs`, `naming.rs`, `runner.rs` with the interfaces above.
- [ ] **Step 5:** `cargo test -p ff-core` → PASS (run 3× to check for flakiness in the cancel test).
- [ ] **Step 6: Commit** `feat(core): tool registry, job runner with cancel, safe output naming`.

### Task 4: Tauri app shell and commands

**Files:**
- Create: `crates/ff-app/{Cargo.toml,build.rs,tauri.conf.json,capabilities/default.json,icons/*}`, `crates/ff-app/src/{main.rs,lib.rs,commands.rs,state.rs}`
- Test: `crates/ff-app/src/commands.rs` `#[cfg(test)] mod tests` (plain functions, no Tauri runtime)

**Interfaces:**
- Consumes: `ff_core::{detect_file, Registry, Runner}`; Task 1 types.
- Produces (Tauri commands; the UI calls these exact names with camelCase args):
  - `platform() -> Platform` (`cfg!(target_os = "android")`)
  - `detect_files(paths: Vec<String>) -> Vec<Result<DetectedFile, String>>` (serialized as `{ok}|{err}`)
  - `tools_for(kind: FileKind) -> Vec<ToolInfo>`
  - `default_out_dir() -> Result<String, String>`: `<Documents>/FileForge` (Android: app-specific external files dir), created if missing
  - `pick_files() -> Vec<String>`, `pick_folder() -> Option<String>` (`tauri-plugin-dialog`, blocking variants)
  - `start_job(req: JobRequest) -> String`; `cancel_job(job_id: String)`
  - `open_path(path: String)`, `reveal_path(path: String)` (`tauri-plugin-opener`)
  - Events: `"job-event"` (payload `JobEvent`), `"files-dropped"` (payload `Vec<String>`, from `WindowEvent::DragDrop(Drop)`)
- Logic lives in `fn detect_files_impl(paths) -> …` / `fn default_out_dir_impl(docs: &Path) -> …` so it can be unit-tested; the `#[tauri::command]` wrappers are one-liners.
- `tauri.conf.json`: identifier `io.github.shamsularefin707.fileforge`, productName `FileForge`, `build.frontendDist: "../ff-ui/dist"`, `build.devUrl: "http://localhost:1420"`, `beforeDevCommand: "trunk serve --config ../ff-ui/Trunk.toml"`, `beforeBuildCommand: "trunk build --release --config ../ff-ui/Trunk.toml"`, `app.withGlobalTauri: true`, CSP `default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' asset: data:; connect-src ipc: http://ipc.localhost` (no remote origins), min window 900×600.

- [ ] **Step 1: Write failing tests:** `detect_files_mixes_ok_and_err` (one real file, one missing → `[Ok, Err("File not found: …")]`), `default_out_dir_created_under_documents`, `tools_for_unknown_is_empty`.
- [ ] **Step 2:** Install Linux deps (`libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev libssl-dev`) and `cargo install tauri-cli --version '^2' --locked`, `cargo install trunk --locked`. `cargo test -p ff-app` → FAIL.
- [ ] **Step 3:** Implement the commands, state (`AppState { registry: Arc<Registry>, runner: Runner }`, workers = `available_parallelism - 1`, min 1) and the drag-drop → `files-dropped` emit. Generate icons with `cargo tauri icon` from a 1024px pond-shaped PNG (draw the SVG in `assets/icon.svg`: a `--river` organic blob on `--lichen`).
- [ ] **Step 4:** `cargo test -p ff-app` → PASS; `cargo build -p ff-app` → OK.
- [ ] **Step 5: Commit** `feat(app): Tauri shell with detection, jobs, dialogs, drag-drop`.

### Task 5: Leptos UI shell (organic design)

**Files:**
- Create: `crates/ff-ui/{Cargo.toml,Trunk.toml,index.html}`, `crates/ff-ui/src/{main.rs,bridge.rs,mock.rs,state.rs,strings.rs}`, `crates/ff-ui/src/components/{rail.rs,pond.rs,file_list.rs,jobs.rs,report.rs}`, `crates/ff-ui/style/{tokens.css,base.css,app.css}`, `crates/ff-ui/assets/fonts/*.woff2` (+ `OFL.txt`)
- Test: `crates/ff-ui/tests/state.rs` (`wasm-bindgen-test`, run headless with `wasm-pack test --node` on pure functions)

**Interfaces:**
- Consumes: Task 4 command and event names; Task 1 types (`ff-types` is a dependency).
- Produces:
  - `bridge.rs`: `async fn invoke<A: Serialize, R: DeserializeOwned>(cmd: &str, args: &A) -> Result<R, String>` and `fn listen<T: DeserializeOwned>(event: &str, f: impl Fn(T) + 'static)`. Both go to `window.__TAURI__` when present, else to `mock.rs`. The mock returns fake files and two fake tools ("To DOCX" best-effort, "To Markdown") and emits fake progress events, so `trunk serve` works in a normal browser.
  - `state.rs` (pure, testable): `fn visible_tabs(p: Platform) -> Vec<Category>` (Scan only on Android); `fn merge_files(existing: &[DetectedFile], new: Vec<DetectedFile>) -> Vec<DetectedFile>` (dedupe by path, keep order); `fn apply_event(jobs: &mut Vec<JobRow>, ev: JobEvent)` (`JobRow { job_id, input, status }`, upsert by `(job_id, input)`).
- UI behaviour:
  - Rail tabs filter which tools are offered: Convert/Compress/Images/Read map to `Category`.
  - The pond accepts `files-dropped` events plus a "choose files" button (`pick_files`).
  - Each file row shows its name, `kind.label()` and a target `<select>` from `tools_for`, with an ochre "best effort" mark when `best_effort`. Files with no tools show "No tools for this file type yet".
  - "Save to <dir> [Change]" uses `pick_folder`. "Run all" calls `start_job` per file.
  - Job rows show progress (a thin `--river` line that fills), then ✓ moss / ⚠ ochre from `report.verdict()`, with Open · Show in folder · Report actions.
  - The report drawer lists metrics before→after and the warnings.
- Tokens, type and layout exactly as in **Design Direction** above.

- [ ] **Step 1: Write failing tests:** `scan_tab_hidden_on_desktop`, `scan_tab_shown_on_android`, `merge_dedupes_by_path_and_keeps_order`, `apply_event_upserts_running_then_done`.
- [ ] **Step 2:** `wasm-pack test --node crates/ff-ui` → FAIL.
- [ ] **Step 3:** Implement `state.rs`, then `bridge.rs` + `mock.rs`, then the components and CSS. Fonts: `npm pack @fontsource/young-serif @fontsource/atkinson-hyperlegible-next` in a scratch dir; copy the latin woff2 files (400, 700) and the licence. If a package is missing, use the nearest OFL humanist alternative and note it in the commit.
- [ ] **Step 4:** Tests → PASS. `trunk build --config crates/ff-ui/Trunk.toml` → OK.
- [ ] **Step 5: Visual check.** Run `trunk serve` (mock backend) and screenshot with headless Chromium/Playwright at 1280×800 and 390×844, in light and dark, in three states: empty, files listed, job done with the report open. Critique against Design Direction:
  - Is the pond the only bold element?
  - Is any text using all caps or monospace?
  - Does `--ochre` text pass AA contrast?
  - Is there a visible focus ring?
  Fix the issues and re-screenshot.
- [ ] **Step 6: Commit** `feat(ui): organic Leptos shell with pond drop zone, jobs and report drawer`.

### Task 6: CI for tests and all three platform builds

**Files:**
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: everything above. Produces: CI artifacts `fileforge-windows` (NSIS .exe), `fileforge-linux` (AppImage + .deb), `fileforge-android` (debug APK, unsigned; signing is M9).

Jobs:
1. `check` (ubuntu-24.04): apt deps from Task 4, `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --exclude ff-ui`, `wasm-pack test --node crates/ff-ui`, `trunk build`.
2. `desktop` (matrix `windows-latest`, `ubuntu-22.04`; needs `check`): `tauri-apps/tauri-action@v0` with `projectPath: crates/ff-app`, no release, then upload the bundle dir.
3. `android` (ubuntu-24.04; needs `check`): `actions/setup-java` (temurin 17), `android-actions/setup-android`, NDK r27 via `sdkmanager`, the rustup Android targets, `cargo tauri android init --ci` (generated project not committed in M1), `cargo tauri android build --debug --apk`, then upload the APK.

Cache with `Swatinem/rust-cache`.

- [ ] **Step 1:** Write `ci.yml` as above.
- [ ] **Step 2:** `actionlint .github/workflows/ci.yml` → no errors (download the actionlint release binary; if blocked, run `python3 -c "import yaml,sys;yaml.safe_load(open(sys.argv[1]))"` as a minimum check).
- [ ] **Step 3:** Run the `check` job's commands locally → all pass.
- [ ] **Step 4: Commit** `ci: test, Windows/Linux bundles, Android debug APK`.
- [ ] **Step 5:** After the repo is pushed, confirm all three CI jobs go green on GitHub; fix and recommit until they do. The Android job is the most likely to need iteration.

---

## Done when

- `cargo test --workspace` and the wasm tests pass; clippy/fmt clean.
- `cargo tauri dev` on Linux opens the shell, and dropped files appear with their detected kind.
- Screenshots in light/dark/desktop/mobile reviewed against Design Direction.
- CI green on GitHub for check, Windows, Linux and Android.
