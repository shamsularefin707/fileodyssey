//! Extraction safety: path checks and an expansion budget (spec §9).

use crate::tool::ToolError;
use std::path::PathBuf;

pub const MAX_ENTRIES: u64 = 100_000;
const MIN_LIMIT: u64 = 1 << 30;
const MAX_LIMIT: u64 = 16 << 30;
const RATIO: u64 = 250;

/// A relative path that stays inside the destination, or `None`.
/// Rejects absolute paths, drive prefixes (anywhere, since on Windows
/// `push("C:")` replaces the whole path), `..`, NTFS streams (`a:b`),
/// reserved device names (`CON`, `aux.txt`), names ending in a dot or space,
/// control characters and empty names.
pub fn safe_path(name: &str) -> Option<PathBuf> {
    let n = name.replace('\\', "/");
    if n.starts_with('/') {
        return None;
    }
    let mut out = PathBuf::new();
    for part in n.split('/') {
        match part {
            "" | "." => continue,
            ".." => return None,
            p if !safe_component(p) => return None,
            p => out.push(p),
        }
    }
    let all_normal = out
        .components()
        .all(|c| matches!(c, std::path::Component::Normal(_)));
    (all_normal && !out.as_os_str().is_empty()).then_some(out)
}

fn safe_component(p: &str) -> bool {
    const RESERVED: &[&str] = &["CON", "PRN", "AUX", "NUL"];
    if p.chars()
        .any(|c| c.is_control() || matches!(c, ':' | '<' | '>' | '"' | '|' | '?' | '*'))
    {
        return false;
    }
    if p.ends_with('.') || p.ends_with(' ') {
        return false;
    }
    let base = p
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_ascii_uppercase();
    let device = RESERVED.contains(&base.as_str())
        || ((base.starts_with("COM") || base.starts_with("LPT"))
            && base.len() == 4
            && base.as_bytes()[3].is_ascii_digit());
    !device
}

/// Counts bytes actually written; header sizes are never trusted.
#[derive(Debug, Clone)]
pub struct Budget {
    limit: u64,
    used: u64,
}

impl Budget {
    /// `min(16 GiB, max(1 GiB, 250 × archive size))`
    pub fn new(archive_len: u64) -> Self {
        Self::with_limit(
            archive_len
                .saturating_mul(RATIO)
                .clamp(MIN_LIMIT, MAX_LIMIT),
        )
    }

    #[doc(hidden)]
    pub fn with_limit(limit: u64) -> Self {
        Self { limit, used: 0 }
    }

    pub fn limit_bytes(&self) -> u64 {
        self.limit
    }

    pub fn charge(&mut self, n: u64) -> Result<(), ToolError> {
        self.used = self.used.saturating_add(n);
        if self.used > self.limit {
            let size = if self.limit >= 1 << 30 {
                format!("{} GB", self.limit >> 30)
            } else {
                format!("{} MB", (self.limit >> 20).max(1))
            };
            return Err(ToolError::Failed(format!(
                "This archive expands to more than {size}, which looks unsafe"
            )));
        }
        Ok(())
    }
}
