//! The contract every engine implements.

use fo_types::{FidelityReport, FileKind, ToolInfo};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// What a tool gets for one input. Tools write only inside `work_dir`; the
/// runner moves the results to their final names.
pub struct Ctx<'a> {
    pub work_dir: &'a Path,
    pub target: Option<&'a str>,
    pub options: &'a serde_json::Value,
    pub cancel: &'a CancelToken,
    pub progress: &'a dyn Fn(f32),
}

pub struct ToolOutput {
    /// (file inside `work_dir`, extension for the final name)
    pub files: Vec<(PathBuf, String)>,
    pub report: FidelityReport,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolError {
    Cancelled,
    Failed(String),
}

impl From<std::io::Error> for ToolError {
    fn from(e: std::io::Error) -> Self {
        ToolError::Failed(e.to_string())
    }
}

pub trait Tool: Send + Sync {
    fn info(&self) -> ToolInfo;
    fn accepts(&self, kind: &FileKind) -> bool;
    fn run(&self, input: &Path, ctx: &Ctx) -> Result<ToolOutput, ToolError>;

    /// A combining tool turns all of a job's inputs into one output
    /// (e.g. images → one PDF) instead of one output per input.
    fn combines(&self) -> bool {
        false
    }

    /// Word used in output names: `name (label).ext`.
    fn output_label(&self) -> &'static str {
        if self.combines() {
            "combined"
        } else {
            "converted"
        }
    }

    fn run_many(&self, _inputs: &[PathBuf], _ctx: &Ctx) -> Result<ToolOutput, ToolError> {
        Err(ToolError::Failed(
            "This tool works on one file at a time".into(),
        ))
    }
}
