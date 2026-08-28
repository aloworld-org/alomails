//! A picture on a page.
//!
//! **JPEG only, and passed through byte for byte.** PDF readers decode JPEG
//! natively (`/DCTDecode`), so placing one needs no decoding here — only its
//! dimensions, which are read from the frame header. That is what keeps this
//! crate dependency-free while a quotation can carry a product photo. A caller
//! holding a PNG or a WebP converts it first, with whatever decoder it already
//! trusts; the invoice path is still not a place to inherit an image library.
//!
//! Grey and RGB JPEGs are accepted. A CMYK one is refused rather than placed
//! wrong: Adobe writes CMYK JPEGs inverted, and a picture that prints as its
//! own negative is worse than a picture that is not there.

use std::fmt;

/// How a picture is scaled into the frame it is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFit {
    /// The whole picture is visible, centred, with the frame's spare space
    /// left blank.
    Contain,
    /// The frame is filled, centred, and whatever overflows it is clipped.
    Cover,
}

/// A JPEG file, checked and measured, ready to be placed on a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JpegImage {
    bytes: Vec<u8>,
    width: u32,
    height: u32,
    components: u8,
}

/// Why a file could not be taken as a picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageError {
    /// The bytes are not a JPEG file.
    NotJpeg,
    /// The file ends before its frame header does.
    Truncated,
    /// The JPEG has this many colour components; only 1 (grey) and 3 (RGB)
    /// are placed.
    Components(u8),
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotJpeg => write!(f, "not a JPEG file"),
            Self::Truncated => write!(f, "the JPEG file is truncated"),
            Self::Components(n) => write!(f, "a JPEG with {n} colour components cannot be placed"),
        }
    }
}

impl std::error::Error for ImageError {}

impl JpegImage {
    /// Takes a JPEG file, reading its size from the frame header.
    ///
    /// # Errors
    /// [`ImageError`] when the bytes are not a JPEG, end before the frame
    /// header, or use a colour model this crate does not place.
    pub fn new(bytes: Vec<u8>) -> Result<Self, ImageError> {
        let (width, height, components) = frame_header(&bytes)?;
        if components != 1 && components != 3 {
            return Err(ImageError::Components(components));
        }
        if width == 0 || height == 0 {
            return Err(ImageError::Truncated);
        }
        Ok(Self {
            bytes,
            width,
            height,
            components,
        })
    }

    /// Width in pixels.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Width over height.
    #[must_use]
    pub fn aspect(&self) -> f64 {
        f64::from(self.width) / f64::from(self.height)
    }

    /// The file, exactly as given.
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The image dictionary the file is written under.
    pub(crate) fn dictionary(&self) -> String {
        let color_space = if self.components == 1 {
            "DeviceGray"
        } else {
            "DeviceRGB"
        };
        format!(
            "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /{color_space} \
             /BitsPerComponent 8 /Filter /DCTDecode",
            self.width, self.height
        )
    }
}

/// Walks the marker segments to the frame header (`SOF0`–`SOF15`, less the
/// three that are not frames) and reads height, width and component count.
fn frame_header(bytes: &[u8]) -> Result<(u32, u32, u8), ImageError> {
    if bytes.get(..2) != Some(&[0xFF, 0xD8]) {
        return Err(ImageError::NotJpeg);
    }
    let mut at = 2;
    loop {
        let prefix = *bytes.get(at).ok_or(ImageError::Truncated)?;
        if prefix != 0xFF {
            return Err(ImageError::NotJpeg);
        }
        let marker = *bytes.get(at + 1).ok_or(ImageError::Truncated)?;
        match marker {
            // A fill byte before the marker proper.
            0xFF => {
                at += 1;
                continue;
            }
            // Markers that carry no segment.
            0xD8 | 0x01 | 0xD0..=0xD7 => {
                at += 2;
                continue;
            }
            // The scan starts, or the file ends, before any frame header.
            0xD9 | 0xDA => return Err(ImageError::NotJpeg),
            _ => {}
        }
        let length = usize::from(u16::from_be_bytes([
            *bytes.get(at + 2).ok_or(ImageError::Truncated)?,
            *bytes.get(at + 3).ok_or(ImageError::Truncated)?,
        ]));
        if length < 2 {
            return Err(ImageError::NotJpeg);
        }
        if is_frame_header(marker) {
            let segment = bytes
                .get(at + 4..at + 2 + length)
                .ok_or(ImageError::Truncated)?;
            let [_, h1, h2, w1, w2, components, ..] = segment else {
                return Err(ImageError::Truncated);
            };
            return Ok((
                u32::from(u16::from_be_bytes([*w1, *w2])),
                u32::from(u16::from_be_bytes([*h1, *h2])),
                *components,
            ));
        }
        at += 2 + length;
    }
}

/// `SOF0`–`SOF15`, minus `DHT` (`C4`), `JPG` (`C8`) and `DAC` (`CC`), which
/// share the range but are not frames.
fn is_frame_header(marker: u8) -> bool {
    matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF)
}

#[cfg(test)]
pub(crate) mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// A JPEG file reduced to its skeleton: SOI, an APP0 segment, a baseline
    /// frame header naming `width` × `height` with `components` channels, then
    /// the start of the scan. Enough for every reader of the header.
    pub(crate) fn skeleton(width: u16, height: u16, components: u8) -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xD8];
        bytes.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]);
        bytes.extend_from_slice(b"JFIF\0\x01\x01\x00\x00\x01\x00\x01\x00\x00");
        let length = 8 + 3 * u16::from(components);
        bytes.extend_from_slice(&[0xFF, 0xC0]);
        bytes.extend_from_slice(&length.to_be_bytes());
        bytes.push(8);
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.push(components);
        for component in 1..=components {
            bytes.extend_from_slice(&[component, 0x11, 0x00]);
        }
        bytes.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x08]);
        bytes
    }

    #[test]
    fn the_frame_header_gives_the_size_and_the_colour_model() {
        let image = JpegImage::new(skeleton(640, 480, 3)).unwrap();
        assert_eq!((image.width(), image.height()), (640, 480));
        assert!((image.aspect() - 4.0 / 3.0).abs() < 1e-9);
        assert!(image.dictionary().contains("/ColorSpace /DeviceRGB"));
        assert!(image.dictionary().contains("/Filter /DCTDecode"));

        let grey = JpegImage::new(skeleton(10, 20, 1)).unwrap();
        assert!(grey.dictionary().contains("/ColorSpace /DeviceGray"));
    }

    #[test]
    fn what_is_not_a_placeable_jpeg_is_refused_by_name() {
        assert_eq!(
            JpegImage::new(b"\x89PNG\r\n".to_vec()),
            Err(ImageError::NotJpeg)
        );
        assert_eq!(JpegImage::new(Vec::new()), Err(ImageError::NotJpeg));
        let mut cut = skeleton(640, 480, 3);
        cut.truncate(12);
        assert_eq!(JpegImage::new(cut), Err(ImageError::Truncated));
        assert_eq!(
            JpegImage::new(skeleton(640, 480, 4)),
            Err(ImageError::Components(4))
        );
        assert_eq!(
            JpegImage::new(skeleton(0, 480, 3)),
            Err(ImageError::Truncated)
        );
        // A scan that begins before any frame header is not a picture we can
        // measure.
        let scan_first = vec![0xFF, 0xD8, 0xFF, 0xDA, 0x00, 0x08];
        assert_eq!(JpegImage::new(scan_first), Err(ImageError::NotJpeg));
    }

    #[test]
    fn a_progressive_frame_header_reads_the_same() {
        let mut bytes = skeleton(300, 200, 3);
        // Turn SOF0 into SOF2 (progressive DCT).
        let sof = bytes.windows(2).position(|w| w == [0xFF, 0xC0]).unwrap();
        bytes[sof + 1] = 0xC2;
        let image = JpegImage::new(bytes).unwrap();
        assert_eq!((image.width(), image.height()), (300, 200));
    }
}
