//! Scanning (Android): what scanned codes hold, scan history, saving
//! scanned document pages, and the actions a scan result offers.

pub mod document;
pub mod history;
pub mod payload;

use fo_types::{FileKind, Payload};
use std::path::{Path, PathBuf};

/// Codes found in an image file (any format the image tools decode).
pub fn codes_in_image(path: &Path) -> Result<Vec<String>, String> {
    let kind = crate::detect::detect_kind(path).map_err(|e| e.to_string())?;
    if !crate::image::decode::decodable(&kind) || kind == FileKind::Pdf {
        return Err("That isn't an image this app can read".into());
    }
    let d = crate::image::decode::decode(path, &kind).map_err(|e| match e {
        crate::tool::ToolError::Failed(m) => m,
        crate::tool::ToolError::Cancelled => "Cancelled".into(),
    })?;
    let first = d
        .frames
        .first()
        .ok_or_else(|| "The image has no pictures in it".to_string())?;
    Ok(crate::qr::read_codes(&first.image))
}

/// `url` if it's a web link that's safe to open, else why not.
pub fn openable_url(url: &str) -> Result<String, String> {
    match payload::classify(url) {
        Payload::Url { url } => Ok(url),
        _ => Err("Only web links (http or https) can be opened".into()),
    }
}

/// A file name from text the UI sends: no folders, no characters that
/// aren't allowed in names.
pub fn safe_stem(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| {
            if "/\\:*?\"<>|".contains(c) || c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect();
    let words: Vec<&str> = cleaned
        .split_whitespace()
        .filter(|w| !w.chars().all(|c| c == '.'))
        .collect();
    if words.is_empty() {
        "Scan".into()
    } else {
        words.join(" ")
    }
}

/// Saves a contact's vCard to `dir` as `<name>.vcf` (never overwriting),
/// for the Contacts app to import.
pub fn save_contact(vcard: &str, dir: &Path) -> Result<PathBuf, String> {
    let Payload::Contact { name, .. } = payload::classify(vcard) else {
        return Err("That isn't a contact card".into());
    };
    let stem = safe_stem(if name.is_empty() { "Contact" } else { &name });
    document::write_new(dir, &stem, "vcf", vcard.as_bytes())
}
