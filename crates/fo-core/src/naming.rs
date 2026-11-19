//! Output naming: never overwrite an existing file or the input itself.

use std::path::{Path, PathBuf};

/// `stem (converted).ext`, then `stem (converted 2).ext`, `(converted 3)`, …
/// Skips any candidate that exists or equals `input`.
pub fn output_path(input: &Path, out_dir: &Path, ext: &str) -> PathBuf {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_owned());
    let ext = ext.trim_start_matches('.');
    let mut n = 1u32;
    loop {
        let suffix = if n == 1 {
            " (converted)".to_owned()
        } else {
            format!(" (converted {n})")
        };
        let candidate = out_dir.join(format!("{stem}{suffix}.{ext}"));
        if candidate != input && !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}
