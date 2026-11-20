//! EPUB 2 and 3 books: metadata, reading order, chapter titles, sanitised
//! chapters and their images. DRM-protected books are refused.

use super::text::{decode_text, resolve, sanitize};
use crate::archive::guard::safe_path;
use fo_types::BookInfo;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::Arc;

/// Largest single file read from inside a book.
pub const MAX_ENTRY_BYTES: u64 = 50 * 1024 * 1024;

/// Encryption that only obfuscates embedded fonts; anything else is DRM.
const FONT_OBFUSCATION: [&str; 2] = [
    "http://www.idpf.org/2008/embedding",
    "http://ns.adobe.com/pdf/enc#RC",
];

pub struct Book {
    zip: zip::ZipArchive<BufReader<File>>,
    title: Option<String>,
    /// Zip paths of the reading order.
    spine: Vec<String>,
    titles: Vec<String>,
    chapters: Arc<HashMap<String, usize>>,
}

fn damaged() -> String {
    "This book is damaged, so it can't be opened.".into()
}

enum Ev {
    Open(String, Vec<(String, String)>),
    Close(String),
    Text(String),
}

/// Calls `f` for each element start/end and text run, with local names
/// (namespace prefixes dropped).
fn walk(xml: &str, mut f: impl FnMut(Ev)) -> Result<(), String> {
    use quick_xml::events::Event;
    let mut r = quick_xml::Reader::from_str(xml);
    let attrs = |e: &quick_xml::events::BytesStart| -> Vec<(String, String)> {
        e.attributes()
            .flatten()
            .map(|a| {
                let k = a.key.local_name().as_ref().to_owned();
                let v = a
                    .unescape_value()
                    .map(|v| v.into_owned())
                    .unwrap_or_else(|_| a.value.to_string());
                (k, v)
            })
            .collect()
    };
    let name = |n: &str| n.to_owned();
    loop {
        match r.read_event().map_err(|_| damaged())? {
            Event::Start(e) => f(Ev::Open(name(e.local_name().as_ref()), attrs(&e))),
            Event::Empty(e) => {
                let n = name(e.local_name().as_ref());
                f(Ev::Open(n.clone(), attrs(&e)));
                f(Ev::Close(n));
            }
            Event::End(e) => f(Ev::Close(name(e.local_name().as_ref()))),
            Event::Text(t) => {
                let raw: &str = &t;
                f(Ev::Text(
                    quick_xml::escape::unescape(raw)
                        .map(|u| u.into_owned())
                        .unwrap_or_else(|_| raw.to_owned()),
                ));
            }
            Event::GeneralRef(r) => {
                let s: String = (*r).to_owned();
                let txt = match s.as_str() {
                    "amp" => "&".into(),
                    "lt" => "<".into(),
                    "gt" => ">".into(),
                    "quot" => "\"".into(),
                    "apos" => "'".into(),
                    _ => r
                        .resolve_char_ref()
                        .ok()
                        .flatten()
                        .map(String::from)
                        .unwrap_or_default(),
                };
                f(Ev::Text(txt));
            }
            Event::CData(t) => f(Ev::Text((*t).to_owned())),
            Event::Eof => return Ok(()),
            _ => {}
        }
    }
}

fn get<'a>(attrs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
}

fn read_entry(zip: &mut zip::ZipArchive<BufReader<File>>, name: &str) -> Result<Vec<u8>, String> {
    let f = zip
        .by_name(name)
        .map_err(|_| format!("{name} is missing from the book"))?;
    if f.size() > MAX_ENTRY_BYTES {
        return Err("A file inside this book is too large to open".into());
    }
    let mut out = Vec::with_capacity(f.size() as usize);
    f.take(MAX_ENTRY_BYTES + 1)
        .read_to_end(&mut out)
        .map_err(|_| damaged())?;
    Ok(out)
}

fn read_text(zip: &mut zip::ZipArchive<BufReader<File>>, name: &str) -> Result<String, String> {
    read_entry(zip, name).map(|b| decode_text(&b))
}

/// href → title from an EPUB 3 nav document (the `toc` nav only).
fn nav_titles(xml: &str, base: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    let (mut depth, mut toc_depth) = (0usize, None::<usize>);
    let mut link: Option<(String, String)> = None;
    walk(xml, |ev| match ev {
        Ev::Open(n, a) => {
            depth += 1;
            if n == "nav"
                && toc_depth.is_none()
                && get(&a, "type").is_some_and(|t| t.split_whitespace().any(|t| t == "toc"))
            {
                toc_depth = Some(depth);
            }
            if n == "a" && toc_depth.is_some() {
                if let Some(h) = get(&a, "href").and_then(|h| resolve(base, h)) {
                    link = Some((h, String::new()));
                }
            }
        }
        Ev::Text(t) => {
            if let Some((_, title)) = link.as_mut() {
                title.push_str(&t);
            }
        }
        Ev::Close(n) => {
            if n == "a" {
                if let Some((h, t)) = link.take() {
                    out.push((h, t.split_whitespace().collect::<Vec<_>>().join(" ")));
                }
            }
            if toc_depth == Some(depth) {
                toc_depth = None;
            }
            depth = depth.saturating_sub(1);
        }
    })?;
    Ok(out)
}

/// href → title from an EPUB 2 NCX.
fn ncx_titles(xml: &str, base: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    let (mut label, mut in_text, mut pending) = (String::new(), false, None::<String>);
    walk(xml, |ev| match ev {
        Ev::Open(n, a) => match n.as_str() {
            "navLabel" => label.clear(),
            "text" => in_text = true,
            "content" => pending = get(&a, "src").and_then(|s| resolve(base, s)),
            _ => {}
        },
        Ev::Text(t) if in_text => label.push_str(&t),
        Ev::Close(n) => {
            if n == "text" {
                in_text = false;
            }
            if n == "content" {
                if let Some(h) = pending.take() {
                    out.push((h, label.trim().to_owned()));
                }
            }
        }
        _ => {}
    })?;
    Ok(out)
}

impl Book {
    pub fn open(path: &Path) -> Result<Book, String> {
        let file = File::open(path).map_err(|e| format!("Couldn't read the file: {e}"))?;
        let mut zip = zip::ZipArchive::new(BufReader::new(file)).map_err(|_| damaged())?;

        if let Ok(enc) = read_text(&mut zip, "META-INF/encryption.xml") {
            let mut drm = false;
            walk(&enc, |ev| {
                if let Ev::Open(n, a) = ev {
                    if n == "EncryptionMethod"
                        && get(&a, "Algorithm").is_some_and(|alg| !FONT_OBFUSCATION.contains(&alg))
                    {
                        drm = true;
                    }
                }
            })?;
            if drm {
                return Err("This book is DRM-protected, so it can't be opened.".into());
            }
        }

        let container = read_text(&mut zip, "META-INF/container.xml").map_err(|_| damaged())?;
        let mut opf_path = None;
        walk(&container, |ev| {
            if let Ev::Open(n, a) = ev {
                if n == "rootfile" && opf_path.is_none() {
                    opf_path = get(&a, "full-path").and_then(|p| resolve("", p));
                }
            }
        })?;
        let opf_path = opf_path.ok_or_else(damaged)?;
        let opf = read_text(&mut zip, &opf_path).map_err(|_| damaged())?;
        let base = dir_of(&opf_path).to_owned();

        let mut title: Option<String> = None;
        let (mut in_title, mut buf) = (false, String::new());
        let mut manifest: HashMap<String, (String, String, String)> = HashMap::new();
        let mut spine_ids = Vec::new();
        let mut toc_id = None;
        walk(&opf, |ev| match ev {
            Ev::Open(n, a) => match n.as_str() {
                "title" if title.is_none() => {
                    in_title = true;
                    buf.clear();
                }
                "item" => {
                    if let (Some(id), Some(href)) = (
                        get(&a, "id"),
                        get(&a, "href").and_then(|h| resolve(&base, h)),
                    ) {
                        let mt = get(&a, "media-type").unwrap_or("").to_owned();
                        let props = get(&a, "properties").unwrap_or("").to_owned();
                        manifest.insert(id.to_owned(), (href, mt, props));
                    }
                }
                "spine" => toc_id = get(&a, "toc").map(str::to_owned),
                "itemref" => {
                    if let Some(id) = get(&a, "idref") {
                        spine_ids.push(id.to_owned());
                    }
                }
                _ => {}
            },
            Ev::Text(t) if in_title => buf.push_str(&t),
            Ev::Close(n) if n == "title" && in_title => {
                in_title = false;
                let t = buf.trim();
                if !t.is_empty() {
                    title = Some(t.to_owned());
                }
            }
            _ => {}
        })?;

        let spine: Vec<String> = spine_ids
            .iter()
            .filter_map(|id| manifest.get(id))
            .filter(|(_, mt, _)| mt.contains("html") || mt.is_empty())
            .map(|(h, _, _)| h.clone())
            .collect();
        if spine.is_empty() {
            return Err(damaged());
        }

        let nav = manifest
            .values()
            .find(|(_, _, p)| p.split_whitespace().any(|p| p == "nav"))
            .map(|(h, _, _)| h.clone());
        let ncx = toc_id
            .and_then(|id| manifest.get(&id).map(|(h, _, _)| h.clone()))
            .or_else(|| {
                manifest
                    .values()
                    .find(|(_, mt, _)| mt == "application/x-dtbncx+xml")
                    .map(|(h, _, _)| h.clone())
            });
        let mut toc: Vec<(String, String)> = Vec::new();
        if let Some(n) = nav {
            if let Ok(x) = read_text(&mut zip, &n) {
                toc = nav_titles(&x, dir_of(&n)).unwrap_or_default();
            }
        }
        if toc.is_empty() {
            if let Some(n) = ncx {
                if let Ok(x) = read_text(&mut zip, &n) {
                    toc = ncx_titles(&x, dir_of(&n)).unwrap_or_default();
                }
            }
        }
        let titles = spine
            .iter()
            .map(|p| {
                toc.iter()
                    .find(|(h, _)| h == p)
                    .map(|(_, t)| t.clone())
                    .unwrap_or_default()
            })
            .collect();
        let chapters = Arc::new(
            spine
                .iter()
                .enumerate()
                .map(|(i, p)| (p.clone(), i))
                .collect(),
        );
        Ok(Book {
            zip,
            title,
            spine,
            titles,
            chapters,
        })
    }

    pub fn info(&self) -> BookInfo {
        BookInfo {
            title: self.title.clone(),
            chapters: self.titles.clone(),
        }
    }

    /// Chapter `i` of the reading order as sanitised HTML.
    pub fn chapter(&mut self, i: usize) -> Result<String, String> {
        let path = self
            .spine
            .get(i)
            .ok_or_else(|| format!("Chapter {} doesn't exist", i + 1))?
            .clone();
        let html = read_text(&mut self.zip, &path)?;
        Ok(sanitize(&html, dir_of(&path), &self.chapters))
    }

    /// An image or other file from inside the book.
    pub fn resource(&mut self, path: &str) -> Result<(Vec<u8>, &'static str), String> {
        if resolve("", path).as_deref() != Some(path) || safe_path(path).is_none() {
            return Err("That file isn't part of this book".into());
        }
        let bytes = read_entry(&mut self.zip, path)?;
        Ok((bytes, mime_for(path)))
    }
}

pub fn mime_for(path: &str) -> &'static str {
    let ext = path
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "css" => "text/css",
        _ => "application/octet-stream",
    }
}
