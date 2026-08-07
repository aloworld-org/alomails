//! Colour, in the one space a printed document may use.
//!
//! **DeviceRGB only.** A print shop would want CMYK and a colour profile; a
//! document that is read on a screen, printed on an office laser and archived
//! as a file is better served by the space every one of those three agrees on.
//! PDF/A-3 (B1.22) will want an output intent alongside it, which is additive.

/// An RGB colour, each channel `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    /// Red channel.
    pub r: f64,
    /// Green channel.
    pub g: f64,
    /// Blue channel.
    pub b: f64,
}

impl Color {
    /// A colour from the 8-bit channels a stylesheet is written in, so the PDF
    /// and the HTML page can quote the same `#16181d` at each other.
    #[must_use]
    pub const fn rgb8(r: u8, g: u8, b: u8) -> Self {
        Self {
            r: r as f64 / 255.0,
            g: g as f64 / 255.0,
            b: b as f64 / 255.0,
        }
    }

    /// Pure black.
    pub const BLACK: Self = Self::rgb8(0, 0, 0);
    /// Pure white — the ink of a monogram on a dark square.
    pub const WHITE: Self = Self::rgb8(255, 255, 255);

    /// The colour as PDF operands (`r g b`), rounded the way every number in a
    /// content stream is: three decimals is finer than any output device.
    pub(crate) fn operands(self) -> String {
        format!("{:.3} {:.3} {:.3}", self.r, self.g, self.b)
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::BLACK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_bit_channels_become_the_unit_range_pdf_wants() {
        assert_eq!(Color::BLACK.operands(), "0.000 0.000 0.000");
        assert_eq!(Color::WHITE.operands(), "1.000 1.000 1.000");
        // The document's near-black, #16181d.
        assert_eq!(
            Color::rgb8(0x16, 0x18, 0x1d).operands(),
            "0.086 0.094 0.114"
        );
        assert_eq!(Color::default(), Color::BLACK);
    }
}
