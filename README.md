# FileOdyssey

An offline file and image toolkit for Windows, Linux and Android. Compress,
read and convert documents and images on your own device. Your files never
leave it.

> Status: under active development. The first public release will be v1.0.0
> on GitHub Releases.

## Build from source

Requirements: Rust (stable) with the `wasm32-unknown-unknown` target,
[Trunk](https://trunkrs.dev), and the Tauri 2 CLI (`cargo install tauri-cli --version '^2'`).
On Linux you also need `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev libssl-dev`.

The reader uses [PDFium](https://pdfium.googlesource.com/pdfium/) (prebuilt
by [pdfium-binaries](https://github.com/bblanchon/pdfium-binaries)). Fetch it
once into `./pdfium/` before building (tests that read PDFs also need
`qpdf`):

```sh
scripts/fetch-pdfium.sh               # Linux; on Windows: .\scripts\fetch-pdfium.ps1
cargo test --workspace                # core tests
cd crates/fo-app && cargo tauri dev   # run the desktop app
```

Android builds need the Android SDK + NDK; see `.github/workflows/ci.yml`.

## Third-party components

- PDFium — BSD-3-Clause / Apache-2.0 (bundled with the app)
- Fonts: Young Serif, Atkinson Hyperlegible Next, Literata — SIL Open Font
  License 1.1 (`crates/fo-ui/assets/fonts/`)

## Licence

GPL-3.0-or-later, with an additional permission for Google Play services and
Google ML Kit (used by the Android scanners). See [LICENSE](LICENSE).
