//! Types shared between the Rust core, the Tauri app and the wasm UI.
//! Keep this crate dependency-light (serde only) so it compiles to wasm.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    Bmp,
    Ico,
    Tiff,
    Webp,
    Avif,
    Heic,
    Jxl,
    Svg,
    Psd,
    Raw,
    Tga,
    Dds,
    Hdr,
    Exr,
    Qoi,
    Pnm,
}

impl ImageFormat {
    pub fn name(&self) -> &'static str {
        match self {
            ImageFormat::Png => "PNG",
            ImageFormat::Jpeg => "JPEG",
            ImageFormat::Gif => "GIF",
            ImageFormat::Bmp => "BMP",
            ImageFormat::Ico => "ICO",
            ImageFormat::Tiff => "TIFF",
            ImageFormat::Webp => "WebP",
            ImageFormat::Avif => "AVIF",
            ImageFormat::Heic => "HEIC",
            ImageFormat::Jxl => "JPEG XL",
            ImageFormat::Svg => "SVG",
            ImageFormat::Psd => "Photoshop",
            ImageFormat::Raw => "Camera RAW",
            ImageFormat::Tga => "TGA",
            ImageFormat::Dds => "DDS",
            ImageFormat::Hdr => "HDR",
            ImageFormat::Exr => "OpenEXR",
            ImageFormat::Qoi => "QOI",
            ImageFormat::Pnm => "PNM",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveFormat {
    Zip,
    SevenZ,
    Tar,
    TarGz,
    TarXz,
    TarZst,
    Rar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Pdf,
    Docx,
    Odt,
    Rtf,
    Epub,
    Markdown,
    Html,
    Text,
    Csv,
    Json,
    Xlsx,
    Pptx,
    Doc,
    Xls,
    Ppt,
    Image(ImageFormat),
    Archive(ArchiveFormat),
    Unknown,
}

impl FileKind {
    /// Human-readable name shown in the UI.
    pub fn label(&self) -> &'static str {
        match self {
            FileKind::Pdf => "PDF",
            FileKind::Docx => "Word document",
            FileKind::Odt => "OpenDocument text",
            FileKind::Rtf => "Rich text",
            FileKind::Epub => "EPUB book",
            FileKind::Markdown => "Markdown",
            FileKind::Html => "Web page",
            FileKind::Text => "Plain text",
            FileKind::Csv => "CSV table",
            FileKind::Json => "JSON",
            FileKind::Xlsx => "Excel workbook",
            FileKind::Pptx => "PowerPoint deck",
            FileKind::Doc => "Word 97–2003 document",
            FileKind::Xls => "Excel 97–2003 workbook",
            FileKind::Ppt => "PowerPoint 97–2003 deck",
            FileKind::Image(f) => match f {
                ImageFormat::Png => "PNG image",
                ImageFormat::Jpeg => "JPEG image",
                ImageFormat::Gif => "GIF image",
                ImageFormat::Bmp => "BMP image",
                ImageFormat::Ico => "Icon",
                ImageFormat::Tiff => "TIFF image",
                ImageFormat::Webp => "WebP image",
                ImageFormat::Avif => "AVIF image",
                ImageFormat::Heic => "HEIC photo",
                ImageFormat::Jxl => "JPEG XL image",
                ImageFormat::Svg => "SVG drawing",
                ImageFormat::Psd => "Photoshop file",
                ImageFormat::Raw => "Camera RAW photo",
                ImageFormat::Tga => "TGA image",
                ImageFormat::Dds => "DDS texture",
                ImageFormat::Hdr => "HDR image",
                ImageFormat::Exr => "OpenEXR image",
                ImageFormat::Qoi => "QOI image",
                ImageFormat::Pnm => "PNM image",
            },
            FileKind::Archive(a) => match a {
                ArchiveFormat::Zip => "ZIP archive",
                ArchiveFormat::SevenZ => "7z archive",
                ArchiveFormat::Tar => "Tar archive",
                ArchiveFormat::TarGz => "Tar.gz archive",
                ArchiveFormat::TarXz => "Tar.xz archive",
                ArchiveFormat::TarZst => "Tar.zst archive",
                ArchiveFormat::Rar => "RAR archive",
            },
            FileKind::Unknown => "Unrecognised file",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Convert,
    Compress,
    Images,
    Read,
    Scan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Desktop,
    Android,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolInfo {
    pub id: String,
    pub label: String,
    pub category: Category,
    /// Output extensions this tool can produce, e.g. `["docx", "md"]`.
    pub targets: Vec<String>,
    /// Known-lossy path; the UI marks it "best effort" before running.
    pub best_effort: bool,
    pub platforms: Vec<Platform>,
    /// Settings the UI shows for this tool; values go into `JobRequest.options`.
    #[serde(default)]
    pub options: Vec<OptionSpec>,
    /// Turns all of a job's inputs into one output (e.g. images → one PDF).
    #[serde(default)]
    pub combines: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptionKind {
    Slider {
        min: u32,
        max: u32,
        step: u32,
        default: u32,
        unit: String,
    },
    Toggle {
        default: bool,
    },
    Number {
        min: u32,
        max: u32,
        default: u32,
        unit: String,
    },
    /// (value, label) pairs.
    Choice {
        choices: Vec<(String, String)>,
        default: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptionSpec {
    pub key: String,
    pub label: String,
    pub kind: OptionKind,
    /// Targets this option applies to; empty means every target.
    #[serde(default)]
    pub targets: Vec<String>,
    /// Show only when another option has this value: `("mode", "percent")`,
    /// or doesn't: `("mode", "!percent")`.
    #[serde(default)]
    pub show_if: Option<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetectedFile {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub kind: FileKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Metric {
    pub name: String,
    pub before: u64,
    pub after: u64,
    /// Shown for context (e.g. file size) but never counted as a loss.
    #[serde(default)]
    pub informational: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Exact,
    Warn,
}

/// Minimum normalised text similarity for a conversion to count as exact (spec §7.1).
pub const EXACT_TEXT_SIMILARITY: f64 = 0.99;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FidelityReport {
    pub metrics: Vec<Metric>,
    pub text_similarity: Option<f64>,
    pub warnings: Vec<String>,
}

impl FidelityReport {
    pub fn verdict(&self) -> Verdict {
        let dropped = self
            .metrics
            .iter()
            .any(|m| !m.informational && m.after < m.before);
        let low_text = self
            .text_similarity
            .is_some_and(|s| s < EXACT_TEXT_SIMILARITY);
        if dropped || low_text || !self.warnings.is_empty() {
            Verdict::Warn
        } else {
            Verdict::Exact
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobRequest {
    pub tool_id: String,
    pub inputs: Vec<String>,
    pub target: Option<String>,
    #[serde(default)]
    pub options: serde_json::Value,
    pub out_dir: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running {
        fraction: f32,
    },
    Done {
        outputs: Vec<String>,
        report: FidelityReport,
    },
    Failed {
        message: String,
    },
    Cancelled,
}

impl JobStatus {
    pub fn is_finished(&self) -> bool {
        matches!(
            self,
            JobStatus::Done { .. } | JobStatus::Failed { .. } | JobStatus::Cancelled
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobEvent {
    pub job_id: String,
    pub input: String,
    pub status: JobStatus,
}
