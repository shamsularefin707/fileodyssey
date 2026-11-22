//! Minimal image-only PDF writer: one image per page.
//! JPEG sources are embedded byte-for-byte (DCTDecode); everything else is
//! stored losslessly with Flate, with an SMask for transparency.

use crate::tool::ToolError;
use image::DynamicImage;
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref};
use std::path::Path;
use std::sync::Arc;

pub struct PdfPage {
    pub image: DynamicImage,
    /// Original JPEG bytes to embed unchanged, when the source was a JPEG.
    pub jpeg: Option<Arc<[u8]>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageSize {
    /// Page is the image size at 72 dpi.
    Image,
    A4,
    Letter,
    /// A4 wide, as tall as the image's shape needs, no margin: for scanned
    /// pages already cropped to the paper.
    ScanA4Width,
}

const MM: f32 = 72.0 / 25.4;

impl PageSize {
    fn dims(self, w: f32, h: f32) -> (f32, f32) {
        match self {
            PageSize::Image => (w, h),
            PageSize::A4 => (210.0 * MM, 297.0 * MM),
            PageSize::Letter => (8.5 * 72.0, 11.0 * 72.0),
            PageSize::ScanA4Width => (210.0 * MM, 210.0 * MM * h / w.max(1.0)),
        }
    }
}

fn zlib(data: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec_zlib(data, 6)
}

/// Writes `count` pages, asking `next_page(i)` for each one just before it is
/// written, so only one decoded image is in memory at a time.
pub fn write_pdf(
    count: usize,
    size: PageSize,
    out: &Path,
    mut next_page: impl FnMut(usize) -> Result<PdfPage, ToolError>,
) -> Result<(), ToolError> {
    let mut pdf = Pdf::new();
    let mut next = 1;
    let mut alloc = || {
        let r = Ref::new(next);
        next += 1;
        r
    };
    let catalog = alloc();
    let tree = alloc();
    let ids: Vec<(Ref, Ref, Ref, Ref)> = (0..count)
        .map(|_| (alloc(), alloc(), alloc(), alloc()))
        .collect();

    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree)
        .kids(ids.iter().map(|i| i.0))
        .count(count as i32);

    for (i, &(page_id, content_id, image_id, mask_id)) in ids.iter().enumerate() {
        let p = next_page(i)?;
        let (iw, ih) = (p.image.width() as f32, p.image.height() as f32);
        let (pw, ph) = size.dims(iw, ih);
        // Fit inside the page with 10 mm margins (none for image-sized pages).
        let margin = if matches!(size, PageSize::Image | PageSize::ScanA4Width) {
            0.0
        } else {
            10.0 * MM
        };
        let scale = ((pw - 2.0 * margin) / iw)
            .min((ph - 2.0 * margin) / ih)
            .min(if size == PageSize::Image {
                1.0
            } else {
                f32::MAX
            });
        let (dw, dh) = (iw * scale, ih * scale);
        let (x, y) = ((pw - dw) / 2.0, (ph - dh) / 2.0);

        let mut page = pdf.page(page_id);
        page.media_box(Rect::new(0.0, 0.0, pw, ph))
            .parent(tree)
            .contents(content_id);
        page.resources().x_objects().pair(Name(b"Im0"), image_id);
        page.finish();

        let mut c = Content::new();
        c.save_state()
            .transform([dw, 0.0, 0.0, dh, x, y])
            .x_object(Name(b"Im0"))
            .restore_state();
        pdf.stream(content_id, &c.finish());

        let gray = matches!(
            p.image.color(),
            image::ColorType::L8 | image::ColorType::L16
        );
        let has_alpha = p.image.color().has_alpha();
        if let Some(jpeg) = &p.jpeg {
            let mut x = pdf.image_xobject(image_id, jpeg);
            x.filter(Filter::DctDecode);
            x.width(iw as i32).height(ih as i32).bits_per_component(8);
            if gray {
                x.color_space().device_gray();
            } else {
                x.color_space().device_rgb();
            }
            x.finish();
        } else {
            let rgb = p.image.to_rgb8();
            let data = zlib(rgb.as_raw());
            let mut x = pdf.image_xobject(image_id, &data);
            x.filter(Filter::FlateDecode);
            x.width(iw as i32).height(ih as i32).bits_per_component(8);
            x.color_space().device_rgb();
            if has_alpha {
                x.s_mask(mask_id);
            }
            x.finish();
            if has_alpha {
                let alpha: Vec<u8> = p.image.to_rgba8().pixels().map(|px| px.0[3]).collect();
                let data = zlib(&alpha);
                let mut m = pdf.image_xobject(mask_id, &data);
                m.filter(Filter::FlateDecode);
                m.width(iw as i32).height(ih as i32).bits_per_component(8);
                m.color_space().device_gray();
                m.finish();
            }
        }
    }
    std::fs::write(out, pdf.finish())
        .map_err(|e| ToolError::Failed(format!("Couldn't save the PDF: {e}")))
}
