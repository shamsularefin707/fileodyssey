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

```sh
cargo test --workspace          # core tests
cd crates/fo-app && cargo tauri dev   # run the desktop app
```

Android builds need the Android SDK + NDK; see `.github/workflows/ci.yml`.

## Licence

GPL-3.0-or-later, with an additional permission for Google Play services and
Google ML Kit (used by the Android scanners). See [LICENSE](LICENSE).
