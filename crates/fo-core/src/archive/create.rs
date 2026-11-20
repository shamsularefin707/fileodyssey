//! Creating archives from a list of files.

use crate::tool::{CancelToken, ToolError};
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

pub const FORMATS: &[&str] = &["zip", "7z", "tar", "tar.gz", "tar.xz", "tar.zst"];

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CreateReport {
    pub entries: u64,
    /// Inputs that shared a name and were stored as `name (2).ext`.
    pub renamed: u64,
    pub input_bytes: u64,
}

fn failed(e: impl std::fmt::Display) -> ToolError {
    ToolError::Failed(format!("Couldn't create the archive: {e}"))
}

/// Unique entry names: the file name, then `stem (2).ext`, `stem (3).ext`…
fn entry_names(inputs: &[PathBuf]) -> (Vec<String>, u64) {
    let mut used = HashSet::new();
    let mut renamed = 0;
    let names = inputs
        .iter()
        .map(|p| {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file".into());
            if used.insert(name.to_lowercase()) {
                return name;
            }
            renamed += 1;
            let (stem, ext) = match name.rsplit_once('.') {
                Some((s, e)) if !s.is_empty() => (s.to_owned(), format!(".{e}")),
                _ => (name.clone(), String::new()),
            };
            (2..)
                .map(|n| format!("{stem} ({n}){ext}"))
                .find(|c| used.insert(c.to_lowercase()))
                .expect("a free name exists")
        })
        .collect();
    (names, renamed)
}

pub fn create(
    inputs: &[PathBuf],
    format: &str,
    level: u32,
    out: &Path,
    cancel: &CancelToken,
) -> Result<CreateReport, ToolError> {
    let level = level.clamp(1, 9);
    let (names, renamed) = entry_names(inputs);
    let mut input_bytes = 0;
    for p in inputs {
        input_bytes += std::fs::metadata(p)?.len();
    }
    let check = || {
        if cancel.is_cancelled() {
            Err(ToolError::Cancelled)
        } else {
            Ok(())
        }
    };
    match format {
        "zip" => {
            let mut z = zip::ZipWriter::new(BufWriter::new(File::create(out)?));
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .compression_level(Some(level as i64))
                .large_file(input_bytes > u32::MAX as u64);
            for (p, name) in inputs.iter().zip(&names) {
                check()?;
                z.start_file(name.as_str(), opts).map_err(failed)?;
                std::io::copy(&mut BufReader::new(File::open(p)?), &mut z)?;
            }
            z.finish().map_err(failed)?.flush()?;
        }
        "7z" => {
            let mut w = sevenz_rust2::ArchiveWriter::create(out).map_err(failed)?;
            w.set_content_methods(vec![
                sevenz_rust2::encoder_options::Lzma2Options::from_level(level).into(),
            ]);
            for (p, name) in inputs.iter().zip(&names) {
                check()?;
                let entry = sevenz_rust2::ArchiveEntry::from_path(p, name.clone());
                w.push_archive_entry(entry, Some(BufReader::new(File::open(p)?)))
                    .map_err(failed)?;
            }
            w.finish().map_err(failed)?;
        }
        "tar" => {
            write_tar(inputs, &names, BufWriter::new(File::create(out)?), &check)?.flush()?;
        }
        "tar.gz" => {
            let gz = flate2::write::GzEncoder::new(
                BufWriter::new(File::create(out)?),
                flate2::Compression::new(level),
            );
            write_tar(inputs, &names, gz, &check)?.finish()?.flush()?;
        }
        "tar.xz" => {
            let xz = lzma_rust2::XzWriter::new(
                BufWriter::new(File::create(out)?),
                lzma_rust2::XzOptions::with_preset(level),
            )
            .map_err(failed)?;
            write_tar(inputs, &names, xz, &check)?
                .finish()
                .map_err(failed)?
                .flush()?;
        }
        "tar.zst" => {
            // ruzstd only implements its "Fastest" level; stream tar → zstd via
            // a temporary tar next to the output (same work dir).
            let tmp = out.with_extension("tmp-tar");
            write_tar(inputs, &names, BufWriter::new(File::create(&tmp)?), &check)?.flush()?;
            check()?;
            let mut c =
                ruzstd::encoding::FrameCompressor::new(ruzstd::encoding::CompressionLevel::Fastest);
            c.set_source(BufReader::new(File::open(&tmp)?));
            let mut sink = BufWriter::new(File::create(out)?);
            c.set_drain(&mut sink);
            c.compress();
            drop(c);
            sink.flush()?;
            let _ = std::fs::remove_file(&tmp);
        }
        other => return Err(ToolError::Failed(format!("Can't create .{other} archives"))),
    }
    Ok(CreateReport {
        entries: inputs.len() as u64,
        renamed,
        input_bytes,
    })
}

fn write_tar<W: Write>(
    inputs: &[PathBuf],
    names: &[String],
    w: W,
    check: &dyn Fn() -> Result<(), ToolError>,
) -> Result<W, ToolError> {
    let mut b = tar::Builder::new(w);
    b.mode(tar::HeaderMode::Deterministic);
    for (p, name) in inputs.iter().zip(names) {
        check()?;
        b.append_path_with_name(p, name).map_err(failed)?;
    }
    b.into_inner().map_err(failed)
}
