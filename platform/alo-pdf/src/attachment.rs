//! Files carried **inside** the PDF: an embedded file and the specification
//! that names it.
//!
//! A PDF can carry arbitrary bytes alongside its pages. Billing is the first
//! caller and the reason this exists: a Factur-X invoice is one document that
//! is *both* the page a human reads and the machine-readable invoice a
//! bookkeeping system imports, and the second is an attached XML file
//! (`docs/design/billing.md`, B1.22).
//!
//! Two things make an attachment more than a blob:
//!
//! - **`/AFRelationship`** (PDF 2.0 §14.13, and the mechanism PDF/A-3 adopts)
//!   says *how* the file relates to the page — [`Relationship::Alternative`]
//!   for "the same content in another form", which is exactly what an
//!   e-invoice is. A receiving system reads the relationship to decide whether
//!   the attachment is the invoice or a delivery note somebody clipped on.
//! - **The name is part of the contract.** Factur-X mandates the file be
//!   called `factur-x.xml`; a reader looks it up by that name. So the name is
//!   the caller's, and this module never invents or rewrites one.

use crate::writer::PdfDate;

/// How an embedded file relates to the document it travels in.
///
/// Only the values a business document needs. The full PDF 2.0 set also has
/// `Source`, `Data`, `Encrypted` and `Unspecified`; adding one is a line, and
/// adding one that no caller passes is a line that cannot be tested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relationship {
    /// The same content in another form — an e-invoice beside the printed
    /// invoice. This is what Factur-X requires of its XML.
    Alternative,
    /// Material referenced by the document but not part of it — a timesheet
    /// behind an invoice line.
    Supplement,
}

impl Relationship {
    /// The PDF name, without its leading slash.
    #[must_use]
    pub fn as_pdf_name(self) -> &'static str {
        match self {
            Self::Alternative => "Alternative",
            Self::Supplement => "Supplement",
        }
    }
}

/// One file carried inside the document.
#[derive(Debug, Clone)]
pub struct Attachment {
    /// The file name a reader shows and a receiving system looks the file up
    /// by. Written as given: a caller that has to produce `factur-x.xml`
    /// produces exactly that.
    name: String,
    /// The MIME type of the bytes, e.g. `text/xml`.
    mime: String,
    /// How the file relates to the pages.
    relationship: Relationship,
    /// What a reader shows next to the name.
    description: String,
    /// The bytes themselves.
    bytes: Vec<u8>,
    /// The file's modification date, in the document's own time base.
    modified: PdfDate,
}

impl Attachment {
    /// An attachment of `bytes`, named `name`, of MIME type `mime`.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        mime: impl Into<String>,
        relationship: Relationship,
        bytes: Vec<u8>,
        modified: PdfDate,
    ) -> Self {
        Self {
            name: name.into(),
            mime: mime.into(),
            relationship,
            description: String::new(),
            bytes,
            modified,
        }
    }

    /// Sets the description a reader shows beside the file name.
    #[must_use]
    pub fn described(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// The name the file is carried under.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The bytes carried.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The file-specification dictionary (PDF 1.7 §7.11.3), which points at the
    /// stream object holding the bytes.
    ///
    /// `/F` and `/UF` carry the same name — one for readers that predate
    /// Unicode file names and one for the rest — because the name is ASCII by
    /// contract, and a specification whose two names disagreed would be a file
    /// that opens under a different name in different readers.
    pub(crate) fn file_spec(&self, stream: usize) -> String {
        format!(
            "<< /Type /Filespec /F {name} /UF {name} /AFRelationship /{relationship} \
             /Desc {description} /EF << /F {stream} 0 R >> >>",
            name = crate::encoding::pdf_string(&self.name),
            relationship = self.relationship.as_pdf_name(),
            description = crate::encoding::pdf_string(&self.description),
        )
    }

    /// The stream dictionary for the embedded bytes, without the `/Length` the
    /// writer adds as it counts them out.
    ///
    /// `/Subtype` is the MIME type as a PDF name, which means `/` has to be
    /// written `#2F`: `text/xml` is one name, not a name followed by another.
    /// `/Params /Size` is what a reader shows before extracting, and what tells
    /// it the stream was not truncated.
    pub(crate) fn stream_dictionary(&self) -> String {
        format!(
            "/Type /EmbeddedFile /Subtype /{} /Params << /Size {} /ModDate ({}) >>",
            name_escape(&self.mime),
            self.bytes.len(),
            self.modified.as_pdf(),
        )
    }
}

/// Escapes a string for use as a PDF **name** (`/Like#20this`).
///
/// Names have their own grammar: everything outside the printable ASCII range,
/// and the delimiters, are written `#` followed by two hex digits (PDF 1.7
/// §7.3.5). A MIME type is the only name here a caller supplies, and `/` in it
/// is exactly the character that would otherwise start a second name.
fn name_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        let regular = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'+');
        if regular {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("#{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date() -> PdfDate {
        PdfDate::new(2026, 8, 7, 9, 30, 0)
    }

    fn xml() -> Attachment {
        Attachment::new(
            "factur-x.xml",
            "text/xml",
            Relationship::Alternative,
            b"<Invoice/>".to_vec(),
            date(),
        )
        .described("Factur-X invoice")
    }

    #[test]
    fn the_specification_names_the_file_the_way_a_reader_looks_it_up() {
        let spec = xml().file_spec(11);
        // Both name entries, because the two are read by different readers and
        // a file that opens under two names is a file nobody can find.
        assert!(spec.contains("/F (factur-x.xml) /UF (factur-x.xml)"));
        assert!(spec.contains("/AFRelationship /Alternative"));
        assert!(spec.contains("/EF << /F 11 0 R >>"));
        assert!(spec.contains("/Desc (Factur-X invoice)"));
    }

    #[test]
    fn the_mime_type_is_written_as_one_pdf_name() {
        // `/text/xml` would be two names, and the second would be read as the
        // value of the first — the subtype would silently become `/text`.
        assert!(
            xml()
                .stream_dictionary()
                .contains("/Subtype /text#2Fxml /Params << /Size 10")
        );
        assert_eq!(name_escape("application/pdf"), "application#2Fpdf");
        assert_eq!(name_escape("a b(c)"), "a#20b#28c#29");
        assert_eq!(name_escape("text/csv"), "text#2Fcsv");
    }

    #[test]
    fn a_name_a_customer_typed_cannot_close_the_string_it_sits_in() {
        let odd = Attachment::new(
            "in(voice).xml",
            "text/xml",
            Relationship::Supplement,
            Vec::new(),
            date(),
        );
        let spec = odd.file_spec(2);
        assert!(spec.contains("/F (in\\(voice\\).xml)"));
        assert!(spec.contains("/AFRelationship /Supplement"));
        // An empty description is still written, so the dictionary's shape
        // never depends on what a caller left blank.
        assert!(spec.contains("/Desc ()"));
    }

    #[test]
    fn the_size_a_reader_is_told_is_the_size_of_the_bytes() {
        let big = Attachment::new(
            "d.xml",
            "text/xml",
            Relationship::Alternative,
            vec![b'x'; 4096],
            date(),
        );
        assert!(big.stream_dictionary().contains("/Size 4096"));
        assert_eq!(big.bytes().len(), 4096);
        assert_eq!(big.name(), "d.xml");
        assert!(
            big.stream_dictionary()
                .contains("/ModDate (D:20260807093000Z00'00')")
        );
    }
}
