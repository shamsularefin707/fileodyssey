# Releasing FileOdyssey

Releases are built by `.github/workflows/release.yml`. Nothing is published
until every file is in place.

## One-time setup: signing secrets

In the repository's Settings → Secrets and variables → Actions:

| Secret | What it is |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | Signs desktop updates (minisign key from `cargo tauri signer generate`). Its public half is `plugins.updater.pubkey` in `crates/fo-app/tauri.conf.json`. |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | That key's password. |
| `ANDROID_KEYSTORE_BASE64` | The Android release keystore (PKCS12), base64. Its certificate's SHA-256 is in `docs/android-signing-cert.txt`; the workflow checks every APK against it. |
| `ANDROID_KEYSTORE_PASSWORD` | The keystore's password (also the key's). |
| `ANDROID_KEY_ALIAS` | `fileodyssey` |

Keep backups of both keys. Losing the updater key means installed copies
can't update themselves; losing the keystore means Android can't install a
new version over the old one.

## Making a release

1. Set the version in `Cargo.toml` (workspace) and `crates/fo-app/tauri.conf.json`.
2. Write `docs/release-notes/vX.Y.Z.md`.
3. Optional trial: run the Release workflow by hand (Actions → Release → Run
   workflow). It builds everything into a **draft** release; check it, then
   delete the draft.
4. Tag and push: `git tag vX.Y.Z && git push origin vX.Y.Z`. The workflow
   builds the Windows installers (NSIS, MSI), Linux AppImage and .deb,
   signed Android APKs (arm64-v8a, armeabi-v7a, x86_64), writes
   `SHA256SUMS` and the updater's `latest.json`, then publishes.

Installed desktop copies find the new version through
`releases/latest/download/latest.json`; Android copies through GitHub's API.
