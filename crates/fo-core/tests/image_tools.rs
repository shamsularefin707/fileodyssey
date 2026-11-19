use fo_core::image::ops::{resize, ResizeSpec};
use fo_core::image::tools::image_tools;
use fo_core::registry::Registry;
use fo_core::runner::Runner;
use fo_types::*;
use image::{DynamicImage, ImageBuffer, Rgb};
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;

fn registry() -> Arc<Registry> {
    let mut r = Registry::new();
    for t in image_tools() {
        r.register(t);
    }
    Arc::new(r)
}

fn run(tool: &str, input: &Path, target: &str, options: serde_json::Value) -> JobStatus {
    let runner = Runner::new(registry(), 1);
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    runner.submit(
        JobRequest {
            tool_id: tool.into(),
            inputs: vec![input.to_string_lossy().into()],
            target: Some(target.into()),
            options,
            out_dir: input.parent().unwrap().to_string_lossy().into(),
        },
        Arc::new(move |e| tx.lock().unwrap().send(e).unwrap()),
    );
    loop {
        let e: JobEvent = rx.recv_timeout(Duration::from_secs(60)).unwrap();
        if e.status.is_finished() {
            return e.status;
        }
    }
}

fn noise(w: u32, h: u32) -> DynamicImage {
    let mut s = 12345u32;
    DynamicImage::ImageRgb8(ImageBuffer::from_fn(w, h, |_, _| {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        Rgb([s as u8, (s >> 8) as u8, (s >> 16) as u8])
    }))
}

fn metric(r: &FidelityReport, name: &str) -> (u64, u64) {
    let m = r
        .metrics
        .iter()
        .find(|m| m.name == name)
        .unwrap_or_else(|| panic!("no {name} in {r:?}"));
    (m.before, m.after)
}

#[test]
fn convert_tool_png_to_webp_via_runner() {
    let d = TempDir::new().unwrap();
    let src = d.path().join("pic.png");
    noise(40, 30).save(&src).unwrap();
    match run(
        "img.convert",
        &src,
        "webp",
        serde_json::json!({"quality": 80}),
    ) {
        JobStatus::Done { outputs, report } => {
            assert!(outputs[0].ends_with("pic (converted).webp"), "{outputs:?}");
            assert!(Path::new(&outputs[0]).exists());
            assert_eq!(report.verdict(), Verdict::Warn);
            assert!(report.warnings.iter().any(|w| w.contains("quality 80")));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn resize_fit_never_upscales() {
    let out = resize(&noise(100, 50), &ResizeSpec::Fit { w: 400, h: 400 });
    assert_eq!((out.width(), out.height()), (100, 50));
    let out = resize(&noise(100, 50), &ResizeSpec::Fit { w: 40, h: 40 });
    assert_eq!((out.width(), out.height()), (40, 20));
}

#[test]
fn resize_percent_halves() {
    let out = resize(&noise(100, 50), &ResizeSpec::Percent(50));
    assert_eq!((out.width(), out.height()), (50, 25));
}

#[test]
fn resize_tool_reports_new_size() {
    let d = TempDir::new().unwrap();
    let src = d.path().join("big.png");
    noise(200, 100).save(&src).unwrap();
    match run(
        "img.resize",
        &src,
        "same",
        serde_json::json!({"mode": "exact", "width": 64, "height": 32}),
    ) {
        JobStatus::Done { outputs, report } => {
            assert!(outputs[0].ends_with(".png"), "{outputs:?}");
            assert_eq!(metric(&report, "Width"), (200, 64));
            assert_eq!(image::open(&outputs[0]).unwrap().height(), 32);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn compress_to_target_size_meets_budget() {
    let d = TempDir::new().unwrap();
    let src = d.path().join("noise.png");
    noise(512, 512).save(&src).unwrap();
    match run(
        "img.compress",
        &src,
        "jpg",
        serde_json::json!({"max_kb": 40}),
    ) {
        JobStatus::Done { outputs, report } => {
            let size = std::fs::metadata(&outputs[0]).unwrap().len();
            assert!(size <= 40 * 1024, "{size} bytes");
            let (before, after) = metric(&report, "File size (KB)");
            assert!(after <= 40 && before > after, "{before} → {after}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn compress_impossible_budget_warns() {
    let d = TempDir::new().unwrap();
    let src = d.path().join("noise.png");
    noise(512, 512).save(&src).unwrap();
    match run(
        "img.compress",
        &src,
        "jpg",
        serde_json::json!({"max_kb": 1}),
    ) {
        JobStatus::Done { report, .. } => {
            assert!(
                report
                    .warnings
                    .iter()
                    .any(|w| w.starts_with("Couldn't reach 1 KB")),
                "{report:?}"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn compress_png_same_is_lossless() {
    let d = TempDir::new().unwrap();
    let src = d.path().join("flat.png");
    DynamicImage::ImageRgb8(ImageBuffer::from_pixel(300, 300, Rgb([10, 200, 30])))
        .save(&src)
        .unwrap();
    match run("img.compress", &src, "same", serde_json::json!({})) {
        JobStatus::Done { outputs, report } => {
            assert!(outputs[0].ends_with(".png"));
            assert_eq!(
                image::open(&outputs[0]).unwrap().to_rgb8(),
                image::open(&src).unwrap().to_rgb8()
            );
            assert!(
                !report.warnings.iter().any(|w| w.contains("lossy")),
                "{report:?}"
            );
        }
        other => panic!("{other:?}"),
    }
}

fn qr_png(dir: &TempDir, text: &str) -> std::path::PathBuf {
    use rxing::Writer;
    let m = rxing::qrcode::QRCodeWriter
        .encode(text, &rxing::BarcodeFormat::QR_CODE, 240, 240)
        .unwrap();
    let img = ImageBuffer::from_fn(m.width(), m.height(), |x, y| {
        if m.get(x, y) {
            image::Luma([0u8])
        } else {
            image::Luma([255u8])
        }
    });
    let p = dir.path().join("code.png");
    DynamicImage::ImageLuma8(img).save(&p).unwrap();
    p
}

#[test]
fn qr_read_finds_text() {
    let d = TempDir::new().unwrap();
    let src = qr_png(&d, "hello fileodyssey");
    match run("qr.read", &src, "txt", serde_json::json!({})) {
        JobStatus::Done { outputs, report } => {
            assert_eq!(
                std::fs::read_to_string(&outputs[0]).unwrap().trim(),
                "hello fileodyssey"
            );
            assert_eq!(metric(&report, "Codes found").1, 1);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn qr_read_none_warns() {
    let d = TempDir::new().unwrap();
    let src = d.path().join("plain.png");
    noise(64, 64).save(&src).unwrap();
    match run("qr.read", &src, "txt", serde_json::json!({})) {
        JobStatus::Done { report, .. } => {
            assert!(
                report
                    .warnings
                    .iter()
                    .any(|w| w == "No QR code or barcode found"),
                "{report:?}"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn tools_offered_for_png_but_not_avif() {
    let r = registry();
    let png = r.tools_for(&FileKind::Image(ImageFormat::Png), Platform::Android);
    let ids: Vec<_> = png.iter().map(|t| t.id.as_str()).collect();
    for id in ["img.convert", "img.resize", "img.compress", "qr.read"] {
        assert!(ids.contains(&id), "{ids:?}");
    }
    assert!(r
        .tools_for(&FileKind::Image(ImageFormat::Avif), Platform::Desktop)
        .is_empty());
    assert!(r.tools_for(&FileKind::Pdf, Platform::Desktop).is_empty());
    let convert = png.iter().find(|t| t.id == "img.convert").unwrap();
    assert!(convert.options.iter().any(|o| o.key == "quality"));
}

#[test]
fn resize_to_an_unsafe_size_is_refused() {
    let d = TempDir::new().unwrap();
    let src = d.path().join("small.png");
    noise(20, 20).save(&src).unwrap();
    match run(
        "img.resize",
        &src,
        "same",
        serde_json::json!({"mode": "exact", "width": 30000, "height": 30000}),
    ) {
        JobStatus::Failed { message } => assert_eq!(
            message,
            "This image is too large to open safely (900 megapixels)"
        ),
        other => panic!("{other:?}"),
    }
}
