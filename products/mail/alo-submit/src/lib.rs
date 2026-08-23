//! The one path a composed message takes out of alo.
//!
//! Two protocols now compose mail — JMAP for alo's own clients and MAPI for
//! Outlook — and both must leave through the same door. Not for tidiness: this
//! module holds the **send-as** check, the `Bcc` strip and the filing that
//! happens after a send, and a second copy of any of those is a second place
//! for one of them to be wrong.
//!
//! ## What is enforced here
//!
//! * **A message's visible `From:` must be an address the authenticated
//!   account owns** — its canonical address or a registered alias. Without
//!   this, holding any credential for any account would let somebody send as
//!   anybody. The check is on the header a recipient *reads*, not only on the
//!   envelope, because the envelope is not what anybody looks at.
//! * **`Bcc:` never goes on the wire.** The sender's stored copy keeps it; the
//!   bytes transmitted do not, and blind recipients are reached through the
//!   envelope instead. Only the header block is examined, so a body line
//!   beginning `Bcc:` is left alone.
//! * **Addresses are checked before they reach an SMTP command.** An address
//!   containing CR, LF or angle brackets is refused rather than escaped: this
//!   is where command injection would enter.
//!
//! ## Where the message actually goes
//!
//! To the deployment's own trusted submission listener, over the same
//! `alo-smtp-client` the delivery path uses — so it is DKIM-signed, queued and
//! delivered by the existing outbound path rather than by a second one that
//! would have to be kept in step.

use alo_smtp_client::client::{OutboundSession, RcptOutcome};
use alo_store::{MAX_PAGE, MessageId, Page};

/// One SMTP transaction to the deployment's internal submission listener.
///
/// The listener DKIM-signs the message and queues it, so this is the hand-off
/// rather than the delivery. `client_name` is what the service calls itself in
/// `EHLO`; it identifies which surface composed the mail in the listener's own
/// logs, and is the only thing about this call that differs between them.
///
/// Addresses are never logged (Law 1) — only the outcome class.
///
/// # Errors
/// A description of which step failed, for the caller's log. Never returned to
/// a client: a sender learns that the message did not go, not why the relay
/// said so.
pub async fn submit(
    addr: &str,
    client_name: &str,
    mail_from: &str,
    rcpts: &[String],
    message: &[u8],
) -> Result<(), String> {
    let sockaddr = tokio::net::lookup_host(addr)
        .await
        .map_err(|e| format!("resolve: {e}"))?
        .next()
        .ok_or_else(|| "no address for submission host".to_owned())?;
    // No pinned source address: this hop reaches the co-located submission
    // listener inside the deployment. The egress address that matters is chosen
    // where the message actually leaves for the internet — alo-smtp's outbound
    // queue (ADR 0044 §1).
    let mut session = OutboundSession::connect_addr(sockaddr, client_name, None)
        .await
        .map_err(|e| format!("connect: {e}"))?;
    let outcomes = session
        .deliver(Some(mail_from), rcpts, message)
        .await
        .map_err(|e| format!("transaction: {e}"))?;
    session.quit().await;
    // The message reaches DATA only once any recipient is accepted, so it is
    // spooled the moment one is Delivered. Treat "any accepted" as success:
    // erroring after a partial acceptance would make a client retry and
    // double-send to the accepted recipients. Only a zero-acceptance result
    // (the relay took nothing) is a send failure. Addresses are never logged
    // (Law 1) — only the outcome class.
    if outcomes.iter().any(|o| matches!(o, RcptOutcome::Delivered)) {
        Ok(())
    } else {
        Err("the relay accepted no recipients".into())
    }
}

/// After a successful send: clear `$draft`, mark `$seen`, and file into Sent,
/// removing the message from Drafts and — for a scheduled send — Scheduled.
///
/// Best-effort by design. The mail has already gone; a filing hiccup is logged
/// and never surfaced as a send failure, because telling a sender their message
/// failed after it was delivered is the one outcome that makes them send it
/// twice.
pub async fn post_send(acc: &alo_store::AccountStore, mid: &MessageId) {
    if let Err(error) = acc.set_keyword(mid, "$draft", false).await {
        tracing::warn!(%error, "post-send: could not clear $draft");
    }
    if let Err(error) = acc.set_keyword(mid, "$seen", true).await {
        tracing::warn!(%error, "post-send: could not set $seen");
    }
    let boxes = match acc.mailboxes(Page::first(MAX_PAGE)).await {
        Ok(boxes) => boxes,
        Err(error) => {
            tracing::warn!(%error, "post-send: mailbox list failed");
            return;
        }
    };
    let Some(sent) = boxes.iter().find(|m| m.role.as_deref() == Some("sent")) else {
        return; // no Sent mailbox: leave the message where it is
    };
    if let Err(error) = acc.add_to_mailbox(mid, &sent.id).await {
        tracing::warn!(%error, "post-send: could not file to Sent");
        return;
    }
    for src in boxes
        .iter()
        .filter(|m| matches!(m.role.as_deref(), Some("drafts" | "scheduled")))
    {
        if let Err(error) = acc.remove_from_mailbox(mid, &src.id).await {
            tracing::warn!(%error, "post-send: could not remove from source mailbox");
        }
    }
}

/// A safe addr-spec for an SMTP command: non-empty, has `@`, and contains no
/// whitespace, control chars, or angle brackets (no SMTP-command injection).
pub fn valid_addr(addr: &str) -> bool {
    !addr.is_empty()
        && addr.len() <= 320
        && addr.contains('@')
        && addr
            .bytes()
            .all(|b| b > 0x20 && b != b'<' && b != b'>' && b != 0x7f)
}

/// The lowercase addr-spec of a message's `From:` header (the address inside
/// the last `<…>`, else the trimmed value), honoring folded continuation
/// lines. `None` if absent or without an `@`. Used to bind the *visible*
/// author to the authenticated account (defence against From spoofing).
pub fn extract_from_addr(msg: &[u8]) -> Option<String> {
    let end = msg
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .unwrap_or(msg.len());
    let text = String::from_utf8_lossy(&msg[..end]);
    let mut lines = text.split("\r\n").peekable();
    let mut value: Option<String> = None;
    while let Some(line) = lines.next() {
        if line.len() >= 5
            && line
                .get(..5)
                .is_some_and(|p| p.eq_ignore_ascii_case("from:"))
        {
            let mut v = line[5..].to_string();
            while let Some(next) = lines.peek() {
                if next.starts_with(' ') || next.starts_with('\t') {
                    v.push(' ');
                    v.push_str(next.trim_start());
                    lines.next();
                } else {
                    break;
                }
            }
            value = Some(v);
            break;
        }
    }
    let v = value?;
    let addr = match (v.rfind('<'), v.rfind('>')) {
        (Some(lt), Some(gt)) if lt < gt => v[lt + 1..gt].trim().to_string(),
        _ => v.trim().to_string(),
    };
    if addr.contains('@') {
        Some(addr.to_lowercase())
    } else {
        None
    }
}

/// Removes any `Bcc:` header field (and its folded continuation lines) from a
/// message's header block, leaving every other byte unchanged. This is the
/// privacy guarantee for blind-carbon: the sender's stored copy keeps `Bcc:`,
/// but the bytes transmitted to recipients must not. Only the header section is
/// examined — a body line that happens to start with `Bcc:` is untouched.
pub fn strip_bcc_header(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    let mut in_headers = true;
    let mut skipping = false;
    while i < raw.len() {
        let end = raw[i..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(raw.len(), |p| i + p + 1);
        let line = &raw[i..end];
        if in_headers {
            if line == b"\r\n" || line == b"\n" {
                // The blank line ends the header block; copy it and stop.
                in_headers = false;
                skipping = false;
            } else if !matches!(line.first(), Some(b' ' | b'\t')) {
                // A new header field: skip it (and its folds) iff it is Bcc.
                skipping = line.len() >= 4 && line[..4].eq_ignore_ascii_case(b"bcc:");
            }
            if !skipping {
                out.extend_from_slice(line);
            }
        } else {
            out.extend_from_slice(line);
        }
        i = end;
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{extract_from_addr, strip_bcc_header, valid_addr};

    /// Builds a message from lines joined with CRLF, as a message actually is.
    ///
    /// Written this way rather than with escapes inside a literal because CRLF
    /// is part of what is under test: a test that quietly used bare LF would
    /// pass against a parser that is wrong about line endings.
    fn message(lines: &[&str]) -> Vec<u8> {
        let mut out = String::new();
        for line in lines {
            out.push_str(line);
            out.push_str("\r\n");
        }
        out.into_bytes()
    }

    #[test]
    fn an_ordinary_address_is_accepted() {
        assert!(valid_addr("alice@example.eu"));
    }

    #[test]
    fn an_address_that_could_inject_an_smtp_command_is_refused() {
        // The whole reason this check exists: a CRLF inside an address would
        // end the `MAIL FROM` line and begin a command of the caller's
        // choosing.
        let injected = String::from("a@x.eu") + "\r\n" + "RCPT TO:<evil@x.eu>";
        assert!(!valid_addr(&injected));
        assert!(!valid_addr("a@x.eu evil@x"));
        assert!(!valid_addr("<a@x.eu>"));
        assert!(!valid_addr("noatsign"));
        assert!(!valid_addr(""));
    }

    #[test]
    fn the_from_address_is_read_out_of_its_several_shapes() {
        assert_eq!(
            extract_from_addr(&message(&["From: a@x.eu", "", "body"])),
            Some("a@x.eu".to_owned())
        );
        assert_eq!(
            extract_from_addr(&message(&["From: Anna <A@X.EU>", "", "body"])),
            Some("a@x.eu".to_owned()),
            "compared lowercase, so casing alone cannot defeat the check"
        );
        // Folded across two lines, which is legal and easy to miss.
        assert_eq!(
            extract_from_addr(&message(&["From: Anna", " <a@x.eu>", "", "body"])),
            Some("a@x.eu".to_owned())
        );
        assert_eq!(
            extract_from_addr(&message(&["Subject: none", "", "body"])),
            None
        );
    }

    #[test]
    fn a_from_header_in_the_body_is_not_mistaken_for_the_real_one() {
        // The search stops at the blank line. Otherwise a second `From:` in
        // the body would let somebody choose which one gets checked.
        assert_eq!(
            extract_from_addr(&message(&["Subject: x", "", "From: evil@x.eu"])),
            None
        );
    }

    #[test]
    fn bcc_is_removed_from_the_header_block_along_with_its_folds() {
        let raw = message(&[
            "To: a@x.eu",
            "Bcc: secret@x.eu,",
            " more@x.eu",
            "Subject: s",
            "",
            "body",
        ]);
        let text = String::from_utf8(strip_bcc_header(&raw)).unwrap();
        assert!(!text.contains("secret@x.eu"), "{text:?}");
        assert!(
            !text.contains("more@x.eu"),
            "the folded continuation survived: {text:?}"
        );
        assert!(text.contains("To: a@x.eu"));
        assert!(text.contains("Subject: s"));
        assert!(text.contains("body"));
    }

    #[test]
    fn a_body_line_that_looks_like_a_bcc_header_is_left_alone() {
        let raw = message(&["To: a@x.eu", "", "Bcc: this is prose"]);
        let text = String::from_utf8(strip_bcc_header(&raw)).unwrap();
        assert!(text.contains("Bcc: this is prose"), "{text:?}");
    }
}
