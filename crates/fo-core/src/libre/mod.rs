//! LibreOffice (desktop, optional): finding `soffice` and running one
//! headless conversion in a job's work folder.

pub mod tool;

use crate::tool::CancelToken;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Where LibreOffice can be downloaded; the only page the app opens for it.
pub const DOWNLOAD_URL: &str = "https://www.libreoffice.org/download/download-libreoffice/";

/// Names of the program on PATH.
fn program_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["soffice.exe", "soffice.com"]
    } else {
        &["soffice", "libreoffice"]
    }
}

/// Standard install locations, most likely first.
fn install_folders() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if cfg!(windows) {
        for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
            if let Some(dir) = std::env::var_os(var) {
                v.push(PathBuf::from(dir).join("LibreOffice/program/soffice.exe"));
            }
        }
    } else {
        for p in [
            "/usr/bin/soffice",
            "/usr/local/bin/soffice",
            "/usr/lib/libreoffice/program/soffice",
            "/usr/lib64/libreoffice/program/soffice",
            "/snap/bin/libreoffice",
            "/var/lib/flatpak/exports/bin/org.libreoffice.LibreOffice",
        ] {
            v.push(PathBuf::from(p));
        }
        if let Ok(rd) = std::fs::read_dir("/opt") {
            let mut opt: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with("libreoffice"))
                })
                .map(|p| p.join("program/soffice"))
                .collect();
            // Newest version folder first.
            opt.sort();
            opt.reverse();
            v.extend(opt);
        }
        if let Some(home) = std::env::var_os("HOME") {
            v.push(
                PathBuf::from(home)
                    .join(".local/share/flatpak/exports/bin/org.libreoffice.LibreOffice"),
            );
        }
    }
    v
}

fn runnable(p: &Path) -> bool {
    p.is_file()
}

/// `soffice` from an explicit override, else PATH, else `extra` folders.
pub fn find_in(
    env_override: Option<&OsStr>,
    path_var: Option<&OsString>,
    extra: &[PathBuf],
) -> Option<PathBuf> {
    if let Some(p) = env_override.map(PathBuf::from) {
        if runnable(&p) {
            return Some(p);
        }
    }
    if let Some(path) = path_var {
        for dir in std::env::split_paths(path) {
            for name in program_names() {
                let p = dir.join(name);
                if runnable(&p) {
                    return Some(p);
                }
            }
        }
    }
    extra.iter().find(|p| runnable(p)).cloned()
}

/// The installed LibreOffice, looked up afresh each call (cheap: a few
/// file checks), so installing it while the app runs turns its tools on.
pub fn find_soffice() -> Option<PathBuf> {
    find_in(
        std::env::var_os("FO_SOFFICE").as_deref(),
        std::env::var_os("PATH").as_ref(),
        &install_folders(),
    )
}

/// A `file://` URL for a local folder, as LibreOffice's `-env:` wants.
fn file_url(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let mut out = String::from(if s.starts_with('/') {
        "file://"
    } else {
        "file:///"
    });
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~:".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn kill_tree(child: &mut Child) {
    #[cfg(unix)]
    {
        // The wrapper script starts soffice.bin in the same process group.
        let pgid = child.id() as i32;
        // SAFETY: plain kill(2) on our own child's process group.
        unsafe {
            libc::kill(-pgid, libc::SIGKILL);
        }
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &child.id().to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Converts `input` with LibreOffice into `work/out/input.<ext>` and returns
/// that path. The input is copied to `work/in/input.<its ext>` first (so odd
/// names can't reach the command line), and LibreOffice runs with its own
/// profile in `work/profile`, so a LibreOffice the user has open isn't
/// disturbed. Killed when `limit` passes or `cancel` fires.
pub fn convert(
    soffice: &Path,
    input: &Path,
    filter: &str,
    ext: &str,
    work: &Path,
    cancel: &CancelToken,
    limit: Duration,
) -> Result<PathBuf, String> {
    let io = |e: std::io::Error| format!("Couldn't prepare the conversion: {e}");
    let (dir_in, dir_out, profile) = (work.join("in"), work.join("out"), work.join("profile"));
    for d in [&dir_in, &dir_out, &profile] {
        std::fs::create_dir_all(d).map_err(io)?;
    }
    let in_ext = input
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let copy = dir_in.join(format!("input.{in_ext}"));
    std::fs::copy(input, &copy).map_err(io)?;
    let copy = copy.canonicalize().map_err(io)?;
    let dir_out = dir_out.canonicalize().map_err(io)?;
    let profile = profile.canonicalize().map_err(io)?;
    let log = work.join("soffice.log");
    let mut cmd = Command::new(soffice);
    cmd.arg(format!("-env:UserInstallation={}", file_url(&profile)))
        .args([
            "--headless",
            "--norestore",
            "--nologo",
            "--nodefault",
            "--nolockcheck",
            "--convert-to",
            filter,
            "--outdir",
        ])
        .arg(&dir_out)
        .arg(&copy)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(&log).map_err(io)?);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Couldn't start LibreOffice: {e}"))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {}
            Err(e) => {
                kill_tree(&mut child);
                return Err(format!("LibreOffice stopped unexpectedly: {e}"));
            }
        }
        if cancel.is_cancelled() {
            kill_tree(&mut child);
            return Err("Cancelled".into());
        }
        if start.elapsed() > limit {
            kill_tree(&mut child);
            return Err(format!(
                "LibreOffice took too long (over {} s) and was stopped",
                limit.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let out = dir_out.join(format!("input.{ext}"));
    match std::fs::metadata(&out) {
        Ok(m) if m.len() > 0 => Ok(out),
        _ => {
            let detail = std::fs::read_to_string(&log).unwrap_or_default();
            let detail = detail.trim().lines().last().unwrap_or("").trim().to_owned();
            Err(if detail.is_empty() {
                "LibreOffice couldn't convert this file".into()
            } else {
                format!("LibreOffice couldn't convert this file ({detail})")
            })
        }
    }
}
