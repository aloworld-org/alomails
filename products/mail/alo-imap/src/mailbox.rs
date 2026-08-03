//! Mailbox name ↔ store-id mapping and LIST semantics. IMAP addresses
//! mailboxes by hierarchical **name** (separator `/`); the store keys them
//! by opaque id with a `parent_id` tree and a JMAP `role`. `INBOX` is the
//! reserved, case-insensitive name for the `role='inbox'` mailbox
//! (RFC 9051 §5.1). A stored mailbox name containing the separator would
//! be ambiguous over IMAP — documented as a shim limit; JMAP-created names
//! normally do not.

use alo_store::ImapMailbox;

/// The IMAP hierarchy separator.
pub const SEP: char = '/';

/// The reserved inbox name.
pub const INBOX: &str = "INBOX";

/// The full IMAP path of `mailbox` within `all`, rendering the inbox-role
/// mailbox (and its subtree root) as `INBOX`.
pub fn path_of(all: &[ImapMailbox], mailbox: &ImapMailbox) -> String {
    let mut segments = Vec::new();
    let mut current = Some(mailbox);
    while let Some(m) = current {
        if m.role.as_deref() == Some("inbox") {
            segments.push(INBOX.to_owned());
            break;
        }
        segments.push(m.name.clone());
        current = m
            .parent_id
            .as_ref()
            .and_then(|pid| all.iter().find(|x| &x.id == pid));
    }
    segments.reverse();
    segments.join(&SEP.to_string())
}

/// Resolves an IMAP path to the mailbox, honoring the `INBOX` alias
/// (case-insensitive on the first segment).
pub fn resolve<'a>(all: &'a [ImapMailbox], path: &str) -> Option<&'a ImapMailbox> {
    let segments: Vec<&str> = path.split(SEP).collect();
    if segments.is_empty() {
        return None;
    }
    // Locate the root segment.
    let first = segments[0];
    let mut current: Option<&ImapMailbox> = if first.eq_ignore_ascii_case(INBOX) {
        all.iter().find(|m| m.role.as_deref() == Some("inbox"))
    } else {
        all.iter().find(|m| {
            m.parent_id.is_none() && m.role.as_deref() != Some("inbox") && m.name == first
        })
    };
    for seg in &segments[1..] {
        let parent = current?;
        current = all
            .iter()
            .find(|m| m.parent_id.as_ref() == Some(&parent.id) && m.name == *seg);
    }
    current
}

/// Whether `mailbox` has any child in `all`.
pub fn has_children(all: &[ImapMailbox], mailbox: &ImapMailbox) -> bool {
    all.iter()
        .any(|m| m.parent_id.as_ref() == Some(&mailbox.id))
}

/// The RFC 6154 special-use attribute for a JMAP role, if any (e.g.
/// `sent` → `\Sent`). Returns an empty string when there is none.
pub fn special_use(role: Option<&str>) -> &'static str {
    match role {
        Some("sent") => "\\Sent",
        Some("drafts") => "\\Drafts",
        Some("trash") => "\\Trash",
        Some("junk") => "\\Junk",
        Some("archive") => "\\Archive",
        Some("all") => "\\All",
        Some("flagged") => "\\Flagged",
        _ => "",
    }
}

/// Matches an IMAP LIST pattern against a candidate mailbox path.
/// `%` matches zero or more chars except the hierarchy separator; `*`
/// matches zero or more chars including it (RFC 9051 §6.3.9).
pub fn list_match(pattern: &str, path: &str) -> bool {
    matches(pattern.as_bytes(), path.as_bytes())
}

fn matches(pat: &[u8], text: &[u8]) -> bool {
    let sep = SEP as u8;
    match pat.first() {
        None => text.is_empty(),
        Some(b'*') => {
            // Zero or more of anything.
            matches(&pat[1..], text) || (!text.is_empty() && matches(pat, &text[1..]))
        }
        Some(b'%') => {
            // Zero or more non-separator chars.
            matches(&pat[1..], text)
                || (!text.is_empty() && text[0] != sep && matches(pat, &text[1..]))
        }
        Some(&c) => !text.is_empty() && text[0] == c && matches(&pat[1..], &text[1..]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards_respect_hierarchy() {
        assert!(list_match("*", "INBOX"));
        assert!(list_match("*", "Work/Projects"));
        assert!(list_match("%", "INBOX"));
        // % does not cross the separator.
        assert!(!list_match("%", "Work/Projects"));
        assert!(list_match("Work/%", "Work/Projects"));
        assert!(!list_match("Work/%", "Work/Projects/Q3"));
        assert!(list_match("Work/*", "Work/Projects/Q3"));
        assert!(list_match("INBOX", "INBOX"));
        assert!(!list_match("IN", "INBOX"));
    }
}
