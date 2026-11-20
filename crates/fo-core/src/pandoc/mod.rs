//! Document conversion through Pandoc.
//!
//! Pandoc itself runs elsewhere: in the app it is the official `pandoc.wasm`
//! inside a Web Worker (see `fo-app/src/pandoc.rs`); in tests it is the same
//! release as a native binary. Both speak `PandocRequest`/`PandocResult`,
//! whose `options` are Pandoc's own defaults-file keys (`from`, `to`,
//! `input-files`, `output-file`, `extract-media`, `standalone`, …).

pub mod formats;
pub mod frame;
mod native;

pub use native::NativePandoc;

use crate::tool::CancelToken;

#[derive(Debug, Clone, PartialEq)]
pub struct PandocRequest {
    pub options: serde_json::Value,
    /// Input files by name (relative, no folders except `media/…`).
    pub files: Vec<(String, Vec<u8>)>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PandocResult {
    pub stdout: Vec<u8>,
    pub stderr: String,
    /// Files Pandoc wrote: the `output-file` and any extracted media.
    pub files: Vec<(String, Vec<u8>)>,
    pub warnings: Vec<String>,
}

impl PandocResult {
    pub fn file(&self, name: &str) -> Option<&[u8]> {
        self.files
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, b)| b.as_slice())
    }
}

pub trait PandocEngine: Send + Sync {
    /// Runs one conversion. `Err` carries Pandoc's message.
    fn run(&self, req: PandocRequest, cancel: &CancelToken) -> Result<PandocResult, String>;
}
