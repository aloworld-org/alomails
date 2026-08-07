//! A small, defensive XML tree (alo Billing, ADR 0035, wave B1.24) — the
//! reader every inbound e-invoice is walked through.
//!
//! This module exists because reading somebody else's XML is a different job
//! from writing our own. Our writers ([`products/mail/alo-jmap/src/billing_cii.rs`]
//! and its UBL sibling) emit a document we control; an inbound file arrives
//! from a system we have never seen, produced by a tool we cannot ask, and may
//! not be a document at all. So the reader's contract is: **either a bounded
//! tree of elements, or a typed refusal — never a panic, never unbounded work,
//! never a fetch.**
//!
//! Three properties are load-bearing, and each is a real attack or a real
//! accident:
//!
//! - **No entity expansion, no DTD, no external anything.** A `<!DOCTYPE>` is
//!   refused outright rather than processed, so the billion-laughs expansion
//!   and the "fetch this URL as an entity" family cannot start. Nothing in this
//!   module opens a socket or a file.
//! - **Bounded depth and element count.** A pathological document costs a fixed
//!   amount of memory and time, so one upload cannot hold a worker.
//! - **Local names only.** Prefixes (`ram:`, `cbc:`, `n1:`) are stripped and
//!   ignored: two systems writing the same standard routinely choose different
//!   prefixes for the same namespace, and matching on the prefix would refuse a
//!   perfectly valid file for a cosmetic reason. What that costs is the ability
//!   to tell two same-named elements from different namespaces apart — which
//!   neither CII nor UBL does within one document, since the paths are
//!   distinct.
//!
//! It is a tree rather than a streaming walk because an e-invoice is read
//! out of order (the totals validate against the lines, the lines against the
//! breakdown) and a document is small — the size cap keeps it that way.

use quick_xml::Reader;
use quick_xml::events::Event;

use crate::error::{Result, StoreError};

/// The deepest nesting an e-invoice is read at. CII, the deeper of the two
/// syntaxes, reaches about eight; the rest is headroom for a wrapper somebody
/// puts around it.
const MAX_DEPTH: usize = 64;

/// The most elements one document may contain. A 500-line invoice in CII is
/// roughly ten thousand elements, so this admits any document our own line cap
/// allows and refuses a file that is trying to be a denial of service.
const MAX_ELEMENTS: usize = 200_000;

/// The most characters of text one element may carry. Long enough for a note
/// or an address, short enough that a megabyte of padding is refused rather
/// than stored.
const MAX_TEXT_CHARS: usize = 100_000;

/// One element of the parsed document: its local name, its attributes, the
/// text directly inside it, and its children in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    /// The local name, with any namespace prefix stripped (`ram:ID` → `ID`).
    pub name: String,
    /// Attributes as `(local name, value)`, in document order.
    pub attrs: Vec<(String, String)>,
    /// The text directly inside this element, unescaped and trimmed. Text
    /// interleaved between children is concatenated, which is right for a data
    /// document and wrong for prose — an e-invoice is the former.
    pub text: String,
    /// The children, in document order.
    pub children: Vec<Element>,
}

impl Element {
    /// The first child named `name`, or `None`.
    #[must_use]
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|child| child.name == name)
    }

    /// Every child named `name`, in document order.
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> {
        self.children.iter().filter(move |child| child.name == name)
    }

    /// The element at `path` below this one, following the **first** child of
    /// each name, or `None` when any step is missing.
    #[must_use]
    pub fn at(&self, path: &[&str]) -> Option<&Element> {
        let mut here = self;
        for step in path {
            here = here.child(step)?;
        }
        Some(here)
    }

    /// The text at `path`, or `""` when the path is not there.
    ///
    /// Absent and empty are deliberately the same answer: an optional business
    /// term that is missing and one that is present but blank mean the same
    /// thing to a reader, and collapsing them keeps every call site from
    /// writing the same `unwrap_or_default()`.
    #[must_use]
    pub fn text_at(&self, path: &[&str]) -> &str {
        self.at(path).map_or("", |element| element.text.as_str())
    }

    /// The value of attribute `name` on this element, or `""`.
    #[must_use]
    pub fn attr(&self, name: &str) -> &str {
        self.attrs
            .iter()
            .find(|(key, _)| key == name)
            .map_or("", |(_, value)| value.as_str())
    }
}

/// Parses `xml` into a bounded tree.
///
/// # Errors
/// [`StoreError::Validation`] when the bytes are not XML, carry a DTD, exceed
/// the depth/element/text bounds, or end before the document is closed. The
/// message names **what** was wrong with the file and never quotes the file:
/// an inbound invoice is somebody's commercial data, and error text is not a
/// place we put it (CLAUDE.md law 1).
pub fn parse(xml: &str) -> Result<Element> {
    let mut reader = Reader::from_str(xml);
    let config = reader.config_mut();
    // Whitespace between elements is layout, not data.
    config.trim_text(true);
    // An element that is never closed is a broken file, not something to
    // guess at.
    config.check_end_names = true;

    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    let mut elements = 0usize;

    loop {
        match reader.read_event() {
            Ok(Event::Start(start)) => {
                elements += 1;
                if elements > MAX_ELEMENTS {
                    return Err(too_big("elements"));
                }
                if stack.len() >= MAX_DEPTH {
                    return Err(too_big("nesting"));
                }
                stack.push(element_of(&start)?);
            }
            Ok(Event::Empty(start)) => {
                elements += 1;
                if elements > MAX_ELEMENTS {
                    return Err(too_big("elements"));
                }
                let element = element_of(&start)?;
                close(&mut stack, &mut root, element)?;
            }
            Ok(Event::End(_)) => {
                let Some(element) = stack.pop() else {
                    return Err(not_xml());
                };
                close(&mut stack, &mut root, element)?;
            }
            Ok(Event::Text(text)) => {
                let Some(open) = stack.last_mut() else {
                    // Text outside any element: a fragment, not a document.
                    continue;
                };
                let decoded = text.unescape().map_err(|_| not_xml())?;
                let decoded = decoded.trim();
                if decoded.is_empty() {
                    continue;
                }
                if open.text.chars().count() + decoded.chars().count() > MAX_TEXT_CHARS {
                    return Err(too_big("text"));
                }
                if !open.text.is_empty() {
                    open.text.push(' ');
                }
                open.text.push_str(decoded);
            }
            // A DTD is refused rather than processed: entity expansion is the
            // classic way an "invoice" becomes a memory exhaustion.
            Ok(Event::DocType(_)) => {
                return Err(StoreError::Validation(
                    "this file carries a document type declaration, which an e-invoice must not; \
                     it is refused unread"
                        .to_owned(),
                ));
            }
            Ok(Event::Eof) => break,
            // Comments, processing instructions and CDATA carry nothing an
            // e-invoice states.
            Ok(_) => {}
            Err(_) => return Err(not_xml()),
        }
    }

    if !stack.is_empty() {
        return Err(not_xml());
    }
    root.ok_or_else(not_xml)
}

/// Attaches a finished element to its parent, or records it as the root.
///
/// A second root is refused: one file is one document, and a reader that
/// silently kept the first would import half of whatever it was handed.
fn close(stack: &mut [Element], root: &mut Option<Element>, element: Element) -> Result<()> {
    match stack.last_mut() {
        Some(parent) => parent.children.push(element),
        None if root.is_none() => *root = Some(element),
        None => return Err(not_xml()),
    }
    Ok(())
}

/// The element a start tag opens: its local name and its attributes, both with
/// namespace prefixes stripped.
fn element_of(start: &quick_xml::events::BytesStart<'_>) -> Result<Element> {
    let name = local_name(start.name().as_ref())?;
    let mut attrs = Vec::new();
    for attr in start.attributes() {
        let attr = attr.map_err(|_| not_xml())?;
        // `xmlns` declarations bind prefixes we deliberately ignore.
        let key = attr.key.as_ref();
        if key == b"xmlns" || key.starts_with(b"xmlns:") {
            continue;
        }
        let key = local_name(key)?;
        let value = attr.unescape_value().map_err(|_| not_xml())?.into_owned();
        attrs.push((key, value));
    }
    Ok(Element {
        name,
        attrs,
        text: String::new(),
        children: Vec::new(),
    })
}

/// The part of a qualified name after the prefix, as UTF-8.
fn local_name(raw: &[u8]) -> Result<String> {
    let after_colon = match raw.iter().rposition(|byte| *byte == b':') {
        Some(at) => &raw[at + 1..],
        None => raw,
    };
    std::str::from_utf8(after_colon)
        .map(str::to_owned)
        .map_err(|_| not_xml())
}

/// The one refusal for "this is not a well-formed XML document", worded so it
/// helps without quoting the file.
fn not_xml() -> StoreError {
    StoreError::Validation(
        "this file is not a well-formed XML document; an e-invoice is the XML file itself \
         (Factur-X or XRechnung)"
            .to_owned(),
    )
}

/// The refusal for a document that is within its rights to be this large but
/// is not something we will read.
fn too_big(what: &str) -> StoreError {
    StoreError::Validation(format!(
        "this file has more {what} than an e-invoice can have; it is refused unread"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(xml: &str) -> Element {
        parse(xml).unwrap_or_else(|e| panic!("rejected: {e}"))
    }

    fn refusal(xml: &str) -> String {
        match parse(xml) {
            Err(StoreError::Validation(message)) => message,
            other => panic!("expected a Validation refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_document_becomes_a_tree_of_local_names() {
        let root = parsed(
            r#"<rsm:Invoice xmlns:rsm="urn:x" xmlns:ram="urn:y">
                 <ram:ID>INV-1</ram:ID>
                 <ram:Line><ram:Qty unitCode="HUR">15</ram:Qty></ram:Line>
                 <ram:Line><ram:Qty unitCode="KMT">240</ram:Qty></ram:Line>
               </rsm:Invoice>"#,
        );
        assert_eq!(root.name, "Invoice", "the prefix is not part of the name");
        assert_eq!(root.text_at(&["ID"]), "INV-1");
        assert_eq!(root.children_named("Line").count(), 2);
        assert_eq!(
            root.at(&["Line", "Qty"]).map(|q| q.attr("unitCode")),
            Some("HUR")
        );
        // A path that is not there is "" rather than an error: an optional
        // term that is absent and one that is blank mean the same thing.
        assert_eq!(root.text_at(&["Nope", "Deeper"]), "");
        assert_eq!(root.at(&["Nope"]), None);
        assert_eq!(root.attr("nope"), "");
    }

    #[test]
    fn the_same_document_parses_whatever_prefixes_it_chose() {
        // The identical document from two systems: same tree, different
        // prefixes. Matching on the prefix would refuse the second.
        let ours =
            parsed(r#"<rsm:Doc xmlns:rsm="urn:x"><ram:ID xmlns:ram="urn:y">7</ram:ID></rsm:Doc>"#);
        let theirs =
            parsed(r#"<n1:Doc xmlns:n1="urn:x"><n2:ID xmlns:n2="urn:y">7</n2:ID></n1:Doc>"#);
        assert_eq!(ours, theirs);
        // …and one with no prefixes at all.
        let plain = parsed(r#"<Doc><ID>7</ID></Doc>"#);
        assert_eq!(plain.text_at(&["ID"]), "7");
    }

    #[test]
    fn entities_are_unescaped_but_a_dtd_is_refused_unread() {
        assert_eq!(
            parsed("<Doc><Name>Kunde &amp; S&#246;hne</Name></Doc>").text_at(&["Name"]),
            "Kunde & Söhne"
        );
        // The billion-laughs shape. It is refused for carrying a DTD at all,
        // before any expansion could be attempted.
        let bomb = r#"<?xml version="1.0"?>
            <!DOCTYPE lolz [<!ENTITY lol "lol"><!ENTITY lol2 "&lol;&lol;&lol;&lol;">]>
            <Doc>&lol2;</Doc>"#;
        let message = refusal(bomb);
        assert!(message.contains("document type declaration"), "{message}");
        // An undeclared entity is a broken document, never a silent empty
        // value.
        assert!(parse("<Doc>&whatever;</Doc>").is_err());
    }

    #[test]
    fn a_file_that_is_not_a_document_is_refused_by_shape() {
        for bad in [
            "",
            "   ",
            "not xml at all",
            "{\"invoice\": 1}",
            "<Doc><Open></Doc>",
            "<Doc></Doc><Second></Second>",
            "<Doc",
        ] {
            let message = refusal(bad);
            assert!(
                message.contains("XML") || message.contains("refused"),
                "{bad:?} → {message}"
            );
        }
    }

    #[test]
    fn depth_and_element_count_are_bounded() {
        let deep = format!(
            "{}{}",
            "<a>".repeat(MAX_DEPTH + 1),
            "</a>".repeat(MAX_DEPTH + 1)
        );
        assert!(refusal(&deep).contains("nesting"));
        // Just inside the bound still parses, so the cap refuses pathology
        // rather than ordinary documents.
        let ok = format!("{}{}", "<a>".repeat(MAX_DEPTH), "</a>".repeat(MAX_DEPTH));
        assert!(parse(&ok).is_ok());
    }

    #[test]
    fn an_empty_element_is_an_element_and_keeps_its_place() {
        let root = parsed("<Doc><A/><B>1</B><A/></Doc>");
        assert_eq!(root.children.len(), 3);
        assert_eq!(root.children_named("A").count(), 2);
        assert_eq!(root.text_at(&["A"]), "");
        assert_eq!(root.text_at(&["B"]), "1");
    }

    #[test]
    fn comments_and_processing_instructions_carry_nothing() {
        let root = parsed(
            r#"<?xml version="1.0"?><!-- a comment --><Doc><!-- another -->
               <ID>INV-1</ID></Doc>"#,
        );
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.text_at(&["ID"]), "INV-1");
    }
}
