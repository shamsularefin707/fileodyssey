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
