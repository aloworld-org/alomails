//! Reading a Delivery Status Notification (RFC 3464) — the receive half of
//! [`dsn`](crate::dsn), which composes them. Split from it deliberately: what
//! we *send* changes with our bounce policy, what we *accept* changes with
//! every provider whose bounces come back to the campaign return path, and
//! one file with both has two reasons to change.
//!
//! Tolerant in what we accept (the protocol doctrine): a report is parsed
//! from whatever line endings, casings and foldings it arrives with, and a
//! message this module cannot read is answered `None` — never an error, never
//! a panic — because the caller's contract (queue item M4.4) is that a
//! non-DSN message to the return path is stored, not crashed on.
//!
//! The strictness lives in one place: [`classify`]. Suppression is
//! irreversible (ADR 0044 §2), so only the unambiguous permanent failure —
//! `Action: failed` with an RFC 3463 class-5 status (§2.3.3, §2.3.4) —
//! reports [`DsnVerdict::Hard`]. A report that is contradictory or
//! incomplete is soft, and softness costs nothing: the next send bounces
//! again.

/// One per-recipient group of a delivery-status report (RFC 3464 §2.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DsnRecipient {
    /// The address the report is about: `Original-Recipient` (§2.3.1) when
    /// present — the address as originally specified, before any forwarding
    /// rewrote it — else `Final-Recipient` (§2.3.2). The `address-type;`
    /// prefix (`rfc822;`, or `utf-8;` per RFC 6533) is already stripped.
    pub address: String,
    /// The `Action` field (§2.3.3), lowercased: `failed`, `delayed`,
    /// `delivered`, `relayed`, `expanded` — or whatever the report said.
    pub action: String,
    /// The `Status` field (§2.3.4): an RFC 3463 `class.subject.detail`,
    /// `None` when absent or not one.
    pub status: Option<String>,
}

/// What one reported recipient calls for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DsnVerdict {
    /// A settled permanent failure: `Action: failed` and a class-5 status.
    Hard,
    /// A transient condition (`delayed`, class 4, or a report too garbled to
    /// be sure) — recorded, never suppressed on.
    Soft,
    /// Nothing failed: delivered/relayed/expanded, or a class-2 status.
    Ignore,
}

/// Classifies one reported recipient. Strict where it must be: only the
/// unambiguous permanent failure is [`DsnVerdict::Hard`], because the caller
/// suppresses on it and suppression cannot be lifted. `failed` with a class-4
/// status, or with no parseable status at all, is a contradiction RFC 3464
/// does not define — resolved to [`DsnVerdict::Soft`], the verdict that can
/// be wrong for free.
pub fn classify(recipient: &DsnRecipient) -> DsnVerdict {
    let class = recipient
        .status
        .as_deref()
        .and_then(|s| s.split('.').next())
        .and_then(|c| c.parse::<u8>().ok());
    match (recipient.action.as_str(), class) {
        ("failed", Some(5)) => DsnVerdict::Hard,
        ("failed" | "delayed", _) => DsnVerdict::Soft,
        // delivered / relayed / expanded — or an action this build does not
        // know, which is not a failure it can act on.
        _ => DsnVerdict::Ignore,
    }
}

/// Parses a message as an RFC 3464 DSN.
///
/// `Some(recipients)` when the message is a `multipart/report` of type
/// `delivery-status` (RFC 6522 §3) with a readable `message/delivery-status`
/// part — the vector holds one entry per per-recipient group that named an
/// address. `None` when it is anything else: not a report, a report of a
/// different type, or bytes that do not parse as one. Never an error.
pub fn parse_dsn(message: &[u8]) -> Option<Vec<DsnRecipient>> {
    let text = String::from_utf8_lossy(message);
    let (headers, body) = split_message(&text)?;
    let content_type = header_value(headers, "content-type")?;
    let ct_lower = content_type.to_ascii_lowercase();
    if !ct_lower.contains("multipart/report") || !report_type_is_delivery_status(&ct_lower) {
        return None;
    }
    let boundary = mime_parameter(&content_type, "boundary")?;
    let status_body = multipart_parts(body, &boundary)
        .into_iter()
        .find_map(|part| {
            let (part_headers, part_body) = split_message(part)?;
            let part_type = header_value(part_headers, "content-type")?;
            part_type
                .to_ascii_lowercase()
                .trim_start()
                .starts_with("message/delivery-status")
                .then_some(part_body)
        })?;
    Some(parse_delivery_status(status_body))
}

/// Whether the content-type (already lowercased) declares
/// `report-type=delivery-status` — quoted or bare (RFC 2045 §5.1 allows
/// either for a parameter value).
fn report_type_is_delivery_status(ct_lower: &str) -> bool {
    ct_lower
        .split(';')
        .filter_map(|param| param.split_once('='))
        .any(|(name, value)| {
            name.trim() == "report-type" && value.trim().trim_matches('"') == "delivery-status"
        })
}

/// Splits a message (or MIME part) into its header block and body at the
/// first empty line, tolerating both CRLF and bare-LF messages.
fn split_message(text: &str) -> Option<(&str, &str)> {
    if let Some((h, b)) = text.split_once("\r\n\r\n") {
        return Some((h, b));
    }
    text.split_once("\n\n")
}

/// The unfolded value of the first header named `name` (case-insensitive) in
/// a header block. Folded continuation lines (RFC 5322 §2.2.3: leading WSP)
/// are joined with a single space.
fn header_value(headers: &str, name: &str) -> Option<String> {
    let mut value: Option<String> = None;
    for line in headers.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(v) = &mut value {
            if line.starts_with(' ') || line.starts_with('\t') {
                v.push(' ');
                v.push_str(line.trim());
                continue;
            }
            return value;
        }
        if let Some((n, v)) = line.split_once(':')
            && n.trim().eq_ignore_ascii_case(name)
        {
            value = Some(v.trim().to_owned());
        }
    }
    value
}

/// The value of a MIME parameter (e.g. `boundary`) in a content-type value,
/// quoted or bare. Case-insensitive on the name; the value keeps its case
/// (a boundary is matched byte-for-byte, RFC 2046 §5.1.1).
fn mime_parameter(content_type: &str, name: &str) -> Option<String> {
    content_type
        .split(';')
        .filter_map(|param| param.split_once('='))
        .find(|(n, _)| n.trim().eq_ignore_ascii_case(name))
        .map(|(_, v)| v.trim().trim_matches('"').to_owned())
        .filter(|v| !v.is_empty())
}

/// The bodies of a multipart's parts: everything between `--boundary`
/// delimiters, stopping at the `--boundary--` close (RFC 2046 §5.1.1).
/// Delimiter matching tolerates bare-LF messages and trailing transport
/// padding on the delimiter line.
fn multipart_parts<'a>(body: &'a str, boundary: &str) -> Vec<&'a str> {
    let delimiter = format!("--{boundary}");
    let mut parts = Vec::new();
    let mut current_start: Option<usize> = None;
    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_end();
        let is_delimiter = trimmed == delimiter || trimmed == format!("{delimiter}--");
        if is_delimiter {
            if let Some(start) = current_start {
                parts.push(&body[start..offset]);
            }
            current_start = (trimmed == delimiter).then_some(offset + line.len());
            if trimmed != delimiter {
                break;
            }
        }
        offset += line.len();
    }
    // A missing close delimiter (a truncated report) still yields the part
    // that was open when the bytes ran out — tolerance over completeness.
    if let Some(start) = current_start
        && start <= body.len()
    {
        parts.push(&body[start..]);
    }
    parts
}

/// Parses a `message/delivery-status` body (RFC 3464 §2.1): a per-message
/// field group, then one per-recipient group per blank line. Groups that
/// name no recipient address (including the per-message group) are skipped.
fn parse_delivery_status(body: &str) -> Vec<DsnRecipient> {
    let normalised = body.replace("\r\n", "\n");
    normalised
        .split("\n\n")
        .filter_map(|group| {
            let address = group_field(group, "original-recipient")
                .or_else(|| group_field(group, "final-recipient"))
                .and_then(|v| strip_address_type(&v))?;
            let action = group_field(group, "action")?.to_ascii_lowercase();
            let status = group_field(group, "status").filter(|s| is_enhanced_status(s));
            Some(DsnRecipient {
                address,
                action,
                status,
            })
        })
        .collect()
}

/// One field of a delivery-status group, unfolded, by lowercase name.
fn group_field(group: &str, name: &str) -> Option<String> {
    header_value(group, name).filter(|v| !v.is_empty())
}

/// Strips the `address-type;` prefix of a Final-/Original-Recipient value
/// (RFC 3464 §2.3.1/2.3.2: `address-type ; generic-address`). A value with
/// no `;` is taken whole — some providers omit the type, and the address is
/// the part we act on.
fn strip_address_type(value: &str) -> Option<String> {
    let address = match value.split_once(';') {
        Some((_type, address)) => address.trim(),
        None => value.trim(),
    };
    (!address.is_empty()).then(|| address.to_owned())
}

/// Whether a token is an RFC 3463 `class.subject.detail` status.
fn is_enhanced_status(token: &str) -> bool {
    let mut parts = token.split('.');
    let class_ok = parts
        .next()
        .is_some_and(|p| p == "2" || p == "4" || p == "5");
    let rest_ok = (0..2).all(|_| {
        parts
            .next()
            .is_some_and(|p| !p.is_empty() && p.len() <= 3 && p.bytes().all(|b| b.is_ascii_digit()))
    });
    class_ok && rest_ok && parts.next().is_none()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    /// A DSN as our own composer writes one — the round trip that keeps the
    /// two halves of RFC 3464 in this crate agreeing with each other.
    fn our_own(status: &str, action: &str) -> String {
        let failed = vec![crate::dsn::FailedRecipient {
            recipient: "gone@example.test".to_owned(),
            status: status.to_owned(),
            diagnostic: format!("smtp; 550 {status} no such user"),
        }];
        let composed = crate::dsn::compose(
            "mx.alo.test",
            "bounces@news.alo.test",
            "42.1.1",
            &failed,
            "Subject: original",
            &"2026-08-27T12:00:00+00:00[UTC]".parse().unwrap(),
        );
        // The composer only writes `Action: failed`; the tests below need
        // the other actions too.
        composed.replace("Action: failed", &format!("Action: {action}"))
    }

    #[test]
    fn our_own_dsn_parses_back_whole() {
        let recipients = parse_dsn(our_own("5.1.1", "failed").as_bytes()).unwrap();
        assert_eq!(
            recipients,
            vec![DsnRecipient {
                address: "gone@example.test".to_owned(),
                action: "failed".to_owned(),
                status: Some("5.1.1".to_owned()),
            }]
        );
        assert_eq!(classify(&recipients[0]), DsnVerdict::Hard);
    }

    #[test]
    fn a_transient_report_is_soft_and_a_delivery_is_ignored() {
        let soft = parse_dsn(our_own("4.2.2", "failed").as_bytes()).unwrap();
        assert_eq!(classify(&soft[0]), DsnVerdict::Soft);
        let delayed = parse_dsn(our_own("4.4.1", "delayed").as_bytes()).unwrap();
        assert_eq!(classify(&delayed[0]), DsnVerdict::Soft);
        let delivered = parse_dsn(our_own("2.0.0", "delivered").as_bytes()).unwrap();
        assert_eq!(classify(&delivered[0]), DsnVerdict::Ignore);
    }

    #[test]
    fn failed_without_a_readable_status_is_soft_not_hard() {
        // RFC 3464 §2.3.4 makes Status required, but suppression is
        // irreversible — a report that breaks the grammar gets the verdict
        // that can be wrong for free.
        let garbled = DsnRecipient {
            address: "gone@example.test".to_owned(),
            action: "failed".to_owned(),
            status: None,
        };
        assert_eq!(classify(&garbled), DsnVerdict::Soft);
    }

    #[test]
    fn original_recipient_wins_over_final_recipient() {
        // §2.3.1: Original-Recipient is the address as first specified —
        // the one our send ledger knows — before forwarding rewrote it.
        let dsn = "Content-Type: multipart/report; report-type=delivery-status; boundary=\"b\"\r\n\
                   \r\n\
                   --b\r\n\
                   Content-Type: message/delivery-status\r\n\
                   \r\n\
                   Reporting-MTA: dns; their-mx.example\r\n\
                   \r\n\
                   Original-Recipient: rfc822; ann@lead.test\r\n\
                   Final-Recipient: rfc822; ann@forwarded.example\r\n\
                   Action: failed\r\n\
                   Status: 5.1.1\r\n\
                   --b--\r\n";
        let recipients = parse_dsn(dsn.as_bytes()).unwrap();
        assert_eq!(recipients[0].address, "ann@lead.test");
    }

    #[test]
    fn a_multi_recipient_report_yields_every_group() {
        let dsn = "Content-Type: multipart/report; report-type=delivery-status; boundary=b\r\n\
                   \r\n\
                   --b\r\n\
                   Content-Type: message/delivery-status\r\n\
                   \r\n\
                   Reporting-MTA: dns; their-mx.example\r\n\
                   \r\n\
                   Final-Recipient: rfc822; ann@lead.test\r\n\
                   Action: failed\r\n\
                   Status: 5.1.1\r\n\
                   \r\n\
                   Final-Recipient: rfc822; ben@lead.test\r\n\
                   Action: delayed\r\n\
                   Status: 4.4.1\r\n\
                   --b--\r\n";
        let recipients = parse_dsn(dsn.as_bytes()).unwrap();
        assert_eq!(recipients.len(), 2);
        assert_eq!(classify(&recipients[0]), DsnVerdict::Hard);
        assert_eq!(classify(&recipients[1]), DsnVerdict::Soft);
    }

    #[test]
    fn tolerance_bare_lf_folded_headers_and_case() {
        // A real provider's bounce: LF-only lines, folded Content-Type,
        // uppercase field names, an unquoted boundary.
        let dsn = "Content-TYPE: multipart/report;\n\
                   \treport-type=delivery-status;\n\
                   \tboundary=xYz\n\
                   \n\
                   --xYz\n\
                   Content-Type: message/delivery-status\n\
                   \n\
                   REPORTING-MTA: dns; mx.example\n\
                   \n\
                   FINAL-RECIPIENT: RFC822; Ann@Lead.TEST\n\
                   ACTION: FAILED\n\
                   STATUS: 5.2.1\n\
                   --xYz--\n";
        let recipients = parse_dsn(dsn.as_bytes()).unwrap();
        assert_eq!(recipients[0].address, "Ann@Lead.TEST");
        assert_eq!(recipients[0].action, "failed");
        assert_eq!(classify(&recipients[0]), DsnVerdict::Hard);
    }

    #[test]
    fn anything_that_is_not_a_delivery_status_report_is_none() {
        // Not a report at all.
        assert_eq!(parse_dsn(b"Subject: hello\r\n\r\nplain mail\r\n"), None);
        // A report of a different type (an ARF complaint, RFC 5965).
        let arf = "Content-Type: multipart/report; report-type=feedback-report; boundary=b\r\n\
                   \r\n--b\r\nContent-Type: message/feedback-report\r\n\r\nFeedback-Type: abuse\r\n--b--\r\n";
        assert_eq!(parse_dsn(arf.as_bytes()), None);
        // multipart/report with no boundary parameter.
        let broken = "Content-Type: multipart/report; report-type=delivery-status\r\n\r\nbody";
        assert_eq!(parse_dsn(broken.as_bytes()), None);
        // Bytes that are not a message.
        assert_eq!(parse_dsn(&[0xFF, 0xFE, 0x00]), None);
        assert_eq!(parse_dsn(b""), None);
    }

    #[test]
    fn a_report_with_no_recipient_groups_is_an_empty_report_not_none() {
        // A well-formed report whose status part names nobody: it IS a DSN,
        // there is just nothing to act on.
        let dsn = "Content-Type: multipart/report; report-type=delivery-status; boundary=b\r\n\
                   \r\n\
                   --b\r\n\
                   Content-Type: message/delivery-status\r\n\
                   \r\n\
                   Reporting-MTA: dns; mx.example\r\n\
                   --b--\r\n";
        assert_eq!(parse_dsn(dsn.as_bytes()), Some(Vec::new()));
    }

    #[test]
    fn a_truncated_report_still_yields_what_arrived() {
        // The close delimiter never came (connection cut mid-DATA upstream);
        // the delivery-status part that did arrive is still read.
        let dsn = "Content-Type: multipart/report; report-type=delivery-status; boundary=b\r\n\
                   \r\n\
                   --b\r\n\
                   Content-Type: message/delivery-status\r\n\
                   \r\n\
                   Reporting-MTA: dns; mx.example\r\n\
                   \r\n\
                   Final-Recipient: rfc822; ann@lead.test\r\n\
                   Action: failed\r\n\
                   Status: 5.1.1\r\n";
        let recipients = parse_dsn(dsn.as_bytes()).unwrap();
        assert_eq!(recipients.len(), 1);
    }

    #[test]
    fn status_grammar_is_rfc_3463_and_nothing_wider() {
        for good in ["2.0.0", "4.4.1", "5.7.999"] {
            assert!(is_enhanced_status(good), "{good}");
        }
        for bad in ["5.7", "9.9.9", "5.a.1", "5.7.1.2", "", "failed"] {
            assert!(!is_enhanced_status(bad), "{bad}");
        }
    }
}
