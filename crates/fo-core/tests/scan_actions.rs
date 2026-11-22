//! Scan actions: codes in an image, links safe to open, saving a contact,
//! and file names from the UI.
use fo_core::scan::{codes_in_image, openable_url, safe_stem, save_contact};

fn qr_png(dir: &std::path::Path, text: &str) -> std::path::PathBuf {
    use rxing::Writer;
    let m = rxing::qrcode::QRCodeWriter
        .encode(text, &rxing::BarcodeFormat::QR_CODE, 240, 240)
        .unwrap();
    let img = image::GrayImage::from_fn(240, 240, |x, y| {
        image::Luma([if m.get(x, y) { 0 } else { 255 }])
    });
    let p = dir.join("code.png");
    img.save(&p).unwrap();
    p
}

#[test]
fn reads_a_code_from_an_image_file() {
    let d = tempfile::TempDir::new().unwrap();
    let p = qr_png(d.path(), "WIFI:S:Cafe;T:WPA;P:secret;;");
    assert_eq!(
        codes_in_image(&p).unwrap(),
        vec!["WIFI:S:Cafe;T:WPA;P:secret;;".to_string()]
    );
    let blank = d.path().join("blank.png");
    image::GrayImage::from_pixel(50, 50, image::Luma([255]))
        .save(&blank)
        .unwrap();
    assert!(codes_in_image(&blank).unwrap().is_empty());
}

#[test]
fn only_web_links_open() {
    assert_eq!(
        openable_url("https://example.org").unwrap(),
        "https://example.org"
    );
    for bad in [
        "javascript:alert(1)",
        "intent://x",
        "file:///sdcard/a",
        "content://x",
        "tel:123",
    ] {
        assert!(openable_url(bad).is_err(), "{bad}");
    }
}

#[test]
fn contact_saved_as_vcf_without_overwriting() {
    let d = tempfile::TempDir::new().unwrap();
    let card = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Arefin/Hossain\r\nEND:VCARD\r\n";
    let a = save_contact(card, d.path()).unwrap();
    let b = save_contact(card, d.path()).unwrap();
    assert_eq!(a.file_name().unwrap(), "Arefin Hossain.vcf");
    assert_eq!(b.file_name().unwrap(), "Arefin Hossain (2).vcf");
    assert_eq!(std::fs::read_to_string(&a).unwrap(), card);
    assert!(save_contact("not a vcard", d.path()).is_err());
}

#[test]
fn stems_from_the_ui_are_plain_names() {
    assert_eq!(safe_stem("Scan 2026-11-22 14.05"), "Scan 2026-11-22 14.05");
    assert_eq!(safe_stem("../../etc/passwd"), "etc passwd");
    assert_eq!(safe_stem("a\\b:c*?\"<>|d"), "a b c d");
    assert_eq!(safe_stem("   "), "Scan");
}
