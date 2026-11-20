//! Safe extraction of zip, 7z, tar and compressed tar archives.

use super::guard::{safe_path, Budget, MAX_ENTRIES};
use crate::tool::{CancelToken, ToolError};
use fo_types::{ArchiveFormat, FileKind};
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::Path;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ExtractReport {
    /// Regular files written.
    pub files: u64,
    /// Bytes written.
    pub bytes: u64,
    /// Every entry seen, including folders and skipped ones.
    pub entries: u64,
    pub skipped_unsafe: u64,
    pub skipped_links: u64,
    pub skipped_unsupported: u64,
    /// Entries whose name was already used (case-insensitively) and were
    /// written as `name (2).ext` instead of overwriting.
    pub renamed: u64,
}

impl ExtractReport {
    pub fn file_entries(&self) -> u64 {
        self.files + self.skipped_unsafe + self.skipped_links + self.skipped_unsupported
    }
}

fn unreadable(e: impl std::fmt::Display) -> ToolError {
    ToolError::Failed(format!("Couldn't read this archive: {e}"))
}

struct Sink<'a> {
    dest: &'a Path,
    budget: Budget,
    /// For tar formats the decompressed stream itself is charged (it also
    /// covers metadata records), so writes aren't charged twice.
    charge_writes: bool,
    cancel: &'a CancelToken,
    report: ExtractReport,
    /// Lowercased relative paths already written.
    used: HashSet<String>,
}

/// Charges every byte read to a budget, so records the tar crate buffers
/// internally (GNU long names, PAX headers) can't be used as a bomb.
struct Charged<'a, R> {
    inner: R,
    budget: Budget,
    cancel: &'a CancelToken,
}

impl<R: Read> Read for Charged<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.cancel.is_cancelled() {
            return Err(std::io::Error::other(CANCELLED));
        }
        let n = self.inner.read(buf)?;
        self.budget.charge(n as u64).map_err(|e| match e {
            ToolError::Failed(m) => std::io::Error::other(m),
            ToolError::Cancelled => std::io::Error::other(CANCELLED),
        })?;
        Ok(n)
    }
}

const CANCELLED: &str = "cancelled";

/// Turns an I/O error raised by `Charged` back into the budget/cancel error.
fn from_io(e: std::io::Error) -> ToolError {
    let msg = e.to_string();
    if msg == CANCELLED {
        ToolError::Cancelled
    } else if msg.starts_with("This archive expands") {
        ToolError::Failed(msg)
    } else {
        unreadable(e)
    }
}

impl Sink<'_> {
    fn seen(&mut self) -> Result<(), ToolError> {
        self.report.entries += 1;
        if self.report.entries > MAX_ENTRIES {
            return Err(ToolError::Failed(format!(
                "This archive has more than {MAX_ENTRIES} entries, which looks unsafe"
            )));
        }
        if self.cancel.is_cancelled() {
            return Err(ToolError::Cancelled);
        }
        Ok(())
    }

    fn dir(&mut self, name: &str) -> Result<(), ToolError> {
        match safe_path(name) {
            Some(rel) => std::fs::create_dir_all(self.dest.join(rel)).map_err(ToolError::from),
            None => Ok(()), // an unsafe folder entry writes nothing; its files are checked on their own
        }
    }

    /// `rel`, or `stem (2).ext`… if that name (ignoring case) was already written.
    fn unique(&mut self, rel: std::path::PathBuf) -> std::path::PathBuf {
        let key = |p: &Path| p.to_string_lossy().to_lowercase();
        if self.used.insert(key(&rel)) {
            return rel;
        }
        self.report.renamed += 1;
        let parent = rel.parent().map(Path::to_path_buf).unwrap_or_default();
        let name = rel
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (stem, ext) = match name.rsplit_once('.') {
            Some((s, e)) if !s.is_empty() => (s.to_owned(), format!(".{e}")),
            _ => (name.clone(), String::new()),
        };
        (2..)
            .map(|n| parent.join(format!("{stem} ({n}){ext}")))
            .find(|c| self.used.insert(key(c)))
            .expect("a free name exists")
    }

    fn file(&mut self, name: &str, mut data: impl Read) -> Result<(), ToolError> {
        let Some(rel) = safe_path(name) else {
            self.report.skipped_unsafe += 1;
            return Ok(());
        };
        let rel = self.unique(rel);
        let path = self.dest.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = File::create(&path)?;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = data.read(&mut buf).map_err(from_io)?;
            if n == 0 {
                break;
            }
            if self.charge_writes {
                self.budget.charge(n as u64)?;
            }
            out.write_all(&buf[..n])?;
            self.report.bytes += n as u64;
            if self.cancel.is_cancelled() {
                return Err(ToolError::Cancelled);
            }
        }
        self.report.files += 1;
        Ok(())
    }
}

pub fn extract(
    path: &Path,
    kind: &FileKind,
    dest: &Path,
    budget: Budget,
    cancel: &CancelToken,
) -> Result<ExtractReport, ToolError> {
    let is_tar = matches!(
        kind,
        FileKind::Archive(
            ArchiveFormat::Tar
                | ArchiveFormat::TarGz
                | ArchiveFormat::TarXz
                | ArchiveFormat::TarZst
        )
    );
    let stream_budget = budget.clone();
    let mut sink = Sink {
        dest,
        budget,
        charge_writes: !is_tar,
        cancel,
        report: ExtractReport::default(),
        used: HashSet::new(),
    };
    let open = || File::open(path).map(BufReader::new).map_err(unreadable);
    let charged = |r: Box<dyn Read>| Charged {
        inner: r,
        budget: stream_budget.clone(),
        cancel,
    };
    match kind {
        FileKind::Archive(ArchiveFormat::Zip) => zip_entries(open()?, &mut sink)?,
        FileKind::Archive(ArchiveFormat::Tar) => {
            tar_entries(charged(Box::new(open()?)), &mut sink)?
        }
        FileKind::Archive(ArchiveFormat::TarGz) => tar_entries(
            charged(Box::new(flate2::read::MultiGzDecoder::new(open()?))),
            &mut sink,
        )?,
        FileKind::Archive(ArchiveFormat::TarXz) => tar_entries(
            charged(Box::new(lzma_rust2::XzReader::new(open()?, true))),
            &mut sink,
        )?,
        FileKind::Archive(ArchiveFormat::TarZst) => {
            let dec = ruzstd::decoding::StreamingDecoder::new(open()?).map_err(unreadable)?;
            tar_entries(charged(Box::new(dec)), &mut sink)?
        }
        FileKind::Archive(ArchiveFormat::SevenZ) => seven_entries(path, &mut sink)?,
        _ => {
            return Err(ToolError::Failed(
                "This kind of archive isn't supported".into(),
            ))
        }
    }
    Ok(sink.report)
}

fn zip_entries(reader: impl Read + std::io::Seek, sink: &mut Sink) -> Result<(), ToolError> {
    let mut zip = zip::ZipArchive::new(reader).map_err(unreadable)?;
    for i in 0..zip.len() {
        sink.seen()?;
        let entry = match zip.by_index(i) {
            Ok(e) => e,
            // Encrypted entries and unsupported methods (bzip2, zstd, PPMd, AES).
            Err(_) => {
                sink.report.skipped_unsupported += 1;
                continue;
            }
        };
        let name = String::from_utf8_lossy(entry.name_raw()).into_owned();
        if entry.is_symlink() {
            sink.report.skipped_links += 1;
        } else if entry.is_dir() {
            sink.dir(&name)?;
        } else {
            sink.file(&name, entry)?;
        }
    }
    Ok(())
}

fn tar_entries(reader: impl Read, sink: &mut Sink) -> Result<(), ToolError> {
    let mut archive = tar::Archive::new(reader);
    for entry in archive.entries().map_err(from_io)? {
        let entry = entry.map_err(from_io)?;
        // Archive-wide metadata (e.g. the pax_global_header of every GitHub
        // tarball) isn't content: don't count it.
        if entry.header().entry_type() == tar::EntryType::XGlobalHeader {
            continue;
        }
        sink.seen()?;
        let name = String::from_utf8_lossy(&entry.path_bytes()).into_owned();
        match entry.header().entry_type() {
            tar::EntryType::Regular | tar::EntryType::Continuous => sink.file(&name, entry)?,
            tar::EntryType::Directory => sink.dir(&name)?,
            tar::EntryType::GNUSparse => sink.report.skipped_unsupported += 1,
            // Pax/GNU metadata records are consumed by `entries()`; anything
            // else (links, devices, FIFOs) is never written.
            _ => sink.report.skipped_links += 1,
        }
    }
    Ok(())
}

fn seven_entries(path: &Path, sink: &mut Sink) -> Result<(), ToolError> {
    let password_msg =
        || ToolError::Failed("Password-protected archives aren't supported yet".into());
    let mut reader = sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty())
        .map_err(|e| {
            if format!("{e:?}").to_lowercase().contains("password") {
                password_msg()
            } else {
                unreadable(e)
            }
        })?;
    let mut failure: Option<ToolError> = None;
    let result = reader.for_each_entries(|entry, data| {
        let step = sink.seen().and_then(|_| {
            if entry.is_directory() {
                sink.dir(entry.name())
            } else if entry.has_stream {
                sink.file(entry.name(), data)
            } else {
                sink.file(entry.name(), std::io::empty())
            }
        });
        match step {
            Ok(()) => Ok(true),
            Err(e) => {
                failure = Some(e);
                Ok(false)
            }
        }
    });
    if let Some(e) = failure {
        return Err(e);
    }
    result.map_err(|e| {
        if format!("{e:?}").to_lowercase().contains("password") {
            password_msg()
        } else {
            unreadable(e)
        }
    })
}
