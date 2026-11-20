//! PDF rendering, text and search through PDFium.
//!
//! PDFium isn't safe to drive from several threads, and its documents borrow
//! the library handle, so one worker thread owns both and serves requests
//! over a channel. `PdfEngine` is a cheap, cloneable handle to that thread.

use crate::pdfconv::{family_of, Glyph, PageContent, PageImage, PageLink, Rule};
use fo_types::{PageSize, PdfInfo, Rect, SearchHit, TextSpan};
use pdfium_render::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock};

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
    Extract(PathBuf, Reply<Vec<PageContent>>),
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
                        Req::Extract(path, reply) => {
                            let r = open(&pdfium, &path).and_then(|(doc, _)| extract(&doc));
                            let _ = reply.send(r);
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

    /// Characters, images and links of every page, for reconstruction.
    pub fn extract(&self, path: &Path) -> Result<Vec<PageContent>, String> {
        self.ask(|r| Req::Extract(path.to_path_buf(), r))
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

static LIB_DIRS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static ENGINE: OnceLock<Result<PdfEngine, String>> = OnceLock::new();

/// Adds a folder to search for PDFium (call before the first PDF is opened).
pub fn add_lib_dir(dir: PathBuf) {
    LIB_DIRS.lock().expect("lib dirs").insert(0, dir);
}

/// The process-wide PDF engine, started on first use.
pub fn engine() -> Result<&'static PdfEngine, String> {
    ENGINE
        .get_or_init(|| {
            let extra = LIB_DIRS.lock().expect("lib dirs").clone();
            PdfEngine::start(&find_pdfium_dirs(&extra))
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// Largest edge of an extracted image; bigger ones are scaled down.
const MAX_IMAGE_EDGE: u32 = 4000;

fn rect_tl(r: PdfRect, ph: f32) -> (f32, f32, f32, f32) {
    (
        r.left().value,
        ph - r.top().value,
        r.right().value,
        ph - r.bottom().value,
    )
}

fn push_space(obj: &PdfPageObject, ph: f32, out: &mut Vec<[f32; 4]>) {
    if let Some(form) = obj.as_x_object_form_object() {
        for i in 0..form.len() {
            if let Ok(child) = form.get(i) {
                push_space(&child, ph, out);
            }
        }
        return;
    }
    let Some(t) = obj.as_text_object() else {
        return;
    };
    let text = t.text();
    if !text.chars().all(char::is_whitespace) {
        return;
    }
    if let Ok(b) = obj.bounds() {
        let (x0, y0, x1, y1) = rect_tl(b.to_rect(), ph);
        // An object with no mapped text counts only when it draws nothing
        // (a space glyph in a font with no ToUnicode entry for it); one with
        // ink is a glyph whose text lives in a marked-content span.
        if !text.is_empty() || x1 - x0 < 0.5 {
            out.push([x0, y0, x1, y1]);
        }
    }
}

/// Most rules kept per page: a vector drawing can hold thousands of strokes.
const MAX_RULES: usize = 4000;

/// Shortest segment that counts as a rule, in points.
const MIN_RULE: f32 = 3.0;

/// Axis-aligned straight lines drawn by path objects: the stroked segments,
/// and filled rectangles thin enough to be lines. `outer` holds the matrices
/// of the form XObjects the object sits in, innermost first.
fn push_rules(obj: &PdfPageObject, ph: f32, outer: &[PdfMatrix], out: &mut Vec<Rule>) {
    if out.len() >= MAX_RULES {
        return;
    }
    if let Some(form) = obj.as_x_object_form_object() {
        let mut chain = vec![form.matrix().unwrap_or(PdfMatrix::IDENTITY)];
        chain.extend_from_slice(outer);
        for i in 0..form.len() {
            if let Ok(child) = form.get(i) {
                push_rules(&child, ph, &chain, out);
            }
        }
        return;
    }
    let Some(path) = obj.as_path_object() else {
        return;
    };
    let own = path.matrix().unwrap_or(PdfMatrix::IDENTITY);
    let to_page = |x: PdfPoints, y: PdfPoints| {
        let (mut x, mut y) = own.apply_to_points(x, y);
        for m in outer {
            (x, y) = m.apply_to_points(x, y);
        }
        (x.value, ph - y.value)
    };
    // Straight segments, with each closed sub-path's closing edge.
    let mut segs: Vec<((f32, f32), (f32, f32))> = Vec::new();
    let mut curved = false;
    let (mut start, mut at) = ((0.0, 0.0), (0.0, 0.0));
    for s in path.segments().iter() {
        let pt = to_page(s.x(), s.y());
        match s.segment_type() {
            PdfPathSegmentType::MoveTo => {
                start = pt;
                at = pt;
            }
            PdfPathSegmentType::LineTo => {
                segs.push((at, pt));
                at = pt;
            }
            _ => {
                curved = true;
                at = pt;
            }
        }
        if s.is_close() {
            segs.push((at, start));
            at = start;
        }
    }
    let thin = |(a, b): &((f32, f32), (f32, f32))| {
        let (dx, dy) = ((a.0 - b.0).abs(), (a.1 - b.1).abs());
        (dy < 0.5 && dx >= MIN_RULE) || (dx < 0.5 && dy >= MIN_RULE)
    };
    if path.is_stroked().unwrap_or(false) {
        let w = path.stroke_width().map(|w| w.value).unwrap_or(1.0).max(0.1) / 2.0;
        for s in segs.iter().filter(|s| thin(s)) {
            let (a, b) = s;
            out.push(Rule {
                x0: a.0.min(b.0) - if (a.1 - b.1).abs() < 0.5 { 0.0 } else { w },
                x1: a.0.max(b.0) + if (a.1 - b.1).abs() < 0.5 { 0.0 } else { w },
                y0: a.1.min(b.1) - if (a.0 - b.0).abs() < 0.5 { 0.0 } else { w },
                y1: a.1.max(b.1) + if (a.0 - b.0).abs() < 0.5 { 0.0 } else { w },
            });
        }
    } else if !curved && !matches!(path.fill_mode(), Ok(PdfPathFillMode::None)) {
        // A filled shape thinner than 2pt is a line.
        let xs = segs.iter().flat_map(|(a, b)| [a.0, b.0]);
        let ys = segs.iter().flat_map(|(a, b)| [a.1, b.1]);
        let (x0, x1) = xs.fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(v), h.max(v)));
        let (y0, y1) = ys.fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(v), h.max(v)));
        let (w, h) = (x1 - x0, y1 - y0);
        if segs.len() >= 3 && w.min(h) < 2.0 && w.max(h) >= MIN_RULE {
            out.push(Rule { x0, y0, x1, y1 });
        }
    }
}

fn push_image(obj: &PdfPageObject, ph: f32, out: &mut Vec<PageImage>) {
    if let Some(form) = obj.as_x_object_form_object() {
        for i in 0..form.len() {
            if let Ok(child) = form.get(i) {
                push_image(&child, ph, out);
            }
        }
        return;
    }
    let Some(img) = obj.as_image_object() else {
        return;
    };
    let Ok(bounds) = obj.bounds() else { return };
    let (x0, y0, x1, y1) = rect_tl(bounds.to_rect(), ph);
    if (x1 - x0) < 4.0 || (y1 - y0) < 4.0 {
        return;
    }
    let Ok(mut bitmap) = img.get_raw_image() else {
        return;
    };
    if bitmap.width().max(bitmap.height()) > MAX_IMAGE_EDGE {
        bitmap = bitmap.resize(
            MAX_IMAGE_EDGE,
            MAX_IMAGE_EDGE,
            image::imageops::FilterType::Triangle,
        );
    }
    let mut png = std::io::Cursor::new(Vec::new());
    if image::DynamicImage::ImageRgba8(bitmap.to_rgba8())
        .write_to(&mut png, image::ImageFormat::Png)
        .is_ok()
    {
        out.push(PageImage {
            x0,
            y0,
            x1,
            y1,
            png: png.into_inner(),
        });
    }
}

fn extract(doc: &PdfDocument) -> Result<Vec<PageContent>, String> {
    let mut pages = Vec::new();
    for page in doc.pages().iter() {
        let (pw, ph) = (page.width().value, page.height().value);
        let mut pc = PageContent {
            width: pw,
            height: ph,
            ..PageContent::default()
        };
        if let Ok(text) = page.text() {
            for ch in text.chars().iter() {
                let Some(t) = ch.unicode_string() else {
                    continue;
                };
                if t.is_empty() {
                    continue;
                }
                let Ok(b) = ch.loose_bounds().or_else(|_| ch.tight_bounds()) else {
                    continue;
                };
                let (x0, y0, x1, y1) = rect_tl(b, ph);
                let name = ch.font_name().to_ascii_lowercase();
                pc.glyphs.push(Glyph {
                    text: t,
                    x0,
                    y0,
                    x1,
                    y1,
                    size: ch.scaled_font_size().value,
                    // Font names are reliable; weights often aren't recorded and
                    // PDFium's guess calls regular text bold.
                    bold: ch.font_is_bold_reenforced()
                        || name.contains("bold")
                        || name.contains("black")
                        || name.contains("heavy")
                        || name.contains("semibold"),
                    italic: ch.font_is_italic()
                        || name.contains("italic")
                        || name.contains("oblique"),
                    mono: ch.font_is_fixed_pitch()
                        || name.contains("mono")
                        || name.contains("courier"),
                    generated: ch.is_generated().unwrap_or(false),
                    hyphen: ch.is_hyphen().unwrap_or(false),
                    font: family_of(&ch.font_name()),
                    color: ch
                        .fill_color()
                        .map(|c| [c.red(), c.green(), c.blue()])
                        .unwrap_or([0, 0, 0]),
                });
            }
        }
        for obj in page.objects().iter() {
            push_image(&obj, ph, &mut pc.images);
            push_space(&obj, ph, &mut pc.spaces);
            push_rules(&obj, ph, &[], &mut pc.rules);
        }
        for link in page.links().iter() {
            let Some(PdfAction::Uri(a)) = link.action() else {
                continue;
            };
            let (Ok(uri), Ok(r)) = (a.uri(), link.rect()) else {
                continue;
            };
            let (x0, y0, x1, y1) = rect_tl(r, ph);
            pc.links.push(PageLink {
                x0,
                y0,
                x1,
                y1,
                uri,
            });
        }
        pages.push(pc);
    }
    Ok(pages)
}
