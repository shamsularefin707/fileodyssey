//! QR code and barcode reading from images.

use image::DynamicImage;

/// Every code found in the image, in reading order. Empty if none.
pub fn read_codes(img: &DynamicImage) -> Vec<String> {
    let luma = img.to_luma8();
    let (w, h) = luma.dimensions();
    match rxing::helpers::detect_multiple_in_luma(luma.into_raw(), w, h) {
        Ok(found) => found.into_iter().map(|r| r.getText().to_owned()).collect(),
        Err(_) => Vec::new(),
    }
}
