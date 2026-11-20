//! Pandoc as a native program: used by tests and the corpus gate, with the
//! same release as the app's `pandoc.wasm`.

use super::{PandocEngine, PandocRequest, PandocResult};
use crate::tool::CancelToken;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

pub struct NativePandoc {
    exe: PathBuf,
}

impl NativePandoc {
    pub fn new(exe: PathBuf) -> Self {
        Self { exe }
    }

    /// `FO_PANDOC`, then `pandoc/native/bin/pandoc` in this checkout.
    pub fn find() -> Option<NativePandoc> {
        let candidates = [
            std::env::var_os("FO_PANDOC").map(PathBuf::from),
            Some(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pandoc/native/bin/pandoc")),
        ];
        candidates
            .into_iter()
            .flatten()
            .find(|p| p.is_file())
            .map(Self::new)
    }
}

fn collect(
    dir: &Path,
    root: &Path,
    skip: &[String],
    out: &mut Vec<(String, Vec<u8>)>,
) -> std::io::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let p = e.path();
        if p.is_dir() {
            collect(&p, root, skip, out)?;
        } else {
            let rel = p
                .strip_prefix(root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            if !skip.contains(&rel) {
                out.push((rel, std::fs::read(&p)?));
            }
        }
    }
    Ok(())
}

impl PandocEngine for NativePandoc {
    fn run(&self, req: PandocRequest, cancel: &CancelToken) -> Result<PandocResult, String> {
        let dir = tempfile::TempDir::new().map_err(|e| e.to_string())?;
        let mut skip = vec!["defaults.json".to_string()];
        for (name, bytes) in &req.files {
            let p = dir.path().join(name);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&p, bytes).map_err(|e| e.to_string())?;
            skip.push(name.clone());
        }
        // JSON is valid YAML, so the options go straight into a defaults file.
        std::fs::write(dir.path().join("defaults.json"), req.options.to_string())
            .map_err(|e| e.to_string())?;
        let mut child = Command::new(&self.exe)
            .args(["--defaults", "defaults.json"])
            .current_dir(dir.path())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Couldn't start pandoc: {e}"))?;
        loop {
            if cancel.is_cancelled() {
                let _ = child.kill();
                return Err("Cancelled".into());
            }
            if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        if !out.status.success() {
            return Err(stderr.trim().to_owned());
        }
        let warnings = stderr
            .lines()
            .filter_map(|l| l.strip_prefix("[WARNING]").map(|w| w.trim().to_owned()))
            .collect();
        let mut files = Vec::new();
        collect(dir.path(), dir.path(), &skip, &mut files).map_err(|e| e.to_string())?;
        Ok(PandocResult {
            stdout: out.stdout,
            stderr,
            files,
            warnings,
        })
    }
}
