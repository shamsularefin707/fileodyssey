//! Which Pandoc reader and writer each format uses.

use fo_types::FileKind;

/// Conversion targets, as output extensions, in menu order.
pub const TARGETS: [&str; 11] = ["docx", "odt", "rtf", "epub", "html", "md", "txt", "tex", "rst", "org", "adoc"];

/// Pandoc reader for a source kind. Plain text has none: it is turned into
/// a Pandoc AST directly (see `tool::text_ast`), read as `json`.
pub fn reader_for(kind: FileKind) -> Option<&'static str> {
    Some(match kind {
        FileKind::Markdown => "markdown",
        FileKind::Html => "html",
        FileKind::Docx => "docx",
        FileKind::Odt => "odt",
        FileKind::Rtf => "rtf",
        FileKind::Epub => "epub",
        FileKind::Latex => "latex",
        FileKind::Rst => "rst",
        FileKind::Org => "org",
        FileKind::AsciiDoc => "asciidoc",
        FileKind::Text => "json",
        _ => return None,
    })
}

/// Pandoc writer for a target extension.
pub fn writer_for(target: &str) -> Option<&'static str> {
    Some(match target {
        "docx" => "docx",
        "odt" => "odt",
        "rtf" => "rtf",
        "epub" => "epub3",
        "html" => "html5",
        "md" => "markdown",
        "txt" => "plain",
        "tex" => "latex",
        "rst" => "rst",
        "org" => "org",
        "adoc" => "asciidoc",
        _ => return None,
    })
}

/// Reader that can read a target back, to measure what survived.
pub fn reader_for_target(target: &str) -> Option<&'static str> {
    Some(match target {
        "docx" => "docx",
        "odt" => "odt",
        "rtf" => "rtf",
        "epub" => "epub",
        "html" => "html",
        "md" => "markdown",
        "tex" => "latex",
        "rst" => "rst",
        "org" => "org",
        "adoc" => "asciidoc",
        _ => return None,
    })
}

fn own_target(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Docx => "docx",
        FileKind::Odt => "odt",
        FileKind::Rtf => "rtf",
        FileKind::Epub => "epub",
        FileKind::Html => "html",
        FileKind::Markdown => "md",
        FileKind::Text => "txt",
        FileKind::Latex => "tex",
        FileKind::Rst => "rst",
        FileKind::Org => "org",
        FileKind::AsciiDoc => "adoc",
        _ => "",
    }
}

pub fn targets_for(kind: FileKind) -> Vec<&'static str> {
    if reader_for(kind).is_none() {
        return vec![];
    }
    let own = own_target(kind);
    TARGETS.iter().copied().filter(|t| *t != own).collect()
}
