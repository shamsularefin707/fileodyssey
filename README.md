# FileOdyssey

An offline file and image toolkit for Windows, Linux and Android. Compress,
read and convert documents and images on your own device. Your files never
leave it.

## What it does

- **Convert documents**: Markdown, Word, OpenDocument, HTML, EPUB, RTF,
  LaTeX, plain text and more, with a report of what was kept every time.
- **PDF to Word, Markdown, HTML and more**: headings, lists, tables,
  footnotes and images rebuilt; Word output keeps fonts and colours.
- **With LibreOffice installed (desktop)**: old .doc/.xls/.ppt files, and
  Office files to PDF with their exact layout.
- **Images**: convert, compress, resize, read QR codes, combine into a PDF.
- **Compress**: archives, PDFs and Office files.
- **Read**: PDFs, e-books, Word documents and images.
- **Android**: scan documents and QR codes with the camera.

## Download

Get the latest version from the
[Releases page](https://github.com/shamsularefin707/fileodyssey/releases/latest).

| You have | Download |
|---|---|
| Windows 10 or 11 | `…_x64-setup.exe` (or the `.msi` if you prefer) |
| Ubuntu, Debian, Mint | `…_amd64.deb` |
| Any other Linux | `…_amd64.AppImage` |
| Android phone or tablet (most) | `…_android-arm64-v8a.apk` |
| Older 32-bit Android | `…_android-armeabi-v7a.apk` |
| Android emulator / Chromebook (Intel) | `…_android-x86_64.apk` |

**Windows** shows a "Windows protected your PC" (SmartScreen) message,
because the installer isn't code-signed (that costs money each year).
Choose **More info → Run anyway**.

**Linux AppImage**: make it executable (`chmod +x FileOdyssey_*.AppImage`)
and run it. The `.deb` installs with `sudo apt install ./FileOdyssey_*.deb`.

**Android**: open the APK and allow installing from your browser or file
manager when asked. The document scanner needs Google Play services; QR
scanning and everything else work without it.

**LibreOffice** (optional, desktop): install it from
[libreoffice.org](https://www.libreoffice.org/download/download-libreoffice/)
to turn on old Office formats and exact-layout PDF output.

**Checking a download**: each release has a `SHA256SUMS` file. On Linux,
`sha256sum -c SHA256SUMS --ignore-missing`; on Windows,
`Get-FileHash .\FileOdyssey_*-setup.exe` and compare. Android APKs are signed
with the certificate whose SHA-256 is in
[docs/android-signing-cert.txt](docs/android-signing-cert.txt).

## Privacy and updates

FileOdyssey works offline. The only time it uses the internet is to look
for a new version when it starts, and you can turn that off under
**About FileOdyssey**. On Windows and Linux an update installs when you
choose "Update and restart"; on Android it opens the release page to
download.

## Build from source

Requirements: Rust (stable) with the `wasm32-unknown-unknown` target,
[Trunk](https://trunkrs.dev), and the Tauri 2 CLI (`cargo install tauri-cli --version '^2'`).
On Linux you also need `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev libssl-dev`.

The reader uses [PDFium](https://pdfium.googlesource.com/pdfium/) (prebuilt
by [pdfium-binaries](https://github.com/bblanchon/pdfium-binaries)). Fetch it
once into `./pdfium/` before building (tests that read PDFs also need
`qpdf`):

Document conversion uses the official [Pandoc](https://pandoc.org) 3.10
WebAssembly build, which runs inside the app's webview. Fetch it (and, for
tests, the matching native Pandoc) once as well:

```sh
scripts/fetch-pdfium.sh               # Linux; on Windows: .\scripts\fetch-pdfium.ps1
scripts/fetch-pandoc.sh --native      # on Windows: .\scripts\fetch-pandoc.ps1 (wasm only)
cargo test --workspace                # core tests
cd crates/fo-app && cargo tauri dev   # run the desktop app
```

Android builds need the Android SDK + NDK; see `.github/workflows/ci.yml`.
How releases are made: [docs/RELEASING.md](docs/RELEASING.md).

## Third-party components

- PDFium — BSD-3-Clause / Apache-2.0 (bundled with the app)
- Pandoc 3.10 (`pandoc.wasm`) — GPL-2.0-or-later, © John MacFarlane and
  contributors (bundled; see `crates/fo-ui/assets/pandoc/PANDOC-COPYRIGHT.txt`)
- pandoc-wasm glue and @bjorn3/browser_wasi_shim — MIT / Apache-2.0
  (`crates/fo-ui/pandoc-worker/`)
- Fonts: Young Serif, Atkinson Hyperlegible Next, Literata — SIL Open Font
  License 1.1 (`crates/fo-ui/assets/fonts/`)
- Android only: Google ML Kit Document Scanner and Google Play services
  (Google's terms; allowed by the licence's additional permission) and the
  Tauri barcode-scanner plugin (MIT / Apache-2.0, using CameraX and ML Kit
  barcode scanning)
- LibreOffice is not bundled; FileOdyssey uses it only if you install it

## Licence

GPL-3.0-or-later, with an additional permission for Google Play services and
Google ML Kit (used by the Android scanners). See [LICENSE](LICENSE).
