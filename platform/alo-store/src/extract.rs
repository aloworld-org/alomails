//! Text extraction from file bytes for Drive content search (ADR 0029).
//!
//! Given a node's kind, declared content-type, and bytes, [`extract_text`]
//! returns the plain text worth indexing, or `None` when there is nothing we
//! know how to read. It is a **pure, synchronous, best-effort** function: the
//! store calls it inside `spawn_blocking`, so a slow parse never blocks the
//! async runtime and a parser panic on a malformed file becomes a dropped join
//! (→ "not content-indexed"), never a crash.
//!
//! What we read today:
//! - **alo Doc** (`kind = "doc"`) — BlockNote JSON, its text runs;
//! - **`text/*`** files — the bytes as UTF-8;
//! - **Office** `.docx`/`.xlsx`/`.pptx` — ZIP-of-XML, the text of the relevant
//!   parts (via `zip` + `quick-xml`);
//! - **PDF** — the text layer (via `pdf-extract`).
//!
//! A file whose bytes we can't turn into text (an image, an unknown binary)
//! yields `None` and stays name-searchable only.

use std::io::{Cursor, Read};

/// Cap on the extracted text fed to the content index, so one enormous document
/// can't bloat the row or the index. Characters, not bytes.
pub const INDEX_TEXT_CAP: usize = 256 * 1024;

const CT_DOCX: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
const CT_XLSX: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
const CT_PPTX: &str = "application/vnd.openxmlformats-officedocument.presentationml.presentation";
const CT_PDF: &str = "application/pdf";

/// A cheap pre-check: is this node's kind/content-type one we might extract text
/// from? Lets the store skip fetching bytes for files that are clearly not text
/// (e.g. images). An unknown/generic type returns `true` so extraction can sniff
/// by magic bytes.
#[must_use]
pub fn is_extractable(kind: &str, content_type: Option<&str>) -> bool {
    if kind == "doc" {
        return true;
    }
    match content_type {
        Some(ct) => {
            ct.starts_with("text/")
                || ct == CT_DOCX
                || ct == CT_XLSX
                || ct == CT_PPTX
                || ct == CT_PDF
                || ct == "application/json"
                || ct == "application/octet-stream"
        }
        None => true,
    }
}

/// Extracts indexable text, or `None` if there is nothing useful. Output is
/// capped at [`INDEX_TEXT_CAP`] characters. Best-effort throughout: a parse
/// failure returns `None`, never an error.
#[must_use]
pub fn extract_text(kind: &str, content_type: Option<&str>, bytes: &[u8]) -> Option<String> {
    let raw = if kind == "doc" {
        blocknote_text(bytes)?
    } else {
        match content_type {
            Some(ct) if ct.starts_with("text/") => String::from_utf8_lossy(bytes).into_owned(),
            Some(CT_DOCX) => docx_text(bytes)?,
            Some(CT_XLSX) => xlsx_text(bytes)?,
            Some(CT_PPTX) => pptx_text(bytes)?,
            Some(CT_PDF) => pdf_text(bytes)?,
            // Unknown/generic type: sniff by magic bytes.
            _ => sniff_text(bytes)?,
        }
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(raw.chars().take(INDEX_TEXT_CAP).collect())
}

/// Sniffs a type-less binary by its magic bytes: a ZIP could be an Office file,
/// `%PDF` is a PDF. Anything else is not text we index.
fn sniff_text(bytes: &[u8]) -> Option<String> {
    if bytes.starts_with(b"%PDF") {
        return pdf_text(bytes);
    }
    if bytes.starts_with(b"PK\x03\x04") {
        // A ZIP — try each Office layout; the first that yields text wins.
        return docx_text(bytes)
            .or_else(|| xlsx_text(bytes))
            .or_else(|| pptx_text(bytes));
    }
    None
}

// ---- alo Doc (BlockNote JSON) ----------------------------------------------

/// Collects the text runs of a BlockNote document into one string.
fn blocknote_text(bytes: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let mut out = String::new();
    collect_json_text(&value, &mut out);
    Some(out)
}

/// Walks a JSON tree, appending every `"text"` string it finds (wherever it
/// appears), space-separated — robust to BlockNote's exact nesting.
fn collect_json_text(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(serde_json::Value::String(text)) = map.get("text") {
                out.push_str(text);
                out.push(' ');
            }
            for child in map.values() {
                collect_json_text(child, out);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_json_text(item, out);
            }
        }
        _ => {}
    }
}

// ---- Office (ZIP of XML) ----------------------------------------------------

/// Reads one named entry out of a ZIP archive as bytes.
fn zip_entry(bytes: &[u8], name: &str) -> Option<Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
    let mut file = archive.by_name(name).ok()?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// The names of every ZIP entry matching a `prefix`…`suffix` (e.g. all slides).
fn zip_entry_names(bytes: &[u8], prefix: &str, suffix: &str) -> Vec<String> {
    let Ok(archive) = zip::ZipArchive::new(Cursor::new(bytes)) else {
        return Vec::new();
    };
    archive
        .file_names()
        .filter(|n| n.starts_with(prefix) && n.ends_with(suffix))
        .map(str::to_owned)
        .collect()
}

/// A `.docx` — the body text lives in `word/document.xml`.
fn docx_text(bytes: &[u8]) -> Option<String> {
    let xml = zip_entry(bytes, "word/document.xml")?;
    let text = xml_all_text(&xml);
    (!text.trim().is_empty()).then_some(text)
}

/// A `.xlsx` — the string cell values live in `xl/sharedStrings.xml` (the bulk
/// of a sheet's searchable text; numeric cells are not indexed).
fn xlsx_text(bytes: &[u8]) -> Option<String> {
    let xml = zip_entry(bytes, "xl/sharedStrings.xml")?;
    let text = xml_all_text(&xml);
    (!text.trim().is_empty()).then_some(text)
}

/// A `.pptx` — text is spread across `ppt/slides/slideN.xml`; concatenate them.
fn pptx_text(bytes: &[u8]) -> Option<String> {
    let mut names = zip_entry_names(bytes, "ppt/slides/slide", ".xml");
    if names.is_empty() {
        return None;
    }
    names.sort();
    let mut out = String::new();
    for name in names {
        if let Some(xml) = zip_entry(bytes, &name) {
            out.push_str(&xml_all_text(&xml));
            out.push(' ');
        }
    }
    (!out.trim().is_empty()).then_some(out)
}

/// Concatenates every text node of an XML document, space-separated. For the
/// Office parts we read, the text nodes *are* the content, so this needs no
/// per-schema tag knowledge and tolerates version differences.
fn xml_all_text(xml: &[u8]) -> String {
    use quick_xml::Reader;
    use quick_xml::events::Event;

    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut out = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Text(e)) => {
                if let Ok(text) = e.unescape() {
                    let text = text.trim();
                    if !text.is_empty() {
                        out.push_str(text);
                        out.push(' ');
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

// ---- PDF --------------------------------------------------------------------

/// A PDF's text layer. `pdf-extract` can panic on some malformed files; the
/// store runs extraction in `spawn_blocking`, so a panic is contained. A PDF
/// that is pure scanned images yields nothing (no OCR) → `None`.
fn pdf_text(bytes: &[u8]) -> Option<String> {
    let text = pdf_extract::extract_text_from_mem(bytes).ok()?;
    (!text.trim().is_empty()).then_some(text)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn blocknote_doc_yields_its_text() {
        let json = br#"[{"type":"paragraph","content":[{"type":"text","text":"hello pangolin","styles":{}}]}]"#;
        let text = extract_text("doc", Some("application/json"), json).unwrap();
        assert!(text.contains("pangolin"));
    }

    #[test]
    fn plain_text_passes_through() {
        let text = extract_text("file", Some("text/plain"), b"the quokka report").unwrap();
        assert!(text.contains("quokka"));
    }

    #[test]
    fn image_is_not_extractable() {
        assert!(!is_extractable("file", Some("image/png")));
        // And an unknown binary that is neither ZIP nor PDF yields nothing.
        assert!(extract_text("file", Some("application/octet-stream"), b"\x89PNG\r\n").is_none());
    }

    #[test]
    fn empty_text_is_none() {
        assert!(extract_text("file", Some("text/plain"), b"   \n\t ").is_none());
    }

    /// Builds a minimal ZIP (an Office container) with the given entries.
    fn zip_of(entries: &[(&str, &str)]) -> Vec<u8> {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, body) in entries {
            w.start_file(*name, SimpleFileOptions::default()).unwrap();
            w.write_all(body.as_bytes()).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn docx_body_text_is_extracted() {
        let doc = zip_of(&[(
            "word/document.xml",
            r#"<?xml version="1.0"?><w:document xmlns:w="x"><w:body><w:p><w:r><w:t>the hippopotamus budget</w:t></w:r></w:p></w:body></w:document>"#,
        )]);
        let text = extract_text("file", Some(CT_DOCX), &doc).unwrap();
        assert!(text.contains("hippopotamus"), "got: {text}");
        // And a content-type-less upload still works by ZIP sniffing.
        assert!(
            extract_text("file", None, &doc)
                .unwrap()
                .contains("hippopotamus")
        );
    }

    #[test]
    fn xlsx_shared_strings_are_extracted() {
        let book = zip_of(&[(
            "xl/sharedStrings.xml",
            r#"<?xml version="1.0"?><sst xmlns="x"><si><t>Revenue</t></si><si><t>platypus region</t></si></sst>"#,
        )]);
        let text = extract_text("file", Some(CT_XLSX), &book).unwrap();
        assert!(text.contains("platypus"), "got: {text}");
    }

    #[test]
    fn pptx_slides_are_extracted() {
        let deck = zip_of(&[
            (
                "ppt/slides/slide1.xml",
                r#"<?xml version="1.0"?><p:sld xmlns:a="x"><a:t>quokka strategy</a:t></p:sld>"#,
            ),
            (
                "ppt/slides/slide2.xml",
                r#"<?xml version="1.0"?><p:sld xmlns:a="x"><a:t>launch in Q3</a:t></p:sld>"#,
            ),
        ]);
        let text = extract_text("file", Some(CT_PPTX), &deck).unwrap();
        assert!(
            text.contains("quokka") && text.contains("launch"),
            "got: {text}"
        );
    }

    #[test]
    fn corrupt_office_file_is_none_not_panic() {
        // Truncated/garbage ZIP bytes must not panic — just no index.
        assert!(extract_text("file", Some(CT_DOCX), b"PK\x03\x04 not a real zip").is_none());
    }
}
