//! Extraction safety: path checks and an expansion budget (spec §9).

use crate::tool::ToolError;
use std::path::PathBuf;

pub const MAX_ENTRIES: u64 = 100_000;
const MIN_LIMIT: u64 = 1 << 30;
const MAX_LIMIT: u64 = 16 << 30;
const RATIO: u64 = 250;

/// A relative path that stays inside the destination, or `None`.
/// Rejects absolute paths, drive prefixes, `..`, NULs and empty names.
pub fn safe_path(name: &str) -> Option<PathBuf> {
    let n = name.replace('\\', "/");
    let bytes = n.as_bytes();
    if n.starts_with('/') || (bytes.len() >= 2 && bytes[1] == b':') {
        return None;
    }
    let mut out = PathBuf::new();
    for part in n.split('/') {
        match part {
            "" | "." => continue,
            ".." => return None,
            p if p.contains('\0') => return None,
            p => out.push(p),
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
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
