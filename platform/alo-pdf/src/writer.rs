//! The file itself: objects, the cross-reference table, and the trailer.
//!
//! A PDF is a list of numbered objects followed by a table of the byte offset
//! of each one. That table is the whole reason a PDF cannot be assembled by
//! concatenation — every object has to be counted as it is written — and it is
//! the whole content of this module.
//!
//! What is produced is **PDF 1.7, uncompressed**: an invoice is a few
//! kilobytes of text either way, and a file a human can read in an editor is a
//! file whose bugs can be seen. The page content stays 7-bit clean; an
//! attachment ([`crate::attachment`]) is carried byte for byte, because the
//! bytes of an e-invoice are the caller's and re-encoding them would change
//! the document a receiving system parses.
//!
//! **Not yet PDF/A-3.** Factur-X asks for that conformance level, and two of
//! its requirements are not this crate's to decide — an embedded font file and
//! an output-intent ICC profile are binaries that need a human's licence
//! choice (`docs/design/billing.md`, B1.17). What is here is everything else
//! the hybrid document needs: the attachment, its `/AFRelationship`, the
//! `/AF` array on the catalogue, the embedded-files name tree, and an XMP
//! metadata stream. Nothing written here *claims* PDF/A conformance, so the
//! file is honest about what it is.

use crate::attachment::Attachment;
use crate::canvas::Canvas;
use crate::font::Font;
use crate::metrics::{FIRST_CHAR, LAST_CHAR};

/// Object number of the document catalogue.
const CATALOG: usize = 1;
/// Object number of the page tree.
const PAGES: usize = 2;
/// Object number of the document information dictionary.
const INFO: usize = 3;
/// Object number of the first font; each face occupies two objects (the font
/// and its descriptor), in [`Font::ALL`] order.
const FIRST_FONT: usize = 4;
/// The first object number a page may use.
const FIRST_PAGE_OBJECT: usize = FIRST_FONT + 2 * 2;

/// A calendar instant, as a PDF document date.
///
/// Plain integers rather than a date type, so this crate stays
/// dependency-free and — more usefully — so a caller must decide what
/// "created" means for its document. A route that stamps the wall clock and a
/// test that stamps a fixed instant then produce byte-identical files for the
/// same input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfDate {
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
}

impl PdfDate {
    /// A UTC instant. Out-of-range components are clamped rather than
    /// refused: a wrong minute in a metadata field is never a reason to fail
    /// to produce a customer's invoice.
    #[must_use]
    pub fn new(year: i32, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> Self {
        Self {
            year: year.clamp(0, 9999),
            month: month.clamp(1, 12),
            day: day.clamp(1, 31),
            hour: hour.min(23),
            minute: minute.min(59),
            second: second.min(60),
        }
    }

    /// The `D:YYYYMMDDHHmmSS` form with an explicit UTC offset (PDF 1.7
    /// §7.9.4). Everything this crate produces is stamped in UTC.
    pub(crate) fn as_pdf(self) -> String {
        format!(
            "D:{:04}{:02}{:02}{:02}{:02}{:02}Z00'00'",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

/// A document being assembled.
pub struct Pdf {
    /// `/Title` in the information dictionary, and what a reader puts in its
    /// window title.
    title: String,
    /// `/CreationDate` and `/ModDate`.
    created: PdfDate,
    /// The pages, in order.
    pages: Vec<Canvas>,
    /// The files carried inside the document, in the order they were attached.
    attachments: Vec<Attachment>,
    /// The XMP packet describing the document, when the caller supplied one.
    metadata: Option<String>,
}

impl Pdf {
    /// An empty document.
    #[must_use]
    pub fn new(title: impl Into<String>, created: PdfDate) -> Self {
        Self {
            title: title.into(),
            created,
            pages: Vec::new(),
            attachments: Vec::new(),
            metadata: None,
        }
    }

    /// Appends a page.
    pub fn add_page(&mut self, page: Canvas) {
        self.pages.push(page);
    }

    /// How many pages have been added.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Carries a file inside the document ([`Attachment`]).
    ///
    /// The order attachments are added is the order the document's `/AF` array
    /// holds them in, and a name is never made unique by this crate: a caller that attaches
    /// two files called the same thing has a bug in the caller, and silently
    /// renaming one would hide it from the receiving system that looks the
    /// file up by name.
    pub fn attach(&mut self, attachment: Attachment) {
        self.attachments.push(attachment);
    }

    /// Sets the XMP metadata packet (ISO 16684-1) describing the document.
    ///
    /// Passed in as a string rather than built here: XMP is a vocabulary
    /// question — what a *billing* document says about itself is billing's to
    /// state — and this crate's job is to carry the packet in a stream a
    /// reader will find.
    pub fn set_metadata(&mut self, xmp: impl Into<String>) {
        self.metadata = Some(xmp.into());
    }

    /// Serialises the document.
    ///
    /// A document with no pages still produces a **valid** file — an empty
    /// page tree — rather than an empty response: a caller that produced
    /// nothing should see a PDF that says so, not a truncated download.
    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        // The binary comment on line 2 is what tells a transfer program this
        // file is not text (PDF 1.7 §7.5.2).
        out.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");

        // Everything after the pages, numbered before anything is written: the
        // catalogue is object 1 and has to name objects that do not exist yet.
        let after_pages = FIRST_PAGE_OBJECT + self.pages.len() * 2;
        // The pictures follow the pages: one object each, in page order.
        let image_count: usize = self.pages.iter().map(|page| page.images().len()).sum();
        let after_images = after_pages + image_count;
        let metadata_object = self.metadata.as_ref().map(|_| after_images);
        let first_attachment = after_images + usize::from(self.metadata.is_some());
        let object_count = first_attachment + self.attachments.len() * 2;
        let mut offsets = vec![0usize; object_count];

        let kids: String = (0..self.pages.len())
            .map(|i| format!("{} 0 R ", FIRST_PAGE_OBJECT + i * 2))
            .collect();
        push_object(
            &mut out,
            &mut offsets,
            CATALOG,
            &format!(
                "<< /Type /Catalog /Pages {PAGES} 0 R{}{} >>",
                metadata_object.map_or_else(String::new, |n| format!(" /Metadata {n} 0 R")),
                attachment_entries(&self.attachments, first_attachment),
            ),
        );
        push_object(
            &mut out,
            &mut offsets,
            PAGES,
            &format!(
                "<< /Type /Pages /Count {} /Kids [{}] >>",
                self.pages.len(),
                kids.trim_end()
            ),
        );
        push_object(
            &mut out,
            &mut offsets,
            INFO,
            &format!(
                "<< /Title {} /Producer (alo workplace) /Creator (alo billing) \
                 /CreationDate ({}) /ModDate ({}) >>",
                crate::encoding::pdf_string(&self.title),
                self.created.as_pdf(),
                self.created.as_pdf(),
            ),
        );
        for (index, font) in Font::ALL.into_iter().enumerate() {
            let font_object = FIRST_FONT + index * 2;
            let descriptor_object = font_object + 1;
            push_object(
                &mut out,
                &mut offsets,
                font_object,
                &font_dictionary(font, descriptor_object),
            );
            push_object(
                &mut out,
                &mut offsets,
                descriptor_object,
                &font_descriptor(font),
            );
        }

        let resources = Font::ALL
            .into_iter()
            .enumerate()
            .map(|(index, font)| format!("/{} {} 0 R ", font.resource(), FIRST_FONT + index * 2))
            .collect::<String>();
        let mut next_image = after_pages;
        for (index, page) in self.pages.iter().enumerate() {
            let page_object = FIRST_PAGE_OBJECT + index * 2;
            let content_object = page_object + 1;
            // The page's pictures, named as its content stream names them.
            let xobjects: String = (0..page.images().len())
                .map(|i| format!("/Im{} {} 0 R ", i + 1, next_image + i))
                .collect();
            let xobject_entry = if xobjects.is_empty() {
                String::new()
            } else {
                format!(" /XObject << {} >>", xobjects.trim_end())
            };
            push_object(
                &mut out,
                &mut offsets,
                page_object,
                &format!(
                    "<< /Type /Page /Parent {PAGES} 0 R /MediaBox [0 0 {:.4} {:.4}] \
                     /Resources << /Font << {} >>{xobject_entry} >> \
                     /Contents {content_object} 0 R >>",
                    page.width(),
                    page.height(),
                    resources.trim_end(),
                ),
            );
            push_stream(&mut out, &mut offsets, content_object, page.content());
            for image in page.images() {
                push_binary_stream(
                    &mut out,
                    &mut offsets,
                    next_image,
                    &image.dictionary(),
                    image.bytes(),
                );
                next_image += 1;
            }
        }

        if let (Some(number), Some(xmp)) = (metadata_object, self.metadata.as_ref()) {
            push_binary_stream(
                &mut out,
                &mut offsets,
                number,
                "/Type /Metadata /Subtype /XML",
                xmp.as_bytes(),
            );
        }
        for (index, attachment) in self.attachments.iter().enumerate() {
            let spec_object = first_attachment + index * 2;
            let stream_object = spec_object + 1;
            push_object(
                &mut out,
                &mut offsets,
                spec_object,
                &attachment.file_spec(stream_object),
            );
            push_binary_stream(
                &mut out,
                &mut offsets,
                stream_object,
                &attachment.stream_dictionary(),
                attachment.bytes(),
            );
        }

        let start_xref = out.len();
        let id = file_id(&out);
        out.extend_from_slice(format!("xref\n0 {object_count}\n").as_bytes());
        out.extend_from_slice(b"0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {object_count} /Root {CATALOG} 0 R /Info {INFO} 0 R \
                 /ID [<{id}> <{id}>] >>\nstartxref\n{start_xref}\n%%EOF\n"
            )
            .as_bytes(),
        );
        out
    }
}

/// Writes one object, recording where it started.
fn push_object(out: &mut Vec<u8>, offsets: &mut [usize], number: usize, body: &str) {
    if let Some(slot) = offsets.get_mut(number) {
        *slot = out.len();
    }
    out.extend_from_slice(format!("{number} 0 obj\n{body}\nendobj\n").as_bytes());
}

/// Writes one stream object — a dictionary carrying the byte length, then the
/// bytes.
fn push_stream(out: &mut Vec<u8>, offsets: &mut [usize], number: usize, content: &str) {
    push_binary_stream(out, offsets, number, "", content.as_bytes());
}

/// Writes one stream object of arbitrary bytes, with `dictionary` prepended to
/// the `/Length` the bytes are counted into.
///
/// Byte for byte: an attached e-invoice is parsed by somebody else's system,
/// and a producer that re-encoded it would be changing a document it does not
/// own.
fn push_binary_stream(
    out: &mut Vec<u8>,
    offsets: &mut [usize],
    number: usize,
    dictionary: &str,
    content: &[u8],
) {
    if let Some(slot) = offsets.get_mut(number) {
        *slot = out.len();
    }
    let separator = if dictionary.is_empty() { "" } else { " " };
    out.extend_from_slice(
        format!(
            "{number} 0 obj\n<< {dictionary}{separator}/Length {} >>\nstream\n",
            content.len()
        )
        .as_bytes(),
    );
    out.extend_from_slice(content);
    out.extend_from_slice(b"endstream\nendobj\n");
}

/// The catalogue entries that make attached files findable, or nothing at all
/// when the document carries none.
///
/// Two entries, and a reader needs **both**: `/Names /EmbeddedFiles` is the
/// name tree an "attachments" pane lists and a receiving system looks
/// `factur-x.xml` up in, while `/AF` is the associated-files array that says
/// the attachments belong to the document as a whole rather than to a page.
/// A file in only one of them is a file half the world cannot find.
///
/// The name tree is written **sorted by name**, which PDF 1.7 §7.9.6 requires
/// of every name tree — a reader is allowed to binary-search it — while `/AF`
/// keeps the order the caller attached in. So a document with two attachments
/// lists them in the caller's order and still resolves a name lookup.
fn attachment_entries(attachments: &[Attachment], first: usize) -> String {
    if attachments.is_empty() {
        return String::new();
    }
    let mut sorted: Vec<(usize, &Attachment)> = attachments.iter().enumerate().collect();
    sorted.sort_by(|(_, a), (_, b)| a.name().cmp(b.name()));
    let names: String = sorted
        .into_iter()
        .map(|(index, a)| {
            format!(
                "{} {} 0 R ",
                crate::encoding::pdf_string(a.name()),
                first + index * 2
            )
        })
        .collect();
    let array: String = (0..attachments.len())
        .map(|index| format!("{} 0 R ", first + index * 2))
        .collect();
    format!(
        " /AF [{}] /Names << /EmbeddedFiles << /Names [{}] >> >>",
        array.trim_end(),
        names.trim_end(),
    )
}

/// The font dictionary for one face.
///
/// `/FirstChar`, `/LastChar`, `/Widths` and `/FontDescriptor` are present
/// together, which PDF 1.7 §9.6.2.1 requires of the standard-14 fonts: all
/// four or none. Declaring them means a reader is told the same widths this
/// crate measured with, rather than inferring them from whichever font it
/// substitutes.
fn font_dictionary(font: Font, descriptor: usize) -> String {
    let widths: String = (FIRST_CHAR..=LAST_CHAR)
        .map(|code| format!("{} ", font.metrics().width(code)))
        .collect();
    format!(
        "<< /Type /Font /Subtype /Type1 /BaseFont /{} /Encoding /WinAnsiEncoding \
         /FirstChar {FIRST_CHAR} /LastChar {LAST_CHAR} /Widths [{}] \
         /FontDescriptor {descriptor} 0 R >>",
        font.metrics().base_font,
        widths.trim_end(),
    )
}

/// The font descriptor for one face. `/Flags 32` is "nonsymbolic": the font
/// uses the standard Latin character set, which is what makes
/// `/WinAnsiEncoding` meaningful. There is no `/FontFile`: these fourteen
/// fonts are the ones a reader already has (and the ones PDF/A will make us
/// embed at B1.22).
fn font_descriptor(font: Font) -> String {
    let m = font.metrics();
    format!(
        "<< /Type /FontDescriptor /FontName /{} /Flags 32 \
         /FontBBox [{} {} {} {}] /ItalicAngle 0 /Ascent {} /Descent {} \
         /CapHeight {} /StemV {} >>",
        m.base_font,
        m.bbox[0],
        m.bbox[1],
        m.bbox[2],
        m.bbox[3],
        m.ascent,
        m.descent,
        m.cap_height,
        m.stem_v,
    )
}

/// The file identifier: a 64-bit FNV-1a over everything written so far, as
/// hex.
///
/// PDF wants two strings, the first of which never changes for a file's
/// lifetime; nothing here ever updates a file in place, so both are the same.
/// It is a **fingerprint, not a secret** — a hash the same input always
/// produces, so two renderings of one invoice are byte-identical and a golden
/// test is possible at all.
fn file_id(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Canvas;
    use crate::text::{Align, TextStyle};

    fn date() -> PdfDate {
        PdfDate::new(2026, 8, 7, 9, 30, 0)
    }

    fn one_page() -> Vec<u8> {
        let mut page = Canvas::a4();
        page.text(
            50.0,
            50.0,
            Align::Left,
            &TextStyle::new(Font::Regular, 10.0),
            "Invoice INV-2026-00001",
        );
        let mut pdf = Pdf::new("Invoice INV-2026-00001", date());
        pdf.add_page(page);
        pdf.finish()
    }

    /// Reads the cross-reference table back and returns the byte offset each
    /// entry claims, so a test can check the claim rather than trust it.
    fn xref_offsets(bytes: &[u8]) -> Vec<usize> {
        let text = String::from_utf8_lossy(bytes);
        // `\nxref\n` occurs exactly once — `startxref` does not contain it.
        let table = text.split_once("\nxref\n").map_or(String::new(), |(_, t)| {
            t.split("trailer").next().unwrap_or_default().to_owned()
        });
        table
            .lines()
            .skip(1)
            .filter(|line| line.ends_with(" n "))
            .filter_map(|line| line.split_whitespace().next()?.parse().ok())
            .collect()
    }

    #[test]
    fn the_file_is_a_pdf_from_its_first_byte_to_its_last() {
        let bytes = one_page();
        assert!(bytes.starts_with(b"%PDF-1.7\n"));
        // The binary marker that stops a transfer mangling it as text.
        assert!(bytes[9..14].iter().any(|b| *b > 0x7F));
        assert!(bytes.ends_with(b"%%EOF\n"));
        assert!(bytes.len() > 1000, "a real page is not a stub");
    }

    #[test]
    fn every_cross_reference_offset_points_at_the_object_it_claims() {
        // The one structural invariant a PDF has: if these are wrong, some
        // readers open the file anyway by scanning it, and others do not —
        // which is the worst possible way to be wrong.
        let bytes = one_page();
        let offsets = xref_offsets(&bytes);
        assert_eq!(offsets.len(), FIRST_PAGE_OBJECT + 1, "9 objects for a page");
        for (index, offset) in offsets.iter().enumerate() {
            let number = index + 1;
            let expected = format!("{number} 0 obj");
            assert!(
                bytes[*offset..].starts_with(expected.as_bytes()),
                "object {number} is not at {offset}",
            );
        }
        // …and startxref points at the table itself.
        let text = String::from_utf8_lossy(&bytes);
        let start: usize = text
            .rsplit_once("startxref\n")
            .and_then(|(_, rest)| rest.split_whitespace().next())
            .and_then(|n| n.parse().ok())
            .unwrap_or_default();
        assert!(bytes[start..].starts_with(b"xref\n"));
    }

    #[test]
    fn the_content_stream_declares_its_own_length() {
        // A /Length that disagrees with the stream is how a page renders
        // blank in one reader and correctly in another.
        let bytes = one_page();
        let text = String::from_utf8_lossy(&bytes);
        let (before, rest) = text
            .split_once("stream\n")
            .unwrap_or_else(|| panic!("no stream in the file"));
        let declared: usize = before
            .rsplit_once("/Length ")
            .and_then(|(_, n)| n.split(' ').next()?.trim_end_matches(" >>").parse().ok())
            .unwrap_or_default();
        let actual = rest.split_once("endstream").map_or(0, |(s, _)| s.len());
        assert_eq!(declared, actual, "declared length is not the stream length");
        assert!(actual > 0);
    }

    #[test]
    fn both_faces_are_declared_with_the_widths_this_crate_measured_with() {
        let bytes = one_page();
        let text = String::from_utf8_lossy(&bytes);
        for name in ["/BaseFont /Helvetica ", "/BaseFont /Helvetica-Bold "] {
            assert!(text.contains(name), "missing {name}");
        }
        assert_eq!(text.matches("/Encoding /WinAnsiEncoding").count(), 2);
        assert_eq!(text.matches("/Type /FontDescriptor").count(), 2);
        // 224 widths per face, from space to ÿ, exactly as the tables hold.
        for dictionary in text.split("/Widths [").skip(1) {
            let widths = dictionary.split(']').next().unwrap_or_default();
            assert_eq!(widths.split_whitespace().count(), 224);
        }
        assert!(text.contains("/FirstChar 32 /LastChar 255"));
        // Both faces are on every page's resources, so a resource name means
        // the same face wherever it appears.
        assert!(text.contains("/Font << /F1 4 0 R /F2 6 0 R >>"));
    }

    #[test]
    fn the_same_document_always_produces_the_same_bytes() {
        // Determinism is what makes a golden test of a rendered invoice
        // possible, and what stops a re-download differing from the file a
        // customer already has.
        assert_eq!(one_page(), one_page());
        // …and a different document differs, including in its id.
        let mut other = Pdf::new("Quote QUO-2026-00001", date());
        other.add_page(Canvas::a4());
        assert_ne!(one_page(), other.finish());
    }

    #[test]
    fn a_multi_page_document_numbers_its_pages_in_order() {
        let mut pdf = Pdf::new("Invoice", date());
        for _ in 0..3 {
            pdf.add_page(Canvas::a4());
        }
        assert_eq!(pdf.page_count(), 3);
        let bytes = pdf.finish();
        let text = String::from_utf8_lossy(&bytes);
        assert_eq!(text.matches("/Type /Page ").count(), 3);
        assert!(text.contains("/Count 3 /Kids [8 0 R 10 0 R 12 0 R]"));
        let offsets = xref_offsets(&bytes);
        assert_eq!(offsets.len(), FIRST_PAGE_OBJECT + 5);
    }

    #[test]
    fn a_document_with_no_pages_is_still_a_readable_file() {
        let bytes = Pdf::new("Nothing", date()).finish();
        assert!(bytes.starts_with(b"%PDF-1.7"));
        assert!(bytes.ends_with(b"%%EOF\n"));
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("/Count 0 /Kids []"));
        for (index, offset) in xref_offsets(&bytes).iter().enumerate() {
            let expected = format!("{} 0 obj", index + 1);
            assert!(bytes[*offset..].starts_with(expected.as_bytes()));
        }
    }

    #[test]
    fn the_title_is_carried_and_escaped_like_any_other_string() {
        let mut pdf = Pdf::new("Invoice (INV-1) \\ Söhne", date());
        pdf.add_page(Canvas::a4());
        let bytes = pdf.finish();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("/Title (Invoice \\(INV-1\\) \\\\ S\\366hne)"));
        assert!(text.contains("/CreationDate (D:20260807093000Z00'00')"));
        // Past the deliberate binary marker on line 2, nothing is high-bit.
        assert!(
            bytes[14..].is_ascii(),
            "the file must stay 7-bit past its header"
        );
    }

    /// A one-page document carrying an XML attachment and an XMP packet — the
    /// hybrid invoice's shape.
    fn with_attachment(xml: &[u8]) -> Vec<u8> {
        let mut pdf = Pdf::new("Invoice INV-2026-00001", date());
        pdf.add_page(Canvas::a4());
        pdf.set_metadata("<?xpacket begin=\"\"?><x:xmpmeta/><?xpacket end=\"w\"?>");
        pdf.attach(
            Attachment::new(
                "factur-x.xml",
                "text/xml",
                crate::attachment::Relationship::Alternative,
                xml.to_vec(),
                date(),
            )
            .described("Factur-X invoice"),
        );
        pdf.finish()
    }

    #[test]
    fn an_attached_file_is_findable_by_name_and_by_the_document() {
        let bytes = with_attachment(b"<Invoice/>");
        let text = String::from_utf8_lossy(&bytes);
        // One page (objects 8, 9), then the metadata (10), then the file
        // specification (11) and the bytes (12).
        assert!(text.contains("/Metadata 10 0 R"));
        assert!(text.contains("/AF [11 0 R]"));
        assert!(text.contains("/Names << /EmbeddedFiles << /Names [(factur-x.xml) 11 0 R] >> >>"));
        assert!(text.contains("/Type /Filespec"));
        assert!(text.contains("/EF << /F 12 0 R >>"));
        assert!(text.contains("/Type /Metadata /Subtype /XML /Length 51"));
        // Every offset still points at the object it claims — the one
        // structural invariant, now over four more objects.
        for (index, offset) in xref_offsets(&bytes).iter().enumerate() {
            let expected = format!("{} 0 obj", index + 1);
            assert!(
                bytes[*offset..].starts_with(expected.as_bytes()),
                "object {} is not at {offset}",
                index + 1
            );
        }
        assert_eq!(xref_offsets(&bytes).len(), 12);
    }

    #[test]
    fn the_attached_bytes_are_carried_exactly_as_given() {
        // A receiving system parses these bytes and checks them against a
        // schema; a producer that re-encoded them would be rewriting somebody
        // else's document.
        let xml = "<Invoice><Name>Émile Zola &amp; Cie</Name></Invoice>".as_bytes();
        let bytes = with_attachment(xml);
        assert!(
            bytes.windows(xml.len()).any(|w| w == xml),
            "the attached bytes are not in the file verbatim"
        );
        assert!(String::from_utf8_lossy(&bytes).contains(&format!("/Size {}", xml.len())));
        // …and the declared length is the byte length, not the character count.
        assert!(String::from_utf8_lossy(&bytes).contains(&format!("/Length {}", xml.len())));
        assert_eq!(with_attachment(xml), bytes, "still deterministic");
    }

    #[test]
    fn a_document_carrying_nothing_is_written_exactly_as_before() {
        // The attachment machinery must be invisible to a plain document: no
        // /AF, no /Names, no /Metadata, and the same object count.
        let text = String::from_utf8_lossy(&one_page()).into_owned();
        assert!(text.contains("<< /Type /Catalog /Pages 2 0 R >>"));
        assert!(!text.contains("/AF ["));
        assert!(!text.contains("/Metadata"));
    }

    #[test]
    fn the_embedded_file_name_tree_is_sorted_even_when_the_caller_is_not() {
        // A name tree a reader may binary-search has to be in name order,
        // while /AF keeps the order the caller attached in.
        let mut pdf = Pdf::new("Two", date());
        pdf.add_page(Canvas::a4());
        for name in ["zeta.xml", "alpha.xml"] {
            pdf.attach(Attachment::new(
                name,
                "text/xml",
                crate::attachment::Relationship::Supplement,
                b"<x/>".to_vec(),
                date(),
            ));
        }
        let text = String::from_utf8_lossy(&pdf.finish()).into_owned();
        assert!(text.contains("/AF [10 0 R 12 0 R]"));
        assert!(text.contains("/Names [(alpha.xml) 12 0 R (zeta.xml) 10 0 R]"));
    }

    #[test]
    fn an_impossible_date_is_clamped_rather_than_refused() {
        // A metadata field is never a reason to fail to produce an invoice.
        let stamp = PdfDate::new(-5, 13, 40, 99, 99, 99).as_pdf();
        assert_eq!(stamp, "D:00001231235960Z00'00'");
    }
}
