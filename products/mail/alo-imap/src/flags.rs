//! IMAP flag ↔ store keyword mapping (RFC 9051 §2.3.2, RFC 8621 §4.1.1).
//! System flags map to the JMAP `$`-keywords; `\Deleted` has no JMAP
//! equivalent and is stored as the internal `$deleted` keyword (documented
//! in `docs/interop.md`). Custom keywords round-trip lowercased.

/// The store keyword backing IMAP `\Deleted` (no JMAP standard keyword).
pub const DELETED: &str = "$deleted";

/// The system flags we recognise, as `(imap, keyword)` pairs.
const SYSTEM: &[(&str, &str)] = &[
    ("\\Seen", "$seen"),
    ("\\Flagged", "$flagged"),
    ("\\Answered", "$answered"),
    ("\\Draft", "$draft"),
    ("\\Deleted", DELETED),
];

/// Maps an IMAP flag to the store keyword to set/clear. `\Recent` and
/// unknown `\`-flags are rejected (`None`) — a client cannot set them.
pub fn imap_to_keyword(flag: &str) -> Option<String> {
    for (imap, kw) in SYSTEM {
        if flag.eq_ignore_ascii_case(imap) {
            return Some((*kw).to_owned());
        }
    }
    if flag.starts_with('\\') {
        // An unrecognised system flag (e.g. \Recent) cannot be set.
        return None;
    }
    // A custom keyword: RFC 8621 keywords are lowercase.
    Some(flag.to_ascii_lowercase())
}

/// Maps a store keyword back to the IMAP flag to advertise.
pub fn keyword_to_imap(keyword: &str) -> String {
    for (imap, kw) in SYSTEM {
        if keyword == *kw {
            return (*imap).to_owned();
        }
    }
    keyword.to_owned()
}

/// Renders a message's keyword set as an IMAP flag list body (space-
/// separated, system flags first for stable output).
pub fn render_flags(keywords: &[String]) -> String {
    let mut out: Vec<String> = keywords.iter().map(|k| keyword_to_imap(k)).collect();
    out.sort();
    out.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_flags_round_trip() {
        assert_eq!(imap_to_keyword("\\Seen").as_deref(), Some("$seen"));
        assert_eq!(imap_to_keyword("\\deleted").as_deref(), Some("$deleted"));
        assert_eq!(keyword_to_imap("$flagged"), "\\Flagged");
        // \Recent can't be set.
        assert_eq!(imap_to_keyword("\\Recent"), None);
        // custom keyword lowercased.
        assert_eq!(imap_to_keyword("Forwarded").as_deref(), Some("forwarded"));
    }
}
