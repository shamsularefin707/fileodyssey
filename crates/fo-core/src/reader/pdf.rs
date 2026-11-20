//! PDF rendering, text and search through PDFium.
//!
//! PDFium isn't safe to drive from several threads, and its documents borrow
//! the library handle, so one worker thread owns both and serves requests
//! over a channel. `PdfEngine` is a cheap, cloneable handle to that thread.

use fo_types::{PageSize, PdfInfo, Rect, SearchHit, TextSpan};
use pdfium_render::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};

/// Widest page bitmap we render, whatever the UI asks for.
pub const MAX_RENDER_WIDTH: u32 = 4096;
/// Search stops after this many hits.
pub const MAX_HITS: usize = 500;

type Reply<T> = Sender<Result<T, String>>;

enum Req {
    Open(PathBuf, Reply<(u64, PdfInfo)>),
    Render(u64, u32, u32, Reply<Vec<u8>>),
    Spans(u64, u32, Reply<Vec<TextSpan>>),
    Search(u64, String, Reply<Vec<SearchHit>>),
    Close(u64),
}

#[derive(Clone)]
pub struct PdfEngine {
    tx: Sender<Req>,
}

/// Folders that may hold the PDFium library, most specific first.
pub fn find_pdfium_dirs(extra: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = extra.to_vec();
    if let Some(d) = std::env::var_os("FO_PDFIUM_DIR") {
        dirs.push(d.into());
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            dirs.push(d.join("pdfium"));
            dirs.push(d.to_path_buf());
        }
    }
    if cfg!(debug_assertions) {
        // Developer checkouts: scripts/fetch-pdfium.sh puts it here.
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pdfium");
        let plat = if cfg!(windows) {
            "win-x64"
        } else {
            "linux-x64"
        };
        dirs.push(
            repo.join(plat)
                .join(if cfg!(windows) { "bin" } else { "lib" }),
        );
    }
    dirs
}

fn bind(dirs: &[PathBuf]) -> Result<Pdfium, String> {
    for d in dirs {
        let lib = Pdfium::pdfium_platform_library_name_at_path(d);
        if lib.is_file() {
            return Pdfium::bind_to_library(&lib)
                .map(Pdfium::new)
                .map_err(|e| format!("Couldn't load the PDF engine: {e}"));
        }
    }
    // Android ships it in the APK's native library folder.
    Pdfium::bind_to_system_library()
        .map(Pdfium::new)
        .map_err(|_| {
            "The PDF engine is missing from this installation. Reinstall FileOdyssey.".into()
        })
}

impl PdfEngine {
    /// Loads PDFium and starts the worker thread.
    pub fn start(lib_dirs: &[PathBuf]) -> Result<PdfEngine, String> {
        let (tx, rx) = channel::<Req>();
        let (ready_tx, ready_rx) = channel::<Result<(), String>>();
        let dirs = lib_dirs.to_vec();
        std::thread::Builder::new()
            .name("pdfium".into())
            .spawn(move || {
                let pdfium = match bind(&dirs) {
                    Ok(p) => {
                        let _ = ready_tx.send(Ok(()));
                        p
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let mut docs: HashMap<u64, PdfDocument<'_>> = HashMap::new();
                let mut next = 1u64;
                for req in rx {
                    // A panic inside one request must not take the engine down.
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match req {
                        Req::Open(path, reply) => {
                            let r = open(&pdfium, &path).map(|(doc, info)| {
                                let id = next;
                                next += 1;
                                docs.insert(id, doc);
                                (id, info)
                            });
                            let _ = reply.send(r);
                        }
                        Req::Render(id, page, w, reply) => {
                            let _ = reply.send(with_page(&docs, id, page, |p| render(p, w)));
                        }
                        Req::Spans(id, page, reply) => {
                            let _ = reply.send(with_page(&docs, id, page, spans));
                        }
                        Req::Search(id, q, reply) => {
                            let r = docs.get(&id).ok_or_else(closed).and_then(|d| search(d, &q));
                            let _ = reply.send(r);
                        }
                        Req::Close(id) => {
                            docs.remove(&id);
                        }
                    }));
                }
            })
            .map_err(|e| format!("Couldn't start the PDF engine: {e}"))?;
        ready_rx
            .recv()
            .map_err(|_| "The PDF engine stopped while starting".to_string())??;
        Ok(PdfEngine { tx })
    }

    fn ask<T>(&self, make: impl FnOnce(Reply<T>) -> Req) -> Result<T, String> {
        let (tx, rx) = channel();
        self.tx
            .send(make(tx))
            .map_err(|_| "The PDF engine has stopped".to_string())?;
        rx.recv()
            .map_err(|_| "The PDF engine couldn't finish this page".to_string())?
    }

    pub fn open(&self, path: &Path) -> Result<(u64, PdfInfo), String> {
        self.ask(|r| Req::Open(path.to_path_buf(), r))
    }

    /// PNG bitmap of `page` (0-based), `width_px` wide (clamped to 16…4096).
    pub fn render(&self, doc: u64, page: u32, width_px: u32) -> Result<Vec<u8>, String> {
        let w = width_px.clamp(16, MAX_RENDER_WIDTH);
        self.ask(|r| Req::Render(doc, page, w, r))
    }

    pub fn text_spans(&self, doc: u64, page: u32) -> Result<Vec<TextSpan>, String> {
        self.ask(|r| Req::Spans(doc, page, r))
    }

    /// Case-insensitive search over the whole document.
    pub fn search(&self, doc: u64, query: &str) -> Result<Vec<SearchHit>, String> {
        if query.trim().is_empty() {
            return Ok(vec![]);
        }
        self.ask(|r| Req::Search(doc, query.to_owned(), r))
    }

    pub fn close(&self, doc: u64) {
        let _ = self.tx.send(Req::Close(doc));
    }
}

fn closed() -> String {
    "This document is no longer open".into()
}

fn open<'a>(pdfium: &'a Pdfium, path: &Path) -> Result<(PdfDocument<'a>, PdfInfo), String> {
    let doc = pdfium.load_pdf_from_file(path, None).map_err(|e| match e {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            "This PDF is password-protected. Opening protected PDFs isn't supported yet."
                .to_string()
        }
        PdfiumError::IoError(e) => format!("Couldn't read the file: {e}"),
        _ => "This PDF is damaged or isn't a PDF, so it can't be opened.".to_string(),
    })?;
    let pages = doc
        .pages()
        .page_sizes()
        .map_err(|_| "This PDF is damaged, so its pages can't be read.".to_string())?
        .into_iter()
        .map(|r| PageSize {
            w: r.width().value,
            h: r.height().value,
        })
        .collect();
    let title = doc
        .metadata()
        .get(PdfDocumentMetadataTagType::Title)
        .map(|t| t.value().trim().to_owned())
        .filter(|t| !t.is_empty());
    Ok((doc, PdfInfo { pages, title }))
}

fn with_page<T>(
    docs: &HashMap<u64, PdfDocument<'_>>,
    id: u64,
    page: u32,
    f: impl FnOnce(&PdfPage) -> Result<T, String>,
) -> Result<T, String> {
    let doc = docs.get(&id).ok_or_else(closed)?;
    if page >= doc.pages().len() as u32 {
        return Err(format!("Page {} doesn't exist", page + 1));
    }
    let p = doc
        .pages()
        .get(page as PdfPageIndex)
        .map_err(|_| format!("Page {} is damaged", page + 1))?;
    f(&p)
}

fn render(page: &PdfPage, width: u32) -> Result<Vec<u8>, String> {
    let cfg = PdfRenderConfig::new()
        .set_target_width(width as Pixels)
        .render_form_data(true)
        .render_annotations(true);
    let img = page
        .render_with_config(&cfg)
        .and_then(|b| b.as_image())
        .map_err(|_| "This page couldn't be drawn".to_string())?;
    let mut out = std::io::Cursor::new(Vec::new());
    img.to_rgb8()
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

/// PDF rect (points, origin bottom-left) → page fractions (origin top-left).
fn frac(r: PdfRect, pw: f32, ph: f32) -> Rect {
    let c = |v: f32| v.clamp(0.0, 1.0);
    let x = c(r.left().value / pw);
    let y = c((ph - r.top().value) / ph);
    Rect {
        x,
        y,
        w: c(r.right().value / pw) - x,
        h: c((ph - r.bottom().value) / ph) - y,
    }
}

fn spans(page: &PdfPage) -> Result<Vec<TextSpan>, String> {
    let (pw, ph) = (page.width().value, page.height().value);
    let text = page
        .text()
        .map_err(|_| "This page's text can't be read".to_string())?;
    Ok(text
        .segments()
        .iter()
        .filter_map(|s| {
            let t = s.text();
            (!t.trim().is_empty()).then(|| TextSpan {
                text: t,
                rect: frac(s.bounds(), pw, ph),
            })
        })
        .collect())
}

fn search(doc: &PdfDocument, query: &str) -> Result<Vec<SearchHit>, String> {
    let opts = PdfSearchOptions::new();
    let mut hits = Vec::new();
    let mut total = 0usize;
    for (i, page) in doc.pages().iter().enumerate() {
        let (pw, ph) = (page.width().value, page.height().value);
        let Ok(text) = page.text() else { continue };
        let Ok(found) = text.search(query, &opts) else {
            continue;
        };
        while let Some(segs) = found.find_next() {
            let rects: Vec<Rect> = segs.iter().map(|s| frac(s.bounds(), pw, ph)).collect();
            hits.push(SearchHit {
                page: i as u32,
                rects,
            });
            total += 1;
            if total >= MAX_HITS {
                return Ok(hits);
            }
        }
    }
    Ok(hits)
}
