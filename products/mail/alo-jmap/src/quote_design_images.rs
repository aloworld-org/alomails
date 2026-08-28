//! A studio picture made placeable in a PDF.
//!
//! The PDF writer places JPEGs and nothing else (`alo_pdf::image` — it stays
//! dependency-free, and a reader decodes JPEG natively). A quotation's picture
//! may be a PNG, a WebP or a JPEG in a colour model the writer refuses, so the
//! conversion lives here, next to the one caller that needs it, using the
//! image decoder the workspace already trusts for Sites' derivatives.
//!
//! Two rules: a picture is **never** enlarged beyond what a page can use, and
//! a picture that cannot be read yields `None` rather than an error — the
//! document still prints, with the caption in the picture's place, because a
//! quotation whose print fails on one photo is worse than one with a gap.

use alo_pdf::JpegImage;
use base64::Engine;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{ImageReader, RgbaImage};
use std::io::Cursor;

use crate::quote_design::DataImage;

/// The longest side a placed picture keeps, in pixels: an A4 column at 300 dpi
/// is about 2100 px, and nothing on the page is wider than the column.
const MAX_SIDE: u32 = 2000;
/// JPEG quality for a re-encoded picture — visually lossless on a print.
const QUALITY: u8 = 88;

/// The picture as a JPEG the PDF writer accepts, or `None` when the bytes
/// cannot be read as any image.
#[must_use]
pub fn printable(image: &DataImage) -> Option<JpegImage> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&image.base64)
        .ok()?;
    // The common case costs nothing: a JPEG the writer can place goes in as
    // it is, and a customer's photo is not re-compressed for no reason.
    if image.mime == "image/jpeg"
        && let Ok(ready) = JpegImage::new(bytes.clone())
        && ready.width() <= MAX_SIDE
        && ready.height() <= MAX_SIDE
    {
        return Some(ready);
    }
    re_encode(&bytes)
}

/// Decodes any supported format, flattens transparency onto white (paper is
/// white), scales an oversized picture down, and writes a baseline JPEG.
fn re_encode(bytes: &[u8]) -> Option<JpegImage> {
    let decoded = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    let decoded = if decoded.width() > MAX_SIDE || decoded.height() > MAX_SIDE {
        decoded.resize(MAX_SIDE, MAX_SIDE, FilterType::Triangle)
    } else {
        decoded
    };
    let rgba: RgbaImage = decoded.to_rgba8();
    let mut flattened = image::RgbImage::new(rgba.width(), rgba.height());
    for (x, y, pixel) in rgba.enumerate_pixels() {
        let alpha = u32::from(pixel[3]);
        let over_white = |channel: u8| -> u8 {
            let blended = (u32::from(channel) * alpha + 255 * (255 - alpha)) / 255;
            u8::try_from(blended).unwrap_or(255)
        };
        flattened.put_pixel(
            x,
            y,
            image::Rgb([
                over_white(pixel[0]),
                over_white(pixel[1]),
                over_white(pixel[2]),
            ]),
        );
    }
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, QUALITY)
        .encode_image(&flattened)
        .ok()?;
    JpegImage::new(out).ok()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn data_image(mime: &'static str, bytes: &[u8]) -> DataImage {
        DataImage {
            mime,
            base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        }
    }

    /// A 4×2 PNG with one fully transparent pixel, encoded here so the test
    /// carries no binary fixture.
    fn tiny_png() -> Vec<u8> {
        let mut rgba = RgbaImage::new(4, 2);
        for (x, y, pixel) in rgba.enumerate_pixels_mut() {
            *pixel = if x == 0 && y == 0 {
                image::Rgba([0, 0, 0, 0])
            } else {
                image::Rgba([200, 30, 30, 255])
            };
        }
        let mut out = Vec::new();
        image::DynamicImage::ImageRgba8(rgba)
            .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn a_png_becomes_a_jpeg_of_the_same_size() {
        let jpeg = printable(&data_image("image/png", &tiny_png())).unwrap();
        assert_eq!((jpeg.width(), jpeg.height()), (4, 2));
    }

    #[test]
    fn a_placeable_jpeg_is_passed_through_untouched() {
        // Encode a JPEG, then hand it in as one: the bytes must come back as
        // they went in, not decoded and re-encoded.
        let rgb = image::RgbImage::from_pixel(6, 3, image::Rgb([10, 200, 90]));
        let mut bytes = Vec::new();
        JpegEncoder::new_with_quality(&mut bytes, 80)
            .encode_image(&rgb)
            .unwrap();
        let placed = printable(&data_image("image/jpeg", &bytes)).unwrap();
        assert_eq!((placed.width(), placed.height()), (6, 3));
        assert_eq!(JpegImage::new(bytes).unwrap(), placed);
    }

    #[test]
    fn what_is_not_a_picture_yields_no_picture_rather_than_an_error() {
        assert!(printable(&data_image("image/png", b"not an image at all")).is_none());
        assert!(
            printable(&DataImage {
                mime: "image/png",
                base64: "!!!".into()
            })
            .is_none()
        );
    }
}
