//! Owner notification for new catalog orders (ADR 0036; the no-checkout order
//! form of ADR 0041), the sibling of [`crate::site_notify`].
//!
//! An order placed on a published site (stored by the public `alo-sites`
//! endpoint) is delivered to the **site owner's own inbox** as an ordinary
//! message — by INTERNAL delivery through the account door, exactly like SMTP
//! local delivery. Nothing here sends outbound mail: the customer's address
//! appears only as `Reply-To`, so confirming an order is one deliberate reply
//! by the owner through their normal send path.
//!
//! Runs as a background sweep from `main.rs`:
//! [`alo_store::Store::claim_order_notifications`] marks rows notified as it
//! claims them (at-most-once — see that module's doc), then each claimed order
//! becomes one RFC 5322 message listing what was asked for, at the prices the
//! publish carried. A delivery failure is logged — never with addresses or
//! content (Law 1) — and the sweep moves on; the order itself remains in the
//! owner's order list regardless.
//!
//! Like the form notification and the calendar RSVP mail, the framing text is
//! English-only for now; localizing server-generated mail is flagged for the
//! wave review (the web i18n catalogs do not reach this process).

use alo_store::{OrderNotification, SiteOrderLine, Store};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use time::format_description::well_known::Rfc2822;

use crate::mime::{Addr, encode_unstructured, format_addr};

/// How many orders one sweep round claims. A round that claims the full batch
/// is immediately followed by another in the same tick, so a backlog drains
/// fast without an unbounded single query.
const BATCH: i64 = 100;

/// Claims every order awaiting notification and delivers each to its site
/// owner's inbox. Returns the number delivered.
pub async fn run_due(store: &Store) -> usize {
    let mut delivered = 0;
    loop {
        let due = match store.claim_order_notifications(BATCH).await {
            Ok(due) => due,
            Err(error) => {
                tracing::warn!(%error, "order-notification sweep: claim failed");
                return delivered;
            }
        };
        let batch_len = due.len();
        for notification in due {
            let account =
                store.for_account(notification.tenant.clone(), notification.owner.clone());
            // Best-effort To: header — the notification is addressed to the
            // owner's inbox by the door itself; the header is display only.
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
                    tracing::warn!(%error, "order-notification sweep: delivery failed");
                }
            }
        }
        if batch_len < BATCH as usize {
            return delivered;
        }
    }
}

/// One notification as a complete RFC 5322 message. Every stored field crosses
/// into the message safely: the customer's name reaches headers only through
/// the RFC 2047 path (which strips CR/LF), the address was gated to contain no
/// whitespace or control characters, and the body travels base64-encoded — an
/// order cannot inject headers or structure.
fn build_notification(n: &OrderNotification, owner_email: Option<&str>) -> String {
    let domain = crate::sites::sites_domain();
    // A display identity on the site's own (sub)domain; nothing ever sends
    // outbound from it, and replies go to the customer instead.
    let from = format_addr(&Addr {
        name: Some(format!("{} orders", n.site_name)),
        email: format!("no-reply@{}.{domain}", n.site_subdomain),
    });
    let reply_to = format_addr(&Addr {
        name: Some(n.customer_name.clone()),
        email: n.customer_email.clone(),
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
        "New order from {} ({})",
        n.customer_name, n.site_name
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
        "{name} <{email}> ordered from \"{catalog}\" on {site}:\r\n\
         \r\n\
         {lines}\
         \r\n\
         Total: {total}{unpriced}\r\n\
         {phone}{note}\
         \r\n\
         Nothing has been paid — this is a request. Reply to this email to \
         confirm it with {name}.\r\n",
        name = n.customer_name,
        email = n.customer_email,
        catalog = n.catalog_name,
        site = n.site_name,
        lines = n.lines.iter().map(line_text).collect::<String>(),
        total = money(n.total_cents, &n.currency),
        unpriced = if n.lines.iter().any(|line| line.unit_price_cents.is_none()) {
            " (plus the items you price yourself)"
        } else {
            ""
        },
        phone = n
            .customer_phone
            .as_ref()
            .map(|phone| format!("Phone: {phone}\r\n"))
            .unwrap_or_default(),
        note = n
            .note
            .as_ref()
            .map(|note| format!("Note: {note}\r\n"))
            .unwrap_or_default(),
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
        id = n.order.as_str(),
        sub = n.site_subdomain,
    )
}

/// One ordered line as a plain-text row. An item published without a price is
/// said to have none rather than shown as free.
fn line_text(line: &SiteOrderLine) -> String {
    match line.line_total_cents {
        Some(_) => format!("  {} x {}\r\n", line.quantity, line.item_name),
        None => format!(
            "  {} x {} (price on request)\r\n",
            line.quantity, line.item_name
        ),
    }
}

/// Minor units written as a decimal amount with its ISO code, using the
/// currency's own exponent — the same arithmetic the published page used, in
/// integers throughout.
fn money(minor: i64, currency: &str) -> String {
    let exponent = alo_store::currency_exponent(currency);
    if exponent == 0 {
        return format!("{minor} {currency}");
    }
    let scale = 10_i64.pow(exponent);
    let whole = minor / scale;
    let fraction = (minor % scale).abs();
    format!(
        "{whole}.{fraction:0>width$} {currency}",
        width = exponent as usize
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use alo_store::{SiteOrderId, TenantId, UserId};
    use time::OffsetDateTime;

    fn notification() -> OrderNotification {
        OrderNotification {
            tenant: TenantId::new("t-1"),
            owner: UserId::new("u-1"),
            site_name: "Harbour Bakery".to_owned(),
            site_subdomain: "harbour".to_owned(),
            catalog_name: "Saturday bake".to_owned(),
            order: SiteOrderId::new("order-1"),
            customer_name: "Ada Lovelace".to_owned(),
            customer_email: "ada@example.test".to_owned(),
            customer_phone: Some("+32 2 555 01".to_owned()),
            note: Some("leave at the door".to_owned()),
            currency: "EUR".to_owned(),
            total_cents: 1_350,
            received_at: OffsetDateTime::UNIX_EPOCH,
            lines: vec![
                SiteOrderLine {
                    position: 0,
                    item_slug: "sourdough".to_owned(),
                    item_name: "Sourdough".to_owned(),
                    quantity: 3,
                    unit_price_cents: Some(450),
                    line_total_cents: Some(1_350),
                },
                SiteOrderLine {
                    position: 1,
                    item_slug: "wedding-cake".to_owned(),
                    item_name: "Wedding cake".to_owned(),
                    quantity: 1,
                    unit_price_cents: None,
                    line_total_cents: None,
                },
            ],
        }
    }

    fn decoded_body(raw: &str) -> String {
        let (_, body) = raw.split_once("\r\n\r\n").unwrap();
        let joined: String = body.split("\r\n").collect();
        String::from_utf8(B64.decode(joined.trim()).unwrap()).unwrap()
    }

    #[test]
    fn the_message_answers_the_customer_and_lists_what_was_ordered() {
        let raw = build_notification(&notification(), Some("owner@bakery.test"));
        assert!(
            raw.contains("Reply-To: \"Ada Lovelace\" <ada@example.test>")
                || raw.contains("Reply-To: Ada Lovelace <ada@example.test>"),
            "the reply goes to the customer: {raw}"
        );
        assert!(raw.contains("To: owner@bakery.test"), "{raw}");
        let body = decoded_body(&raw);
        assert!(body.contains("3 x Sourdough"), "{body}");
        assert!(
            body.contains("1 x Wedding cake (price on request)"),
            "{body}"
        );
        assert!(body.contains("Total: 13.50 EUR"), "{body}");
        assert!(body.contains("plus the items you price yourself"), "{body}");
        assert!(body.contains("Phone: +32 2 555 01"), "{body}");
        assert!(body.contains("Note: leave at the door"), "{body}");
        assert!(
            body.contains("Nothing has been paid"),
            "the owner is told what an order is: {body}"
        );
    }

    #[test]
    fn a_customer_cannot_inject_headers_through_their_own_name() {
        let mut hostile = notification();
        hostile.customer_name = "Ada\r\nBcc: victim@example.test".to_owned();
        let raw = build_notification(&hostile, None);
        let (headers, _) = raw.split_once("\r\n\r\n").unwrap();
        // The name survives as text inside Reply-To (folded to spaces by the
        // header encoder); what must never happen is a new header line.
        assert!(
            !headers
                .split("\r\n")
                .any(|line| line.to_ascii_lowercase().starts_with("bcc:")),
            "a header was injected: {headers}"
        );
    }

    #[test]
    fn currencies_without_minor_units_are_written_whole() {
        assert_eq!(money(1_350, "EUR"), "13.50 EUR");
        assert_eq!(money(1_200, "JPY"), "1200 JPY");
        assert_eq!(money(12_500, "KWD"), "12.500 KWD");
        assert_eq!(money(0, "EUR"), "0.00 EUR");
    }
}
