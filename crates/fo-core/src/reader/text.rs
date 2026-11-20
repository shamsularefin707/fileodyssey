//! Plain text, Markdown and HTML for the reader, always as sanitised HTML.
//!
//! The reader never runs scripts and never touches the network, so
//! `sanitize` removes active content and rewrites every URL into one of a
//! few inert forms the UI understands:
//!
//! * `data-fo-src="<path>"` on images inside the document (or its folder);
//!   the UI turns it into a `reader://` URL.
//! * `href="#fo-<id>"` for anchors on the same page (ids are prefixed with
//!   `fo-` so a document can't clobber the app's own element ids).
//! * `href="fo-ch:<n>#fo-<id>"` for links to another chapter of a book.
//! * `href="fo-ext:<url>"` for web and mail links; the UI shows the address
//!   instead of opening it.

use fo_types::FileKind;
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Largest text, Markdown or HTML file the reader opens.
pub const MAX_TEXT_BYTES: u64 = 20 * 1024 * 1024;

/// Decodes text whatever its encoding: BOM first, then UTF-8, then a guess
/// (windows-1252, Shift_JIS, …).
pub fn decode_text(bytes: &[u8]) -> String {
    if let Some((enc, bom)) = encoding_rs::Encoding::for_bom(bytes) {
        return enc
            .decode_without_bom_handling(&bytes[bom..])
            .0
            .into_owned();
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_owned();
    }
    let mut det = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
    det.feed(bytes, true);
    det.guess(None, chardetng::Utf8Detection::Allow)
        .decode_without_bom_handling(bytes)
        .0
        .into_owned()
}

/// CommonMark plus tables, strikethrough, task lists and footnotes.
/// The result is NOT sanitised (Markdown may contain raw HTML).
pub fn markdown_html(src: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES;
    let mut out = String::new();
    html::push_html(&mut out, Parser::new_ext(src, opts));
    out
}

pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + s.len() / 8);
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

fn is_absolute(url: &str) -> bool {
    if url.starts_with("//") {
        return true;
    }
    match url.find(':') {
        Some(i) => !url[..i].contains(['/', '?', '#']),
        None => false,
    }
}

fn percent_decode(s: &str) -> String {
    percent_encoding::percent_decode_str(s)
        .decode_utf8_lossy()
        .into_owned()
}

/// Resolves `rel` against folder `base` (both `/`-separated, relative to the
/// document root). `None` if it climbs above the root or looks hostile.
pub fn resolve(base: &str, rel: &str) -> Option<String> {
    let rel = rel.split(['?', '#']).next().unwrap_or("");
    let rel = percent_decode(rel);
    if rel.is_empty() || rel.starts_with('/') || rel.contains('\\') || rel.contains('\0') {
        return None;
    }
    let mut parts: Vec<&str> = base.split('/').filter(|p| !p.is_empty()).collect();
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Sanitises `html` (see the module docs). `base` is the folder of the
/// document inside its container; `chapters` maps a resolved path to a
/// chapter index.
pub fn sanitize(html: &str, base: &str, chapters: &Arc<HashMap<String, usize>>) -> String {
    let base = base.to_owned();
    let chapters = Arc::clone(chapters);
    let filter = higher_ranked(move |el: &str, attr: &str, value: &str| {
        let v = value.trim();
        match (el, attr) {
            ("img", "src") => {
                if v.starts_with("data:image/") {
                    Some(Cow::Owned(v.to_owned()))
                } else if is_absolute(v) {
                    None
                } else {
                    resolve(&base, v).map(|p| Cow::Owned(format!("fo-res:{p}")))
                }
            }
            ("a", "href") => {
                if let Some(id) = v.strip_prefix('#') {
                    return (!id.is_empty()).then(|| Cow::Owned(format!("#fo-{id}")));
                }
                if is_absolute(v) {
                    let lower = v.to_ascii_lowercase();
                    let web = ["http:", "https:", "mailto:"]
                        .iter()
                        .any(|s| lower.starts_with(s));
                    return web.then(|| Cow::Owned(format!("fo-ext:{v}")));
                }
                let frag = v.split_once('#').map(|(_, f)| f).filter(|f| !f.is_empty());
                let n = *chapters.get(&resolve(&base, v)?)?;
                Some(Cow::Owned(match frag {
                    Some(f) => format!("fo-ch:{n}#fo-{f}"),
                    None => format!("fo-ch:{n}"),
                }))
            }
            (
                _,
                "src" | "href" | "cite" | "longdesc" | "srcset" | "action" | "poster"
                | "background",
            ) => None,
            _ => Some(Cow::Borrowed(value)),
        }
    });
    let clean = ammonia::Builder::default()
        .add_generic_attributes(["id"])
        .id_prefix(Some("fo-"))
        .add_clean_content_tags(["title"])
        // Only `img src="data:image/…"` survives the filter below.
        .add_url_schemes(["data"])
        .attribute_filter(filter)
        .clean(html)
        .to_string();
    clean.replace(" src=\"fo-res:", " data-fo-src=\"")
}

/// Pins the closure to the `for<'u>` signature ammonia expects.
fn higher_ranked<F>(f: F) -> F
where
    F: for<'u> Fn(&str, &str, &'u str) -> Option<Cow<'u, str>>,
{
    f
}

fn no_chapters() -> Arc<HashMap<String, usize>> {
    Arc::new(HashMap::new())
}

/// Reads a text, Markdown or HTML file as sanitised HTML. Images it uses
/// are relative to the file's own folder.
pub fn read_text_doc(path: &Path, kind: FileKind) -> Result<String, String> {
    let len = std::fs::metadata(path)
        .map_err(|e| format!("Couldn't read the file: {e}"))?
        .len();
    if len > MAX_TEXT_BYTES {
        return Err(format!(
            "This file is {} MB; the reader opens text files up to {} MB.",
            len / (1024 * 1024),
            MAX_TEXT_BYTES / (1024 * 1024)
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("Couldn't read the file: {e}"))?;
    let text = decode_text(&bytes);
    Ok(match kind {
        FileKind::Markdown => sanitize(&markdown_html(&text), "", &no_chapters()),
        FileKind::Html => sanitize(&text, "", &no_chapters()),
        _ => format!("<pre class=\"fo-plain\">{}</pre>", escape_html(&text)),
    })
}
