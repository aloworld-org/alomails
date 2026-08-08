//! Owner notification for new site-form submissions (ADR 0036, S1.16c).
//!
//! A visitor's contact-form submission (stored by the public `alo-sites`
//! endpoint, or any other writer) is delivered to the **site owner's own
//! inbox** as an ordinary message — by INTERNAL delivery through the
//! account door, exactly like SMTP local delivery. Nothing here ever
//! sends outbound mail: the visitor's address appears only as `Reply-To`,
//! so answering is one deliberate reply by the owner through their normal
//! send path.
//!
//! Runs as a background sweep from `main.rs` (the snooze-sweeper posture):
//! [`alo_store::Store::claim_form_notifications`] marks rows notified as it
//! claims them (at-most-once — see that module's doc), then each claimed
//! submission becomes one RFC 5322 message. A delivery failure (e.g. the
//! owning user was deleted) is logged — never with addresses or content
//! (Law 1) — and the sweep moves on; the submission row itself remains in
//! the owner's submissions list regardless.
//!
//! The message text is deliberately assembled from neutral framing plus
//! the submission's own words; like the calendar RSVP and DSN mail it is
//! English-only for now — localizing server-generated mail is flagged for
//! the wave review (the web i18n catalogs do not reach this process).

use alo_store::{FormNotification, Store};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use time::format_description::well_known::Rfc2822;

use crate::mime::{Addr, encode_unstructured, format_addr};

/// How many submissions one sweep round claims. A round that claims the
/// full batch is immediately followed by another in the same tick, so a
/// backlog drains fast without an unbounded single query.
const BATCH: i64 = 100;

/// Claims every submission awaiting notification and delivers each to its
/// site owner's inbox. Returns the number delivered.
pub async fn run_due(store: &Store) -> usize {
    let mut delivered = 0;
    loop {
        let due = match store.claim_form_notifications(BATCH).await {
            Ok(due) => due,
            Err(error) => {
                tracing::warn!(%error, "form-notification sweep: claim failed");
                return delivered;
            }
        };
        let batch_len = due.len();
        for notification in due {
            let account =
                store.for_account(notification.tenant.clone(), notification.owner.clone());
            // Best-effort To: header — the notification is addressed to the
            // owner's inbox by the door itself, the header is display only.
            let owner_email = store
                .for_tenant(notification.tenant.clone())
                .email_of(&notification.owner)
                .await
                .ok()
                .flatten();
            let raw = build_notification(&notification, owner_email.as_deref());
            match account.deliver(raw.as_bytes()).await {
                Ok(_) => delivered += 1,
                Err(error) => {
                    tracing::warn!(%error, "form-notification sweep: delivery failed");
                }
            }
        }
        if batch_len < BATCH as usize {
            return delivered;
        }
    }
}

/// One notification as a complete RFC 5322 message. Every submission field
/// crosses into the message safely: the sender's name reaches headers only
/// through the RFC 2047 path (which strips CR/LF), the address was gated to
/// contain no whitespace/control characters, and the free-text body travels
/// base64-encoded — a submission cannot inject headers or structure.
fn build_notification(n: &FormNotification, owner_email: Option<&str>) -> String {
    let domain = crate::sites::sites_domain();
    // The From is a display identity on the site's own (sub)domain; nothing
    // ever sends outbound from it, and replies go to the visitor instead.
    let from = format_addr(&Addr {
        name: Some(format!("{} contact form", n.site_name)),
        email: format!("no-reply@{}.{domain}", n.site_subdomain),
    });
    let reply_to = format_addr(&Addr {
        name: Some(n.sender_name.clone()),
        email: n.sender_email.clone(),
    });
    let to = owner_email
        .map(|email| {
            format!(
                "To: {}\r\n",
                format_addr(&Addr {
                    name: None,
                    email: email.to_owned()
                })
            )
        })
        .unwrap_or_default();
    let subject = encode_unstructured(&format!(
        "New message from {} ({})",
        n.sender_name, n.site_name
    ));
    let date = n
        .received_at
        .format(&Rfc2822)
        .unwrap_or_else(|_| String::new());
    let date = if date.is_empty() {
        String::new()
    } else {
        format!("Date: {date}\r\n")
    };
    let body = format!(
        "{name} <{email}> wrote through the \"{form}\" form on {site}:\r\n\
         \r\n\
         {message}\r\n\
         \r\n\
         -- \r\n\
         Reply to this email to answer {name} directly.\r\n",
        name = n.sender_name,
        email = n.sender_email,
        form = n.form_name,
        site = n.site_name,
        message = n.message,
    );
    let body_b64 = wrap76(&B64.encode(body));
    format!(
        "From: {from}\r\n\
         Reply-To: {reply_to}\r\n\
         {to}\
         Subject: {subject}\r\n\
         {date}\
         Message-ID: <{id}@{sub}.{domain}>\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         Content-Transfer-Encoding: base64\r\n\
         \r\n\
         {body_b64}\r\n",
        id = n.submission.as_str(),
        sub = n.site_subdomain,
    )
}

/// Folds a base64 string to 76-character lines (RFC 2045 §6.8).
fn wrap76(b64: &str) -> String {
    b64.as_bytes()
        .chunks(76)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join("\r\n")
}
