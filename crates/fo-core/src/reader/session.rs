//! Open documents and the `reader://` protocol that serves their pictures.
//!
//! The webview only ever gets a document id. Page bitmaps, book images and
//! image bytes are fetched as `/{doc}/page/{n}?w={px}`, `/{doc}/res/{path}`
//! and `/{doc}/image`; anything else, or any id not opened in this session,
//! is refused.

use super::epub::{mime_for, Book, MAX_ENTRY_BYTES};
use super::image_view::{display_bytes, image_info};
use super::pdf::PdfEngine;
use super::text::{read_text_doc, resolve, sanitize};
use crate::pandoc::PandocEngine;
use fo_types::{FileKind, ReaderBody, ReaderView, SearchHit, TextSpan};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

enum Doc {
    Pdf {
        id: u64,
        pages: u32,
    },
    Book(Mutex<Book>),
    /// Text, Markdown or HTML; images come from `dir`.
    Page {
        dir: PathBuf,
    },
    Image {
        path: PathBuf,
        kind: FileKind,
    },
    /// Word/ODT/RTF converted by Pandoc; images kept in memory.
    Converted {
        media: HashMap<String, Vec<u8>>,
    },
}

pub type Served = Result<(Vec<u8>, &'static str), (u16, String)>;

pub struct Sessions {
    lib_dirs: Mutex<Vec<PathBuf>>,
    engine: OnceLock<Result<PdfEngine, String>>,
    docs: Mutex<HashMap<u64, Arc<Doc>>>,
    next: AtomicU64,
    pandoc: Mutex<Option<Arc<dyn PandocEngine>>>,
}

fn not_found() -> (u16, String) {
    (404, "Not found".into())
}

impl Sessions {
    pub fn new(lib_dirs: Vec<PathBuf>) -> Self {
        Self {
            lib_dirs: Mutex::new(lib_dirs),
            engine: OnceLock::new(),
            docs: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
            pandoc: Mutex::new(None),
        }
    }

    /// Lets Word, OpenDocument and RTF files open (through Pandoc).
    pub fn set_pandoc(&self, engine: Arc<dyn PandocEngine>) {
        *self.pandoc.lock().expect("pandoc") = Some(engine);
    }

    fn converted(&self, path: &Path, kind: FileKind) -> Result<(Doc, ReaderBody), String> {
        let engine = self.pandoc.lock().expect("pandoc").clone().ok_or_else(|| {
            "Reading Word, OpenDocument and RTF files arrives together with document conversion in a later update.".to_string()
        })?;
        let from = crate::pandoc::formats::reader_for(kind).unwrap_or("docx");
        let bytes = std::fs::read(path).map_err(|e| format!("Couldn't read the file: {e}"))?;
        let name = format!("input.{from}");
        let req = crate::pandoc::PandocRequest {
            options: serde_json::json!({"from": from, "to": "html5", "input-files": [name], "extract-media": "media"}),
            files: vec![(name, bytes)],
        };
        let r = engine
            .run(req, &crate::tool::CancelToken::new())
            .map_err(|e| {
                format!(
                    "This document couldn't be opened: {}",
                    e.lines().next().unwrap_or("")
                )
            })?;
        let html = sanitize(
            &String::from_utf8_lossy(&r.stdout),
            "",
            &Arc::new(HashMap::new()),
        );
        let media = r
            .files
            .into_iter()
            .filter(|(n, _)| n.starts_with("media/"))
            .collect();
        Ok((Doc::Converted { media }, ReaderBody::Page(html)))
    }

    /// Adds a folder to look in for PDFium (before the first PDF opens).
    pub fn add_lib_dir(&self, dir: PathBuf) {
        self.lib_dirs.lock().expect("lib dirs").insert(0, dir);
    }

    fn engine(&self) -> Result<&PdfEngine, String> {
        self.engine
            .get_or_init(|| {
                let dirs = self.lib_dirs.lock().expect("lib dirs").clone();
                PdfEngine::start(&super::pdf::find_pdfium_dirs(&dirs))
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    fn get(&self, doc: u64) -> Option<Arc<Doc>> {
        self.docs.lock().expect("docs").get(&doc).cloned()
    }

    pub fn open(&self, path: &Path) -> Result<ReaderView, String> {
        let detected = crate::detect::detect_file(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "This file no longer exists.".to_string(),
            _ => format!("Couldn't read the file: {e}"),
        })?;
        let (doc, body) = match detected.kind {
            FileKind::Pdf => {
                let (id, info) = self.engine()?.open(path)?;
                let pages = info.pages.len() as u32;
                (Doc::Pdf { id, pages }, ReaderBody::Pdf(info))
            }
            FileKind::Epub => {
                let book = Book::open(path)?;
                let info = book.info();
                (Doc::Book(Mutex::new(book)), ReaderBody::Book(info))
            }
            k @ (FileKind::Markdown
            | FileKind::Html
            | FileKind::Text
            | FileKind::Csv
            | FileKind::Json) => {
                let html = read_text_doc(path, k)?;
                let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
                (Doc::Page { dir }, ReaderBody::Page(html))
            }
            k @ FileKind::Image(_) => {
                let info = image_info(path, k)?;
                (
                    Doc::Image {
                        path: path.to_path_buf(),
                        kind: k,
                    },
                    ReaderBody::Image(info),
                )
            }
            k @ (FileKind::Docx
            | FileKind::Odt
            | FileKind::Rtf
            | FileKind::Latex
            | FileKind::Rst
            | FileKind::Org
            | FileKind::AsciiDoc) => self.converted(path, k)?,
            _ => return Err("FileOdyssey can't read this kind of file.".into()),
        };
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.docs.lock().expect("docs").insert(id, Arc::new(doc));
        Ok(ReaderView {
            doc: id,
            name: detected.name,
            body,
        })
    }

    pub fn chapter(&self, doc: u64, i: usize) -> Result<String, String> {
        match self.get(doc).as_deref() {
            Some(Doc::Book(b)) => b.lock().expect("book").chapter(i),
            _ => Err("This book is no longer open".into()),
        }
    }

    pub fn text_spans(&self, doc: u64, page: u32) -> Result<Vec<TextSpan>, String> {
        match self.get(doc).as_deref() {
            Some(Doc::Pdf { id, .. }) => self.engine()?.text_spans(*id, page),
            _ => Err("This PDF is no longer open".into()),
        }
    }

    pub fn search(&self, doc: u64, query: &str) -> Result<Vec<SearchHit>, String> {
        match self.get(doc).as_deref() {
            Some(Doc::Pdf { id, .. }) => self.engine()?.search(*id, query),
            _ => Err("This PDF is no longer open".into()),
        }
    }

    pub fn close(&self, doc: u64) {
        let removed = self.docs.lock().expect("docs").remove(&doc);
        if let Some(Doc::Pdf { id, .. }) = removed.as_deref() {
            if let Ok(e) = self.engine() {
                e.close(*id);
            }
        }
    }

    /// Answers a `reader://` request. `url_path` is the URL path (with the
    /// query, if any), percent-encoded or not.
    pub fn serve(&self, url_path: &str) -> Served {
        let decoded = percent_encoding::percent_decode_str(url_path)
            .decode_utf8()
            .map_err(|_| (400, "Bad address".to_string()))?;
        let rest = decoded.trim_start_matches('/');
        let (id, route) = rest
            .split_once('/')
            .ok_or((400, "Bad address".to_string()))?;
        let doc = id
            .parse::<u64>()
            .ok()
            .and_then(|id| self.get(id))
            .ok_or_else(not_found)?;
        if let Some(res) = route.strip_prefix("res/") {
            return self.resource(&doc, res);
        }
        match (&*doc, route) {
            (Doc::Pdf { id, pages }, r) if r.starts_with("page/") => {
                let (page, query) = r["page/".len()..]
                    .split_once('?')
                    .unwrap_or((&r["page/".len()..], ""));
                let page: u32 = page.parse().map_err(|_| (400, "Bad page".to_string()))?;
                if page >= *pages {
                    return Err(not_found());
                }
                let width = query
                    .split('&')
                    .find_map(|kv| kv.strip_prefix("w="))
                    .and_then(|w| w.parse::<u32>().ok())
                    .unwrap_or(1024);
                let png = self
                    .engine()
                    .and_then(|e| e.render(*id, page, width))
                    .map_err(|e| (500, e))?;
                Ok((png, "image/png"))
            }
            (Doc::Image { path, kind }, "image") => {
                display_bytes(path, *kind).map_err(|e| (500, e))
            }
            _ => Err((400, "Bad address".into())),
        }
    }

    fn resource(&self, doc: &Doc, path: &str) -> Served {
        let forbidden = || (403, "That file isn't part of this document".to_string());
        if resolve("", path).as_deref() != Some(path) {
            return Err(forbidden());
        }
        if !mime_for(path).starts_with("image/") {
            return Err(forbidden());
        }
        match doc {
            Doc::Book(b) => b
                .lock()
                .expect("book")
                .resource(path)
                .map_err(|_| not_found()),
            Doc::Converted { media } => media
                .get(path)
                .map(|b| (b.clone(), mime_for(path)))
                .ok_or_else(not_found),
            Doc::Page { dir } => {
                let file = dir.join(path);
                // Symlinks must not lead outside the document's folder.
                let (Ok(real), Ok(root)) = (file.canonicalize(), dir.canonicalize()) else {
                    return Err(not_found());
                };
                if !real.starts_with(&root) {
                    return Err(forbidden());
                }
                let len = std::fs::metadata(&real).map_err(|_| not_found())?.len();
                if len > MAX_ENTRY_BYTES {
                    return Err((413, "Image too large".into()));
                }
                std::fs::read(&real)
                    .map(|b| (b, mime_for(path)))
                    .map_err(|_| not_found())
            }
            _ => Err((400, "Bad address".into())),
        }
    }
}
