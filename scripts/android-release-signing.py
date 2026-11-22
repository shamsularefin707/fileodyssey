#!/usr/bin/env python3
"""Adds release signing to the generated Android project (gen/android is
made by `cargo tauri android init` and isn't kept in git). The keystore is
PKCS12; its path and password come from the environment at build time:
ANDROID_KEYSTORE_PATH, ANDROID_KEYSTORE_PASSWORD, ANDROID_KEY_ALIAS."""
import sys

path = sys.argv[1] if len(sys.argv) > 1 else "crates/fo-app/gen/android/app/build.gradle.kts"
s = open(path, encoding="utf-8").read()
if "signingConfigs" in s:
    sys.exit(f"{path} already has signingConfigs")
for needle in ("    buildTypes {", 'getByName("release") {'):
    if s.count(needle) != 1:
        sys.exit(f"Expected one {needle!r} in {path}; the Tauri template changed")
signing = """    signingConfigs {
        create("release") {
            storeFile = file(System.getenv("ANDROID_KEYSTORE_PATH") ?: error("ANDROID_KEYSTORE_PATH isn't set"))
            storePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
            keyAlias = System.getenv("ANDROID_KEY_ALIAS")
            keyPassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
            storeType = "pkcs12"
        }
    }
"""
s = s.replace("    buildTypes {", signing + "    buildTypes {")
s = s.replace('getByName("release") {', 'getByName("release") {\n            signingConfig = signingConfigs.getByName("release")')
open(path, "w", encoding="utf-8").write(s)
print(f"Release signing added to {path}")
