//! vCard 4.0 (RFC 6350) serialization and parsing for [`Contact`] — the
//! interchange format contacts trade in (CardDAV device sync, import and
//! export). We map the fields our address book models; the exotic vCard
//! properties (GEO, KEY, embedded PHOTO binaries, structured ADR, …) are
//! out of scope and are ignored on read and never emitted — a deviation
//! recorded in `docs/interop.md`.
//!
//! Emitted profile: `VERSION:4.0`, `UID`, `FN` (required), `N`, `EMAIL`
//! and `TEL` (with `TYPE=` labels), `ORG`, `TITLE`, `NOTE`. Lines are
//! CRLF-terminated and property values are escaped per §3.4. Parsing is
//! lenient (RFC 6350 §3.2 line unfolding, unknown properties skipped,
//! bad input never panics — vCards arrive from foreign clients).

use crate::id::ContactId;
use crate::model::{Contact, ContactField};

/// Splits a `.vcf` document (which may hold many cards) into its
/// individual `BEGIN:VCARD`…`END:VCARD` blocks and parses each — the
/// import path (Gmail/Outlook/Apple export one file with every
/// contact). Malformed or nameless cards are skipped, never fatal.
/// The number of blocks is bounded so a hostile upload cannot fan out
/// unboundedly.
pub fn from_vcards(input: &str) -> Vec<Contact> {
    const MAX_CARDS: usize = 50_000;
    let mut out = Vec::new();
    let mut current = String::new();
    let mut inside = false;
    for line in input.lines() {
        let trimmed = line.trim_start();
        if trimmed.eq_ignore_ascii_case("BEGIN:VCARD") {
            inside = true;
            current.clear();
        }
        if inside {
            current.push_str(line);
            current.push('\n');
        }
        if trimmed.eq_ignore_ascii_case("END:VCARD") {
            inside = false;
            if let Some(contact) = from_vcard(&current) {
                out.push(contact);
                if out.len() >= MAX_CARDS {
                    break;
                }
            }
        }
    }
    out
}

/// Serializes many contacts into one `.vcf` document (each a full
/// card) — the export path.
pub fn to_vcards(contacts: &[Contact]) -> String {
    let mut out = String::new();
    for contact in contacts {
        out.push_str(&to_vcard(contact));
    }
    out
}

/// Serializes a contact as a vCard 4.0 document (CRLF line endings).
pub fn to_vcard(contact: &Contact) -> String {
    let mut out = String::new();
    out.push_str("BEGIN:VCARD\r\nVERSION:4.0\r\n");
    line(
        &mut out,
        "UID",
        &format!("urn:uuid:{}", contact.id.as_str()),
    );
    // FN is mandatory in 4.0 (§6.2.1); the model guarantees it non-empty.
    line(&mut out, "FN", &contact.display_name);
    if contact.first_name.is_some() || contact.last_name.is_some() {
        // N = Family;Given;Additional;Prefix;Suffix (§6.2.2).
        let n = format!(
            "{};{};;;",
            escape(contact.last_name.as_deref().unwrap_or("")),
            escape(contact.first_name.as_deref().unwrap_or("")),
        );
        // N's structure uses raw `;`, so write it pre-escaped, not via `line`.
        out.push_str("N:");
        out.push_str(&n);
        out.push_str("\r\n");
    }
    for email in &contact.emails {
        typed_line(&mut out, "EMAIL", email);
    }
    for phone in &contact.phones {
        typed_line(&mut out, "TEL", phone);
    }
    if let Some(org) = &contact.organization {
        line(&mut out, "ORG", org);
    }
    if let Some(title) = &contact.job_title {
        line(&mut out, "TITLE", title);
    }
    if let Some(note) = &contact.notes {
        line(&mut out, "NOTE", note);
    }
    out.push_str("END:VCARD\r\n");
    out
}

/// Parses the first `VCARD` in `input` into a [`Contact`]. The `UID`
/// (when a bare or `urn:uuid:`/`uuid:` form) becomes the contact id;
/// otherwise a fresh id is generated. Returns `None` only when no `FN`
/// (or usable fallback) is present — a nameless card is not a contact.
pub fn from_vcard(input: &str) -> Option<Contact> {
    let mut id: Option<ContactId> = None;
    let mut display_name: Option<String> = None;
    let mut first_name = None;
    let mut last_name = None;
    let mut emails = Vec::new();
    let mut phones = Vec::new();
    let mut organization = None;
    let mut job_title = None;
    let mut notes = None;

    for logical in unfold(input) {
        let Some((name, params, value)) = parse_line(&logical) else {
            continue;
        };
        match name.to_ascii_uppercase().as_str() {
            "UID" => id = Some(parse_uid(&value)),
            "FN" => display_name = Some(unescape(&value)),
            "N" => {
                // Family;Given;… — only the first two components used.
                let parts: Vec<&str> = split_structured(&value);
                last_name = parts.first().map(|s| unescape(s)).filter(|s| !s.is_empty());
                first_name = parts.get(1).map(|s| unescape(s)).filter(|s| !s.is_empty());
            }
            "EMAIL" => emails.push(ContactField {
                kind: type_param(&params),
                value: unescape(&value),
            }),
            "TEL" => phones.push(ContactField {
                kind: type_param(&params),
                value: unescape(&value),
            }),
            "ORG" => {
                // ORG is structured (Org;Unit;…); the first component is the org.
                let first = split_structured(&value).into_iter().next().unwrap_or("");
                organization = Some(unescape(first)).filter(|s| !s.is_empty());
            }
            "TITLE" => job_title = Some(unescape(&value)).filter(|s| !s.is_empty()),
            "NOTE" => notes = Some(unescape(&value)).filter(|s| !s.is_empty()),
            _ => {}
        }
    }

    // Fall back to a name built from N, or the first email, so a card
    // without FN (some exporters omit it) still yields a usable contact.
    let display_name = display_name
        .filter(|s| !s.trim().is_empty())
        .or_else(|| match (&first_name, &last_name) {
            (Some(f), Some(l)) => Some(format!("{f} {l}")),
            (Some(f), None) => Some(f.clone()),
            (None, Some(l)) => Some(l.clone()),
            (None, None) => None,
        })
        .or_else(|| emails.first().map(|e| e.value.clone()))?;

    Some(Contact {
        id: id.unwrap_or_else(ContactId::generate),
        display_name,
        first_name,
        last_name,
        emails,
        phones,
        organization,
        job_title,
        notes,
    })
}

/// Emits `NAME:escaped-value` + CRLF, folding long lines at 75 octets
/// (§3.2). `value` is a single free-text component.
fn line(out: &mut String, name: &str, value: &str) {
    fold(out, name, &escape(value), None);
}

/// Emits a typed property (`EMAIL`/`TEL`) with an optional `TYPE=` param.
fn typed_line(out: &mut String, name: &str, field: &ContactField) {
    let params = field
        .kind
        .as_deref()
        .filter(|k| is_safe_param(k))
        .map(|k| format!("TYPE={k}"));
    fold(out, name, &escape(&field.value), params.as_deref());
}

/// Writes `NAME[;params]:value`, folding at 75 octets per §3.2 (a space
/// begins each continuation line).
fn fold(out: &mut String, name: &str, value: &str, params: Option<&str>) {
    let mut header = String::from(name);
    if let Some(params) = params {
        header.push(';');
        header.push_str(params);
    }
    let full = format!("{header}:{value}");
    let bytes = full.as_bytes();
    if bytes.len() <= 75 {
        out.push_str(&full);
        out.push_str("\r\n");
        return;
    }
    // Fold on octet boundaries that are also char boundaries.
    let mut start = 0;
    let mut first = true;
    while start < full.len() {
        let budget = if first { 75 } else { 74 };
        let mut end = (start + budget).min(full.len());
        while end < full.len() && !full.is_char_boundary(end) {
            end -= 1;
        }
        if !first {
            out.push(' ');
        }
        out.push_str(&full[start..end]);
        out.push_str("\r\n");
        start = end;
        first = false;
    }
}

/// Unfolds (§3.2) a raw vCard body into logical lines: a CRLF (or LF)
/// followed by a space or tab is a continuation of the prior line.
fn unfold(input: &str) -> Vec<String> {
    let mut logical: Vec<String> = Vec::new();
    for raw in input.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(rest) = raw.strip_prefix([' ', '\t'])
            && let Some(last) = logical.last_mut()
        {
            last.push_str(rest);
            continue;
        }
        logical.push(raw.to_owned());
    }
    logical
}

/// Splits one logical line into `(name, params, value)`. Params are the
/// raw `;`-separated tokens between the name and the first unquoted `:`.
fn parse_line(line: &str) -> Option<(String, Vec<String>, String)> {
    let colon = find_value_colon(line)?;
    let (head, value) = line.split_at(colon);
    let value = &value[1..]; // drop the ':'
    let mut parts = head.split(';');
    let name = parts.next()?.trim().to_owned();
    if name.is_empty() {
        return None;
    }
    let params = parts.map(|p| p.trim().to_owned()).collect();
    Some((name, params, value.to_owned()))
}

/// The index of the `:` that separates the property name/params from the
/// value, honoring quoted param values (which may contain `:`).
fn find_value_colon(line: &str) -> Option<usize> {
    let mut in_quotes = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            ':' if !in_quotes => return Some(i),
            _ => {}
        }
    }
    None
}

/// The first `TYPE=` parameter value, lowercased (e.g. `work`), if any.
fn type_param(params: &[String]) -> Option<String> {
    params.iter().find_map(|p| {
        let (k, v) = p.split_once('=')?;
        if k.eq_ignore_ascii_case("type") {
            let v = v.trim_matches('"').split(',').next().unwrap_or("").trim();
            (!v.is_empty()).then(|| v.to_ascii_lowercase())
        } else {
            None
        }
    })
}

/// Reads a `UID`: strips a `urn:uuid:` / `uuid:` scheme if present.
fn parse_uid(value: &str) -> ContactId {
    let bare = value
        .strip_prefix("urn:uuid:")
        .or_else(|| value.strip_prefix("uuid:"))
        .unwrap_or(value)
        .trim();
    ContactId::new(bare.to_owned())
}

/// Splits a structured value on unescaped `;` (for `N`, `ORG`).
fn split_structured(value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2; // skip the escaped char
            continue;
        }
        if bytes[i] == b';' {
            parts.push(&value[start..i]);
            start = i + 1;
        }
        i += 1;
    }
    parts.push(&value[start..]);
    parts
}

/// Escapes a value for a vCard text component (§3.4): `\`, `,`, `;`,
/// newline. (`:` is not escaped in values.)
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            ',' => out.push_str("\\,"),
            ';' => out.push_str("\\;"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            _ => out.push(c),
        }
    }
    out
}

/// Reverses [`escape`] (§3.4). Unknown escapes keep their literal char.
fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n' | 'N') => out.push('\n'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out.trim().to_owned()
}

/// A TYPE param value we will emit unquoted: letters/digits/dash only,
/// so a hostile stored label can never inject a `;`/`:`/`"` into a line.
fn is_safe_param(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn sample() -> Contact {
        Contact {
            id: ContactId::new("abc123"),
            display_name: "Alice Martin".to_owned(),
            first_name: Some("Alice".to_owned()),
            last_name: Some("Martin".to_owned()),
            emails: vec![
                ContactField {
                    kind: Some("work".to_owned()),
                    value: "alice@example.eu".to_owned(),
                },
                ContactField {
                    kind: None,
                    value: "alice.m@perso.fr".to_owned(),
                },
            ],
            phones: vec![ContactField {
                kind: Some("mobile".to_owned()),
                value: "+33 6 12 34 56 78".to_owned(),
            }],
            organization: Some("Example SARL".to_owned()),
            job_title: Some("Directrice".to_owned()),
            notes: Some("Rencontrée au salon; suivi Q3.".to_owned()),
        }
    }

    #[test]
    fn round_trips_all_fields() {
        let original = sample();
        let vcard = to_vcard(&original);
        assert!(vcard.starts_with("BEGIN:VCARD\r\nVERSION:4.0\r\n"));
        assert!(vcard.contains("UID:urn:uuid:abc123\r\n"));
        assert!(vcard.contains("FN:Alice Martin\r\n"));
        assert!(vcard.contains("N:Martin;Alice;;;\r\n"));
        assert!(vcard.contains("EMAIL;TYPE=work:alice@example.eu\r\n"));
        assert!(vcard.contains("TEL;TYPE=mobile:+33 6 12 34 56 78\r\n"));
        let parsed = from_vcard(&vcard).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn note_semicolon_and_comma_survive_a_round_trip() {
        let original = sample();
        let vcard = to_vcard(&original);
        // The note's `;` is escaped in the wire form...
        assert!(vcard.contains("NOTE:Rencontrée au salon\\; suivi Q3.\r\n"));
        // ...and comes back intact.
        assert_eq!(from_vcard(&vcard).unwrap().notes, original.notes);
    }

    #[test]
    fn parses_a_foreign_card_and_unfolds() {
        // A card as another client might emit it: folded NOTE, quoted-ish
        // TYPE, unknown properties, LF-only line endings.
        let card = "BEGIN:VCARD\nVERSION:4.0\nFN:Bob Dupont\nN:Dupont;Bob;;;\n\
                    EMAIL;TYPE=HOME:bob@dupont.fr\nTEL;TYPE=work,voice:+33123\n\
                    ORG:Dupont & Fils;R&D\nX-CUSTOM:ignored\n\
                    NOTE:First line\n  continued here\nEND:VCARD\n";
        let c = from_vcard(card).unwrap();
        assert_eq!(c.display_name, "Bob Dupont");
        assert_eq!(c.first_name.as_deref(), Some("Bob"));
        assert_eq!(c.emails[0].value, "bob@dupont.fr");
        assert_eq!(c.emails[0].kind.as_deref(), Some("home"));
        assert_eq!(c.phones[0].kind.as_deref(), Some("work"));
        assert_eq!(c.organization.as_deref(), Some("Dupont & Fils"));
        assert_eq!(c.notes.as_deref(), Some("First line continued here"));
    }

    #[test]
    fn splits_and_parses_a_multi_card_file() {
        // Two cards plus a nameless one (skipped) and CRLF endings, as a
        // real exporter emits.
        let vcf = "BEGIN:VCARD\r\nVERSION:4.0\r\nFN:Ada Lovelace\r\nEMAIL:ada@eng.uk\r\nEND:VCARD\r\n\
                   BEGIN:VCARD\r\nVERSION:4.0\r\nEND:VCARD\r\n\
                   BEGIN:VCARD\r\nVERSION:4.0\r\nFN:Bob Dupont\r\nTEL:+33\r\nEND:VCARD\r\n";
        let contacts = from_vcards(vcf);
        assert_eq!(contacts.len(), 2, "the nameless card is skipped");
        assert_eq!(contacts[0].display_name, "Ada Lovelace");
        assert_eq!(contacts[1].display_name, "Bob Dupont");
    }

    #[test]
    fn export_then_import_round_trips_a_set() {
        let set = vec![sample(), {
            let mut c = sample();
            c.id = ContactId::new("def456");
            c.display_name = "Bob Second".to_owned();
            c.first_name = Some("Bob".to_owned());
            c.last_name = Some("Second".to_owned());
            c
        }];
        let vcf = to_vcards(&set);
        let back = from_vcards(&vcf);
        assert_eq!(back, set, "export→import is lossless for a whole set");
    }

    #[test]
    fn missing_fn_falls_back_and_nameless_is_none() {
        let card = "BEGIN:VCARD\nVERSION:4.0\nEMAIL:only@address.eu\nEND:VCARD\n";
        assert_eq!(from_vcard(card).unwrap().display_name, "only@address.eu");
        // No name and no email → not a contact.
        assert!(from_vcard("BEGIN:VCARD\nVERSION:4.0\nEND:VCARD\n").is_none());
    }

    #[test]
    fn hostile_type_label_cannot_inject_structure() {
        let mut c = sample();
        c.emails = vec![ContactField {
            kind: Some("work:INJECT;X-Evil=1".to_owned()),
            value: "x@y.eu".to_owned(),
        }];
        let vcard = to_vcard(&c);
        // The unsafe label is dropped, so no extra params are emitted.
        assert!(vcard.contains("EMAIL:x@y.eu\r\n"));
        assert!(!vcard.contains("X-Evil"));
    }

    #[test]
    fn long_value_folds_and_unfolds_cleanly() {
        let mut c = sample();
        c.notes = Some("z".repeat(300));
        let vcard = to_vcard(&c);
        // Folded: continuation lines begin with a space.
        assert!(vcard.contains("\r\n z"));
        assert_eq!(from_vcard(&vcard).unwrap().notes, c.notes);
    }
}
