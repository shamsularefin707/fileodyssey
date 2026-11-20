//! Output naming: never overwrite an existing file or the input itself.

use std::path::{Path, PathBuf};

/// `stem (converted).ext`, then `stem (converted 2).ext`, `(converted 3)`, …
/// Skips any candidate that exists or equals `input`.
pub fn output_path(input: &Path, out_dir: &Path, ext: &str) -> PathBuf {
    output_path_with(input, out_dir, ext, "converted")
}

/// Like `output_path`, with another label: `stem (label).ext`, `stem (label 2).ext`, …
pub fn output_path_with(input: &Path, out_dir: &Path, ext: &str, label: &str) -> PathBuf {
    let stem = stem_of(input);
    let ext = ext.trim_start_matches('.');
    let mut n = 1u32;
    loop {
        let suffix = if n == 1 {
            format!(" ({label})")
        } else {
            format!(" ({label} {n})")
        };
        // An empty extension names a folder.
        let name = if ext.is_empty() {
            format!("{stem}{suffix}")
        } else {
            format!("{stem}{suffix}.{ext}")
        };
        let candidate = out_dir.join(name);
        if candidate != input && !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

/// File name without its extension; `photos.tar.gz` → `photos`.
fn stem_of(input: &Path) -> String {
    let name = input
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_owned());
    let lower = name.to_ascii_lowercase();
    for double in [".tar.gz", ".tar.xz", ".tar.zst", ".tar.bz2"] {
        if lower.ends_with(double) && name.len() > double.len() {
            return name[..name.len() - double.len()].to_owned();
        }
    }
    input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or(name)
}
