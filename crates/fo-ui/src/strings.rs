//! Every user-facing string, in one place, so a Bengali table can be added later.

use fo_types::Category;

pub const APP_NAME: &str = "FileOdyssey";
pub const POND_TITLE: &str = "Drop files here";
pub const POND_OR: &str = "or";
pub const CHOOSE_FILES: &str = "choose files";
pub const POND_PRIVATE: &str = "Everything stays on this device.";
pub const NO_TOOLS: &str = "No tools for this file type yet";
pub const BEST_EFFORT: &str = "best effort";
pub const BEST_EFFORT_HINT: &str =
    "Some layout may not carry over. The report will list anything that changed.";
pub const TO: &str = "to";
pub const REMOVE: &str = "Remove";
pub const SAVE_TO: &str = "Save to";
pub const CHANGE: &str = "Change";
pub const RUN_ALL: &str = "Run all";
pub const RESULTS: &str = "Results";
pub const QUEUED: &str = "Waiting";
pub const CANCEL: &str = "Cancel";
pub const CANCELLED: &str = "Cancelled";
pub const OPEN: &str = "Open";
pub const SHOW_IN_FOLDER: &str = "Show in folder";
pub const REPORT: &str = "Report";
pub const KEPT_EVERYTHING: &str = "Everything kept";
pub const SOMETHING_CHANGED: &str = "Check the report";
pub const REPORT_TITLE: &str = "What was kept";
pub const REPORT_BEFORE: &str = "Before";
pub const REPORT_AFTER: &str = "After";
pub const REPORT_TEXT: &str = "Text matched";
pub const REPORT_NOTES: &str = "Notes";
pub const REPORT_NONE: &str = "Nothing was lost in this conversion.";
pub const CLOSE: &str = "Close";
pub const OPTIONS: &str = "Options";
pub const HIDE_OPTIONS: &str = "Hide options";
pub const SAME_FORMAT: &str = "Same format";
pub const TEXT: &str = "Text";
pub const COMBINE_PAGE: &str = "Page size";
pub const COMBINE_RUN: &str = "Combine";
pub const NO_FILES_YET: &str = "Nothing here yet. Drop a file into the pond to start.";

pub fn tab_label(c: Category) -> &'static str {
    match c {
        Category::Convert => "Convert",
        Category::Compress => "Compress",
        Category::Images => "Images",
        Category::Read => "Read",
        Category::Scan => "Scan",
    }
}

pub fn tab_intro(c: Category) -> &'static str {
    match c {
        Category::Convert => "Turn documents into other formats without losing what's inside.",
        Category::Compress => "Make files and archives smaller.",
        Category::Images => "Resize, compress and change the format of images.",
        Category::Read => "Open PDFs, books and documents to read.",
        Category::Scan => "Scan paper documents and QR codes with your camera.",
    }
}

pub fn running(pct: u32) -> String {
    format!("Working… {pct}%")
}

pub fn failed(msg: &str) -> String {
    format!("Didn't finish: {msg}")
}

pub const FORMAT: &str = "Format";

/// Heading for a combining tool's bar.
pub fn combine_title(tool_id: &str, n: usize) -> String {
    match tool_id {
        "img.to_pdf" => format!("Combine {n} images into one PDF"),
        "arc.create" if n == 1 => "Put 1 file in an archive".to_owned(),
        "arc.create" => format!("Put {n} files in an archive"),
        _ => format!("Combine {n} files"),
    }
}

/// The bar's button: says what happens.
pub fn combine_action(tool_id: &str) -> &'static str {
    match tool_id {
        "arc.create" => "Create archive",
        _ => COMBINE_RUN,
    }
}

/// How a target extension is shown in the "to" menu.
pub fn target_label(target: &str) -> String {
    match target {
        "same" => SAME_FORMAT.to_owned(),
        "folder" => "Folder".to_owned(),
        "7z" | "tar" | "tar.gz" | "tar.xz" | "tar.zst" | "zip" => format!(".{target}"),
        "txt" => TEXT.to_owned(),
        "jpg" => "JPEG".to_owned(),
        "jxl" => "JPEG XL".to_owned(),
        "tiff" => "TIFF".to_owned(),
        "md" => "Markdown".to_owned(),
        "tex" => "LaTeX".to_owned(),
        "rst" => "reStructuredText".to_owned(),
        "org" => "Org".to_owned(),
        "adoc" => "AsciiDoc".to_owned(),
        "xlsx" => "Excel".to_owned(),
        other => other.to_uppercase(),
    }
}

// Reader
pub const READ_ACTION: &str = "Read";
pub const CANT_READ_YET: &str = "Can't be read yet";
pub const OPENING: &str = "Opening…";
pub const BACK_TO_FILES: &str = "Files";
pub const BACK_TO_FILES_LABEL: &str = "Close and go back to your files";
pub const ZOOM_IN: &str = "Zoom in";
pub const ZOOM_OUT: &str = "Zoom out";
pub const FIT_WIDTH: &str = "Fit width";
pub const FIT: &str = "Fit";
pub const ACTUAL_SIZE: &str = "Actual size";
pub const PAGES: &str = "Pages";
pub const PAGE: &str = "Page";
pub const FIND: &str = "Find in document";
pub const FIND_NEXT: &str = "Next match";
pub const FIND_PREV: &str = "Previous match";
pub const NO_MATCHES: &str = "No matches";
pub const CONTENTS: &str = "Contents";
pub const SMALLER_TEXT: &str = "Smaller text";
pub const LARGER_TEXT: &str = "Larger text";
pub const PREVIOUS_PART: &str = "Previous";
pub const COPY_ADDRESS: &str = "Copy address";
pub const COPIED: &str = "Copied";
pub const DISMISS: &str = "Dismiss";
pub const DETAILS: &str = "Details";
pub const SIZE: &str = "Size";
pub const NO_CAMERA_DETAILS: &str = "No camera details in this image.";

pub fn match_count(i: usize, n: usize) -> String {
    format!("{} of {n}", i + 1)
}

pub fn of_pages(n: usize) -> String {
    format!("of {n}")
}

pub fn link_note(url: &str) -> String {
    format!("This link goes to {url}. The reader works offline, so it isn't opened.")
}

pub fn zoom_label(z: f64) -> String {
    format!("{}%", (z * 100.0).round() as i64)
}
