//! `EmailSubmission/set` (RFC 8621 §7): sending a composed message.
//!
//! The draft is sent through the SMTP **trusted internal submission listener**
//! using the shared production SMTP client (`alo-smtp-client`) — the same
//! client the delivery path uses — so the message is DKIM-signed, queued, and
//! delivered by the existing outbound path. See
//! `docs/design/email-submission.md`.
//!
//! Send-as is enforced here: `MAIL FROM` must be the authenticated user's own
//! canonical address or a registered alias (`forbiddenFrom` otherwise), so a
//! bearer token cannot send as another identity. On success the message is
//! un-drafted, marked seen, and filed into Sent.

use std::collections::HashSet;

use alo_smtp_client::client::{OutboundSession, RcptOutcome};
use alo_store::{MAX_PAGE, MessageId, Page};
use serde_json::{Map, Value, json};

use crate::state::{Account, AppState, SendMode};

/// Maximum recipients accepted in one submission (anti-abuse; a per-user send
/// rate quota is a tracked follow-up — see docs/design/security-audit-followups.md).
const MAX_RECIPIENTS: usize = 100;

/// `EmailSubmission/set`. Only `create` is meaningful (a submission is a
/// transient action); `update`/`destroy` are accepted as no-ops.
pub async fn set(account: &Account, args: &Value, state: &AppState) -> Result<Value, Value> {
    crate::api::check_account(args, account)?;
    let st = account.acc.state().await.unwrap_or_else(|_| String::new());

    let mut created = Map::new();
    let mut not_created = Map::new();
    if let Some(creates) = args.get("create").and_then(Value::as_object) {
        for (cid, props) in creates {
            match create_one(account, props, state).await {
                Ok(email_id) => {
                    post_send(&account.acc, &MessageId::new(&email_id)).await;
                    created.insert(
                        cid.clone(),
                        json!({ "id": format!("s{email_id}"), "emailId": email_id }),
                    );
                }
                Err(e) => {
                    not_created.insert(cid.clone(), e);
                }
            }
        }
    }

    Ok(json!({
        "accountId": account.account_id(),
        "oldState": st, "newState": st,
        "created": created, "notCreated": not_created,
        "updated": {}, "notUpdated": {}, "destroyed": [], "notDestroyed": {}
    }))
}

async fn create_one(account: &Account, props: &Value, state: &AppState) -> Result<String, Value> {
    let prepared = validate_and_prepare(account, props, state).await?;

    // Submit through the trusted internal listener (DKIM-signed + queued there).
    // Failures are logged server-side without recipient/body detail.
    let Some(addr) = state.submission_addr.as_deref() else {
        tracing::error!("EmailSubmission/set: no submission listener configured");
        return Err(set_err("forbiddenToSend", "sending is not available"));
    };
    // Privacy: strip the `Bcc:` header from the bytes put on the wire so
    // recipients never learn who was blind-copied. The sender's own copy (moved
    // to Sent by `post_send`) keeps `bytes` intact; delivery to the bcc'd
    // addresses happens through the envelope `rcptTo`.
    let wire = strip_bcc_header(prepared.bytes.as_ref());
    submit(addr, &prepared.mail_from, &prepared.rcpts, &wire)
        .await
        .map_err(|reason| {
            tracing::error!(reason = %reason, "EmailSubmission/set: submission failed");
            set_err("forbiddenToSend", "the message could not be sent")
        })?;

    Ok(prepared.mid.as_str().to_owned())
}

/// A draft that has passed every send check, ready to be submitted now (an
/// immediate `EmailSubmission/set`) or later (a scheduled send). Both paths must
/// run exactly the same validation, so it lives here once.
pub(crate) struct Prepared {
    pub mid: MessageId,
    pub bytes: bytes::Bytes,
    pub mail_from: String,
    pub rcpts: Vec<String>,
}

/// Validate a submission request against the authenticated account: the draft
/// exists and is a draft, its visible `From:` and the envelope `mailFrom` are
/// addresses this account owns (anti-spoof), and the envelope has a sane
/// recipient set. Returns the validated envelope; performs no send. Shared by
/// immediate submission and scheduled send so the security checks cannot drift
/// apart.
pub(crate) async fn validate_and_prepare(
    account: &Account,
    props: &Value,
    state: &AppState,
) -> Result<Prepared, Value> {
    // 0. Delegation (ADR 0017): sending from a delegated mailbox requires a send
    // grant. (The signed-in user's own account has `delegated == None`.)
    if let Some(d) = &account.delegated
        && !d.send_mode.can_send()
    {
        return Err(set_err(
            "forbiddenToSend",
            "you don't have permission to send from this mailbox",
        ));
    }

    // 1. The draft to send.
    let Some(email_id) = props.get("emailId").and_then(Value::as_str) else {
        return Err(set_err("invalidProperties", "emailId is required"));
    };
    let mid = MessageId::new(email_id);

    // 2. Its bytes. The account door scopes this: a foreign or absent id is a
    // clean notFound, never another tenant's message.
    let bytes = account
        .acc
        .message_bytes(&mid)
        .await
        .map_err(|_| set_err("notFound", "emailId not found"))?;

    // 3. The user's valid send-from addresses (canonical + aliases).
    let ts = state.store.for_tenant(account.tenant.clone());
    let canonical = ts
        .email_of(&account.user)
        .await
        .map_err(|_| set_err("forbiddenToSend", "sender lookup failed"))?
        .ok_or_else(|| set_err("forbiddenFrom", "no address for this account"))?;
    let mut valid: HashSet<String> = HashSet::new();
    valid.insert(canonical.to_lowercase());
    if let Ok(aliases) = ts.aliases_of(&account.user).await {
        for a in aliases {
            valid.insert(a.to_lowercase());
        }
    }

    // Only a draft is sendable (a received/sent message is not re-sendable).
    let keywords = account
        .acc
        .keywords(&mid)
        .await
        .map_err(|_| set_err("forbiddenToSend", "could not read the message"))?;
    if !keywords.iter().any(|k| k == "$draft") {
        return Err(set_err("forbiddenToSend", "only a draft can be submitted"));
    }

    // The visible `From:` header — not only the SMTP envelope — MUST be an
    // address this account owns. Otherwise a bearer token could send a
    // DKIM-signed message with a forged author (intra-domain impersonation),
    // since the outbound path signs with our domain and does not rewrite From.
    let from_header = extract_from_addr(bytes.as_ref())
        .ok_or_else(|| set_err("forbiddenFrom", "the message has no From address"))?;
    if !valid.contains(&from_header) {
        return Err(set_err(
            "forbiddenFrom",
            "the message From is not an address of this account",
        ));
    }

    // 4. Envelope. mailFrom defaults to the canonical address and MUST be one
    // the account owns; rcptTo is taken from the envelope.
    let env = props.get("envelope");
    let mail_from = env
        .and_then(|e| e.get("mailFrom"))
        .and_then(|m| m.get("email"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| canonical.clone());
    if !valid_addr(&mail_from) || !valid.contains(&mail_from.to_lowercase()) {
        return Err(set_err(
            "forbiddenFrom",
            "mailFrom is not an address of this account",
        ));
    }
    let rcpts: Vec<String> = env
        .and_then(|e| e.get("rcptTo"))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|r| r.get("email").and_then(Value::as_str))
                .filter(|e| valid_addr(e))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    if rcpts.is_empty() {
        return Err(set_err(
            "noRecipients",
            "the envelope has no valid recipients",
        ));
    }
    if rcpts.len() > MAX_RECIPIENTS {
        return Err(set_err(
            "tooManyRecipients",
            "too many recipients for one message",
        ));
    }

    // On-behalf sending (ADR 0017): prepend a `Sender:` header naming the acting
    // delegate, so recipients see who actually sent from the shared mailbox
    // (`From:` stays the shared address). Send-as adds no `Sender:`.
    let bytes = match &account.delegated {
        Some(d) if d.send_mode == SendMode::OnBehalf => match ts.email_of(&d.delegate).await {
            Ok(Some(sender)) => prepend_sender_header(&bytes, &sender),
            _ => bytes,
        },
        _ => bytes,
    };

    Ok(Prepared {
        mid,
        bytes,
        mail_from,
        rcpts,
    })
}

/// Prepend a `Sender:` header to a raw message (header order is unconstrained,
/// so inserting at the top is valid and keeps the rest of the message intact).
fn prepend_sender_header(bytes: &[u8], sender: &str) -> bytes::Bytes {
    let mut out = Vec::with_capacity(bytes.len() + sender.len() + 12);
    out.extend_from_slice(format!("Sender: {sender}\r\n").as_bytes());
    out.extend_from_slice(bytes);
    bytes::Bytes::from(out)
}

/// One SMTP transaction to the internal listener via the shared client.
/// Relays a message through the trusted internal submission listener (which
/// DKIM-signs and queues it). Shared by interactive/scheduled `EmailSubmission`
/// and the system-originated signup verification mail. Addresses are never
/// logged (Law 1) — only the outcome class.
pub(crate) async fn submit(
    addr: &str,
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
    let mut session = OutboundSession::connect_addr(sockaddr, "alo-jmap", None)
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

/// Submit every scheduled send that is now due. Called on an interval by the
/// background sweeper (see `main.rs`). Rows are **claimed** (deleted) up front by
/// [`claim_due_sends`](alo_store::Store::claim_due_sends), so the schedule is
/// gone before the message reaches the wire: a crash or DB hiccup after
/// submission can never re-send it (at-most-once). Each claimed message is put
/// through the same outbound path as an interactive send; on success it is filed
/// to Sent, and a send that fails is returned to Drafts so nothing is lost (the
/// user can resend by hand) but it is never silently re-sent. Cross-tenant
/// maintenance; never surfaces to a client.
pub async fn run_due_scheduled(state: &AppState) {
    let Some(addr) = state.submission_addr.as_deref() else {
        return; // sending isn't configured on this node; nothing to do
    };
    let due = match state.store.claim_due_sends(100).await {
        Ok(due) => due,
        Err(error) => {
            tracing::warn!(%error, "scheduled-send sweep: could not claim due sends");
            return;
        }
    };
    for send in due {
        let acc = state
            .store
            .for_account(send.tenant.clone(), send.user.clone());
        // The draft may have been deleted between scheduling and now — the row is
        // already claimed, so there is nothing to clean up; just move on.
        let bytes = match acc.message_bytes(&send.message_id).await {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let wire = strip_bcc_header(bytes.as_ref());
        match submit(addr, &send.mail_from, &send.rcpts, &wire).await {
            Ok(()) => post_send(&acc, &send.message_id).await,
            Err(reason) => {
                tracing::error!(reason = %reason, "scheduled-send sweep: submission failed");
                // The claim already removed the schedule; return the draft to
                // Drafts so the message isn't stranded in Scheduled forever.
                if let Err(error) = acc.return_to_drafts(&send.message_id).await {
                    tracing::warn!(%error, "scheduled-send sweep: could not return to Drafts");
                }
            }
        }
    }
}

/// After a successful send: clear `$draft`, mark `$seen`, and file into Sent
/// (removing it from Drafts and — for a scheduled send — Scheduled). Best-effort
/// — the mail is already sent, so a filing hiccup is logged, never surfaced as a
/// send failure. Takes the account store directly so both the interactive
/// submission path and the background scheduled-send sweeper can call it.
async fn post_send(acc: &alo_store::AccountStore, mid: &MessageId) {
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

fn set_err(kind: &str, description: &str) -> Value {
    json!({ "type": kind, "description": description })
}

/// A safe addr-spec for an SMTP command: non-empty, has `@`, and contains no
/// whitespace, control chars, or angle brackets (no SMTP-command injection).
pub(crate) fn valid_addr(addr: &str) -> bool {
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
pub(crate) fn extract_from_addr(msg: &[u8]) -> Option<String> {
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
fn strip_bcc_header(raw: &[u8]) -> Vec<u8> {
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
    use super::{extract_from_addr, strip_bcc_header, valid_addr};

    #[test]
    fn accepts_a_normal_address() {
        assert!(valid_addr("alice@example.eu"));
    }

    #[test]
    fn rejects_injection_and_malformed() {
        assert!(!valid_addr("a@x.eu\r\nRCPT TO:<evil@x>"));
        assert!(!valid_addr("a@x.eu evil@x"));
        assert!(!valid_addr("<a@x.eu>"));
        assert!(!valid_addr("noatsign"));
        assert!(!valid_addr(""));
    }

    #[test]
    fn extracts_from_addr_forms() {
        assert_eq!(
            extract_from_addr(b"From: \"Disan\" <Disan@Namel3ss.com>\r\nTo: x@y\r\n\r\nbody"),
            Some("disan@namel3ss.com".to_owned())
        );
        assert_eq!(
            extract_from_addr(b"Subject: hi\r\nfrom: bare@example.eu\r\n\r\nbody"),
            Some("bare@example.eu".to_owned())
        );
        assert_eq!(extract_from_addr(b"To: x@y\r\n\r\nbody"), None);
    }

    #[test]
    fn strips_bcc_from_the_wire_only() {
        // A simple Bcc between other headers is removed; the rest is byte-exact.
        let msg = b"From: a@x\r\nTo: b@y\r\nBcc: secret@z\r\nSubject: hi\r\n\r\nbody\r\n";
        let out = strip_bcc_header(msg);
        assert_eq!(out, b"From: a@x\r\nTo: b@y\r\nSubject: hi\r\n\r\nbody\r\n");
        assert!(!out.windows(4).any(|w| w.eq_ignore_ascii_case(b"bcc:")));

        // A FOLDED Bcc (continuation lines) is removed whole.
        let folded = b"To: b@y\r\nBcc: one@z,\r\n two@z,\r\n\tthree@z\r\nSubject: s\r\n\r\nb";
        assert_eq!(strip_bcc_header(folded), b"To: b@y\r\nSubject: s\r\n\r\nb");

        // A body line that merely starts with 'Bcc:' is NOT touched.
        let body_bcc = b"To: b@y\r\n\r\nBcc: this is body text\r\n";
        assert_eq!(strip_bcc_header(body_bcc), body_bcc);

        // Case-insensitive on the field name; no Bcc is a no-op.
        assert_eq!(
            strip_bcc_header(b"bCc: x@z\r\nTo: y@w\r\n\r\nb"),
            b"To: y@w\r\n\r\nb"
        );
        assert_eq!(strip_bcc_header(b"To: y@w\r\n\r\nb"), b"To: y@w\r\n\r\nb");
    }
}
