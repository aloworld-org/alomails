//! Threading primitives (RFC 8621 §3 / RFC 5322 References).
//!
//! We thread solely on the `In-Reply-To`/`References` message-ids: a
//! message joins the thread of an **earlier** message it references, and
//! absent a reference match it starts a new thread. We deliberately do
//! **not** merge on base subject alone (that bleeds unrelated
//! conversations together), so `base_subject` produces only a thread
//! display label, never a join key. Two accepted, documented
//! limitations (`docs/interop.md`): threading is **forward-only** (a
//! reply delivered before its parent is not retro-merged), and
//! same-subject messages without references stay separate.

/// Normalizes a subject to its thread base: strips leading `Re:`/`Fwd:`/
/// `Fw:` prefixes (case-insensitive, repeated) and surrounding
/// whitespace, then lowercases for comparison.
pub fn base_subject(subject: &str) -> String {
    let mut s = subject.trim();
    loop {
        let trimmed = strip_reply_prefix(s);
        if trimmed.len() == s.len() {
            break;
        }
        s = trimmed.trim_start();
    }
    s.trim().to_lowercase()
}

/// Strips one leading reply/forward prefix (case-insensitive), returning
/// the remainder — or the input unchanged if none matched.
fn strip_reply_prefix(s: &str) -> &str {
    for prefix in ["re:", "fwd:", "fw:"] {
        // Compare bytes, not a `&str` slice: a subject whose first char is
        // multi-byte (`"a€"`) would make `s[..prefix.len()]` panic on a
        // non-char-boundary. A byte match means the leading `prefix.len()`
        // bytes are ASCII, so the subsequent `&s[prefix.len()..]` slice
        // lands on a boundary and is safe.
        if let Some(head) = s.as_bytes().get(..prefix.len())
            && head.eq_ignore_ascii_case(prefix.as_bytes())
        {
            return &s[prefix.len()..];
        }
    }
    s
}

/// Extracts `<message-id>` tokens (angle brackets included) from a
/// header value such as `References` or `In-Reply-To`. Order preserved,
/// duplicates removed.
pub fn extract_message_ids(value: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<'
            && let Some(end) = value[i..].find('>')
        {
            // Skip an empty `<>`: a malformed id that would otherwise
            // thread unrelated messages together on the `ANY(...)` match.
            if end >= 2 {
                let id = value[i..=i + end].to_owned();
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            i += end + 1;
            continue;
        }
        i += 1;
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_subject_strips_reply_and_forward_prefixes() {
        assert_eq!(base_subject("Re: Hello"), "hello");
        assert_eq!(
            base_subject("RE: FWD: Re:  Quarterly plan"),
            "quarterly plan"
        );
        assert_eq!(base_subject("Fw: [list] note"), "[list] note");
        assert_eq!(base_subject("  plain  "), "plain");
        // A subject that merely contains "re:" mid-string is untouched.
        assert_eq!(base_subject("aware: things"), "aware: things");
    }

    #[test]
    fn base_subject_never_panics_on_multibyte_subjects() {
        // Regression: a multi-byte char at the prefix-length boundary used
        // to panic (`s[..3]` inside a 3-byte char). These must not panic.
        assert_eq!(base_subject("a€"), "a€"); // byte 3 is inside '€'
        assert_eq!(base_subject("ab😀"), "ab😀"); // 4-byte char at offset 2
        assert_eq!(base_subject("€"), "€");
        assert_eq!(base_subject("Re: €"), "€"); // ASCII prefix, then multibyte
        assert_eq!(base_subject(""), "");
    }

    #[test]
    fn extract_message_ids_pulls_bracketed_tokens() {
        assert_eq!(
            extract_message_ids("<a@x.test> <b@y.test>"),
            vec!["<a@x.test>".to_owned(), "<b@y.test>".to_owned()]
        );
        assert_eq!(
            extract_message_ids("<a@x.test>\r\n <a@x.test>"),
            vec!["<a@x.test>".to_owned()],
            "duplicates removed"
        );
        assert!(extract_message_ids("no brackets here").is_empty());
        assert!(extract_message_ids("<unterminated").is_empty());
        // An empty `<>` is skipped so it cannot merge unrelated messages.
        assert!(extract_message_ids("<>").is_empty());
        assert_eq!(
            extract_message_ids("<> <real@x>"),
            vec!["<real@x>".to_owned()]
        );
    }
}
