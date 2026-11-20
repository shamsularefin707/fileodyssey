//! File-type detection by content, falling back to the extension only for
//! text formats that have no magic bytes. Reads at most the first 8 KiB, plus
//! the zip central directory for zip-based containers.

use fo_types::{ArchiveFormat, DetectedFile, FileKind, ImageFormat};
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

const SNIFF_LEN: usize = 8 * 1024;

pub fn detect_file(path: &Path) -> io::Result<DetectedFile> {
    let meta = std::fs::metadata(path)?;
    if !meta.is_file() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Not a file"));
    }
    let kind = detect_kind(path)?;
    Ok(DetectedFile {
        path: path.to_string_lossy().into_owned(),
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        size: meta.len(),
        kind,
    })
}

pub fn detect_kind(path: &Path) -> io::Result<FileKind> {
    let mut f = File::open(path)?;
    let mut buf = Vec::with_capacity(SNIFF_LEN);
    f.by_ref().take(SNIFF_LEN as u64).read_to_end(&mut buf)?;
    if buf.is_empty() {
        return Ok(FileKind::Unknown);
    }
    let ext = extension(path);

    if let Some(t) = infer::get(&buf) {
        if let Some(kind) = from_infer(t.extension(), path, &ext) {
            return Ok(kind);
        }
    }
    if let Some(kind) = from_magic(&buf, &ext) {
        return Ok(kind);
    }
    if buf.len() >= 262 && &buf[257..262] == b"ustar" {
        return Ok(FileKind::Archive(ArchiveFormat::Tar));
    }
    Ok(from_text(&buf, &ext))
}

fn extension(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn from_infer(infer_ext: &str, path: &Path, ext: &str) -> Option<FileKind> {
    use ImageFormat as I;
    let img = |f| Some(FileKind::Image(f));
    match infer_ext {
        "pdf" => Some(FileKind::Pdf),
        "png" => img(I::Png),
        "jpg" => img(I::Jpeg),
        "gif" => img(I::Gif),
        "bmp" => img(I::Bmp),
        "ico" => img(I::Ico),
        "tif" => img(I::Tiff),
        "webp" => img(I::Webp),
        "avif" => img(I::Avif),
        "heif" | "heic" => img(I::Heic),
        "jxl" => img(I::Jxl),
        "psd" => img(I::Psd),
        "exr" => img(I::Exr),
        "cr2" | "cr3" | "nef" | "arw" | "dng" | "orf" | "rw2" | "raf" => img(I::Raw),
        "rtf" => Some(FileKind::Rtf),
        "7z" => Some(FileKind::Archive(ArchiveFormat::SevenZ)),
        "rar" => Some(FileKind::Archive(ArchiveFormat::Rar)),
        "xz" => Some(FileKind::Archive(ArchiveFormat::TarXz)),
        "zst" => Some(FileKind::Archive(ArchiveFormat::TarZst)),
        "gz" => Some(FileKind::Archive(ArchiveFormat::TarGz)),
        "tar" => Some(FileKind::Archive(ArchiveFormat::Tar)),
        // Zip-based containers: decide by the entries, never by infer's guess.
        "zip" | "docx" | "xlsx" | "pptx" | "epub" | "odt" | "ods" | "odp" | "jar" | "apk" => {
            Some(from_zip(path))
        }
        // OLE compound files (legacy Office): the container doesn't say which app.
        "doc" | "xls" | "ppt" | "msi" => Some(match ext {
            "xls" => FileKind::Xls,
            "ppt" => FileKind::Ppt,
            _ => FileKind::Doc,
        }),
        _ => None,
    }
}

/// Image formats `infer` doesn't know.
fn from_magic(buf: &[u8], ext: &str) -> Option<FileKind> {
    use ImageFormat as I;
    let img = |f| Some(FileKind::Image(f));
    if buf.starts_with(b"qoif") {
        return img(I::Qoi);
    }
    if buf.starts_with(b"DDS ") {
        return img(I::Dds);
    }
    if buf.starts_with(b"#?RADIANCE") || buf.starts_with(b"#?RGBE") {
        return img(I::Hdr);
    }
    if buf.len() > 2
        && buf[0] == b'P'
        && (b'1'..=b'7').contains(&buf[1])
        && buf[2].is_ascii_whitespace()
    {
        return img(I::Pnm);
    }
    if ext == "tga" || ext == "tpic" {
        // TGA has no magic number; trust the extension if the header is plausible.
        let plausible = buf.len() >= 18 && matches!(buf[2], 1 | 2 | 3 | 9 | 10 | 11);
        if plausible {
            return img(I::Tga);
        }
    }
    None
}

fn from_zip(path: &Path) -> FileKind {
    let fallback = FileKind::Archive(ArchiveFormat::Zip);
    let Ok(file) = File::open(path) else {
        return fallback;
    };
    let Ok(mut zip) = zip::ZipArchive::new(file) else {
        return fallback;
    };

    let mimetype = zip.by_name("mimetype").ok().and_then(|e| {
        let mut s = String::new();
        e.take(128).read_to_string(&mut s).ok().map(|_| s)
    });
    if let Some(m) = mimetype.as_deref().map(str::trim) {
        match m {
            "application/epub+zip" => return FileKind::Epub,
            "application/vnd.oasis.opendocument.text" => return FileKind::Odt,
            _ => {}
        }
    }

    let names: Vec<String> = zip.file_names().map(str::to_owned).collect();
    if names.iter().any(|n| n == "[Content_Types].xml") {
        let has = |prefix: &str| names.iter().any(|n| n.starts_with(prefix));
        if has("word/") {
            return FileKind::Docx;
        }
        if has("ppt/") {
            return FileKind::Pptx;
        }
        if has("xl/") {
            return FileKind::Xlsx;
        }
    }
    fallback
}

fn from_text(buf: &[u8], ext: &str) -> FileKind {
    // A sniff window may cut a multi-byte character; ignore an incomplete tail.
    let text = match std::str::from_utf8(buf) {
        Ok(s) => s,
        Err(e) if e.error_len().is_none() && buf.len() >= SNIFF_LEN => {
            std::str::from_utf8(&buf[..e.valid_up_to()]).unwrap_or_default()
        }
        Err(_) => return FileKind::Unknown,
    };
    if text.contains('\0') {
        return FileKind::Unknown;
    }
    if text.trim_start().starts_with("{\\rtf") {
        return FileKind::Rtf;
    }
    match ext {
        "md" | "markdown" => return FileKind::Markdown,
        "html" | "htm" | "xhtml" => return FileKind::Html,
        _ => {}
    }
    match root_element(text) {
        Some(r) if r.starts_with("<svg") => return FileKind::Image(ImageFormat::Svg),
        Some(r) if r.to_ascii_lowercase().starts_with("<html") => return FileKind::Html,
        _ => {}
    }
    match ext {
        "md" | "markdown" => FileKind::Markdown,
        "html" | "htm" | "xhtml" => FileKind::Html,
        "csv" => FileKind::Csv,
        "json" => FileKind::Json,
        "tex" | "latex" | "ltx" => FileKind::Latex,
        "rst" | "rest" => FileKind::Rst,
        "org" => FileKind::Org,
        "adoc" | "asciidoc" | "asc" => FileKind::AsciiDoc,
        _ => FileKind::Text,
    }
}

/// The first element tag, skipping the XML declaration, comments and doctype.
/// `<!doctype html>` itself counts as an HTML root.
fn root_element(text: &str) -> Option<&str> {
    let mut rest = text.trim_start_matches('\u{feff}').trim_start();
    loop {
        if !rest.starts_with('<') {
            return None;
        }
        let lower: String = rest
            .chars()
            .take(15)
            .collect::<String>()
            .to_ascii_lowercase();
        if lower.starts_with("<!doctype html") {
            return Some("<html");
        }
        let end = if rest.starts_with("<!--") {
            rest.find("-->").map(|i| i + 3)?
        } else if rest.starts_with("<?") || rest.starts_with("<!") {
            rest.find('>').map(|i| i + 1)?
        } else {
            return Some(rest);
        };
        rest = rest[end..].trim_start();
    }
}
