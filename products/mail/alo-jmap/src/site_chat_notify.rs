//! Telling the site owner their assistant hit its monthly ceiling
//! (ADR 0040 §3, item S3.02c): "at the ceiling the bot does not degrade
//! quietly — it says it is unavailable and offers the contact form, **and
//! the tenant is told**". The telling is one INTERNAL message in the site
//! owner's own inbox — never outbound mail — delivered through the account
//! door, the same posture as the form, order and booking notifications.
//!
//! Runs as a background sweep from `main.rs`:
//! [`alo_store::Store::claim_chat_ceiling_notifications`] marks ledger rows
//! notified as it claims them (at-most-once per site-month — see that
//! module's doc), then each claimed row becomes one RFC 5322 message. A
//! delivery failure is logged — never with tenant data (Law 1) — and the
//! sweep moves on; the settings screen shows the hit ceiling regardless, so
//! nothing is silently lost.
//!
//! Like the sibling notifiers, the message text is English-only for now —
//! localizing server-generated mail is flagged for the wave review.

use alo_store::{ChatCeilingNotification, Store};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;

use crate::mime::{Addr, encode_unstructured, format_addr};

/// How many hit ceilings one sweep round claims. A round that claims the
/// full batch is immediately followed by another in the same tick.
const BATCH: i64 = 100;

/// Claims every hit ceiling awaiting notification and delivers each to its
/// site owner's inbox. Returns the number delivered.
pub async fn run_due(store: &Store) -> usize {
    let mut delivered = 0;
    loop {
        let due = match store.claim_chat_ceiling_notifications(BATCH).await {
            Ok(due) => due,
            Err(error) => {
                tracing::warn!(%error, "assistant-ceiling sweep: claim failed");
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
                    tracing::warn!(%error, "assistant-ceiling sweep: delivery failed");
                }
            }
        }
        if batch_len < BATCH as usize {
            return delivered;
        }
    }
}

/// Integer cents as a euro amount for the message body — money stays integer
/// cents everywhere else.
fn euros(cents: i64) -> String {
    format!("€{}.{:02}", cents / 100, (cents % 100).abs())
}

/// One notification as a complete RFC 5322 message. The only free text that
/// crosses into it is the tenant's own site name, which reaches headers
/// through the RFC 2047 path (CR/LF cannot survive it) and the body
/// base64-encoded — nothing here can inject headers or structure.
fn build_notification(n: &ChatCeilingNotification, owner_email: Option<&str>) -> String {
    let domain = crate::sites::sites_domain();
    // A display identity on the site's own subdomain; nothing sends from it.
    let from = format_addr(&Addr {
        name: Some(format!("{} website assistant", n.site_name)),
        email: format!("no-reply@{}.{domain}", n.site_subdomain),
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
        "Your website assistant reached its monthly budget ({})",
        n.site_name
    ));
    let body = format!(
        "The assistant on {site} ({sub}.{domain}) has spent {spent} of its \
         {ceiling} budget for {month} and is now paused.\r\n\
         \r\n\
         Visitors are told the assistant is unavailable and are offered your \
         contact form instead. Nothing else on your website is affected.\r\n\
         \r\n\
         It will answer again at the start of next month — or right away if \
         you raise the monthly budget in the website's assistant settings.\r\n",
        site = n.site_name,
        sub = n.site_subdomain,
        spent = euros(n.spent_cents),
        ceiling = euros(n.monthly_ceiling_cents),
        month = n.month,
    );
    let body_b64 = wrap76(&B64.encode(body));
    format!(
        "From: {from}\r\n\
         {to}\
         Subject: {subject}\r\n\
         Message-ID: <assistant-ceiling-{month}@{sub}.{domain}>\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         Content-Transfer-Encoding: base64\r\n\
         \r\n\
         {body_b64}\r\n",
        month = n.month,
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
