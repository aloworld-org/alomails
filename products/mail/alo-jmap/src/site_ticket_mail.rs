//! The ticket email (ADR 0050, item S3.04h): the buyer of a paid ticket sale
//! gets their ticket in their inbox — the first mail alo sends a tenant's
//! stranger automatically, on the terms that ADR decided.
//!
//! The store half ([`alo_store::site_ticket_mail`]) claims at-most-once and
//! enforces the per-tenant daily ceiling; this module is the identity, the
//! words and the wire. The From is the **deployment's own transactional
//! address** — never a tenant value — with the site's name only as an
//! RFC 2047-encoded display name; the site owner's address appears only as
//! `Reply-To`, resolved through `for_tenant(sale.tenant)` so no other
//! tenant's owner can ever be resolved for a sale. The message leaves through
//! the same trusted internal submission listener as every other platform
//! mail (the signup-code path), which adds Date/Message-ID fixups and
//! DKIM-signs by the From domain.
//!
//! Unset `ALO_SITES_MAIL_FROM` and this sweep is never spawned: the feature
//! is default-off, and the off-switch is the same config. Nothing that
//! reaches a log carries a buyer's name or address (Law 1) — only ids and
//! coarse reasons.

use alo_store::{Store, TicketMailNotification};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc2822;

use crate::mime::{Addr, encode_unstructured, format_addr};

/// How many sales one claim round takes. Deliberately small: the store's
/// daily ceiling is only per-round precise, so the batch bounds the overshoot
/// (ADR 0050).
const BATCH: i64 = 25;

/// The per-tenant daily ceiling (ADR 0050): sales beyond it are deferred to
/// the next 24-hour window by the claim, never dropped.
const DAILY_CAP: i64 = 200;

/// The words of one language — the buyer reads the site's own language, the
/// same primary-subtag rule every sites surface resolves by.
struct Words {
    subject: &'static str,
    hello: &'static str,
    /// `{}` is the site's name.
    thanks: &'static str,
    seats: &'static str,
    paid: &'static str,
    link_line: &'static str,
    footer: &'static str,
    /// The decimal separator amounts are written with.
    decimal: char,
}

static EN_WORDS: Words = Words {
    subject: "Your ticket",
    hello: "Hello",
    thanks: "Thank you for your purchase at",
    seats: "Seats",
    paid: "Paid",
    link_line: "Your ticket and calendar file:",
    footer: "Show this ticket at the entrance. Questions? Just reply to this email.",
    decimal: '.',
};

static FR_WORDS: Words = Words {
    subject: "Votre billet",
    hello: "Bonjour",
    thanks: "Merci pour votre achat sur",
    seats: "Places",
    paid: "Payé",
    link_line: "Votre billet et le fichier calendrier :",
    footer: "Présentez ce billet à l'entrée. Une question ? Répondez simplement à cet e-mail.",
    decimal: ',',
};

static NL_WORDS: Words = Words {
    subject: "Je ticket",
    hello: "Hallo",
    thanks: "Bedankt voor je aankoop bij",
    seats: "Plaatsen",
    paid: "Betaald",
    link_line: "Je ticket en agendabestand:",
    footer: "Toon dit ticket bij de ingang. Vragen? Beantwoord gewoon deze e-mail.",
    decimal: ',',
};

fn words_for(tag: &str) -> &'static Words {
    let primary = tag.split(['-', '_']).next().unwrap_or_default();
    match primary.to_ascii_lowercase().as_str() {
        "fr" => &FR_WORDS,
        "nl" => &NL_WORDS,
        _ => &EN_WORDS,
    }
}

/// Claims every fulfilled, unmailed ticket sale and sends each buyer their
/// ticket through the trusted submission listener. Returns how many mails
/// left. Only called when `ALO_SITES_MAIL_FROM` is configured (`main.rs`
/// gates the spawn); the claim is the at-most-once, so a send that fails is
/// logged and not retried — the ticket stays reachable on the return and
/// order-status pages.
pub async fn run_due(store: &Store, submission_addr: &str, from_addr: &str) -> usize {
    let mut sent = 0;
    loop {
        let due = match store.claim_ticket_mails(BATCH, DAILY_CAP).await {
            Ok(due) => due,
            Err(error) => {
                tracing::warn!(%error, "ticket mail sweep: claim failed");
                return sent;
            }
        };
        let batch_len = due.len();
        for n in due {
            // Defense in depth: the checkout door gated the address shape,
            // but this string becomes an envelope recipient — refuse anything
            // that could not be one rather than hand it to the wire.
            if !addr_ok(&n.buyer_email) {
                tracing::warn!(
                    fulfilment = n.fulfilment.as_str(),
                    "ticket mail: implausible buyer address, skipped"
                );
                continue;
            }
            // The buyer's reply should reach the seller: the owner's address,
            // resolved inside the sale's own tenant — any other tenant's
            // owner is unreachable through this door by construction.
            let reply_to = store
                .for_tenant(n.tenant.clone())
                .email_of(&n.owner)
                .await
                .ok()
                .flatten();
            let raw = build_ticket_mail(
                &n,
                from_addr,
                reply_to.as_deref(),
                OffsetDateTime::now_utc(),
            );
            match crate::submission::submit(
                submission_addr,
                from_addr,
                std::slice::from_ref(&n.buyer_email),
                raw.as_bytes(),
            )
            .await
            {
                Ok(()) => sent += 1,
                Err(reason) => {
                    tracing::warn!(
                        %reason,
                        fulfilment = n.fulfilment.as_str(),
                        "ticket mail: send failed"
                    );
                }
            }
        }
        if batch_len < BATCH as usize {
            return sent;
        }
    }
}

/// A cheap plausibility gate on an address about to become an envelope
/// recipient: exactly one `@`, no whitespace or control bytes, a sane length.
fn addr_ok(email: &str) -> bool {
    !email.is_empty()
        && email.len() <= 320
        && email.chars().filter(|c| *c == '@').count() == 1
        && !email.starts_with('@')
        && !email.ends_with('@')
        && email.chars().all(|c| !c.is_whitespace() && !c.is_control())
}

/// One ticket email as a complete RFC 5322 message, in the site's language.
/// Every stored field crosses into the message safely: names and the sale's
/// description reach headers only through the RFC 2047 path (which strips
/// CR/LF), the addresses were gated, and the body travels base64-encoded —
/// no stored value can inject headers or structure. The From address is the
/// caller's platform identity; nothing of any tenant is ever an address on
/// the sending side.
fn build_ticket_mail(
    n: &TicketMailNotification,
    from_addr: &str,
    reply_to: Option<&str>,
    now: OffsetDateTime,
) -> String {
    let words = words_for(&n.default_locale);
    let domain = crate::sites::sites_domain();
    let from = format_addr(&Addr {
        name: Some(n.site_name.clone()),
        email: from_addr.to_owned(),
    });
    let reply_to = reply_to
        .map(|email| {
            format!(
                "Reply-To: {}\r\n",
                format_addr(&Addr {
                    name: Some(n.site_name.clone()),
                    email: email.to_owned(),
                })
            )
        })
        .unwrap_or_default();
    let to = format_addr(&Addr {
        name: Some(n.buyer_name.clone()),
        email: n.buyer_email.clone(),
    });
    let subject = encode_unstructured(&format!(
        "{}: {} ({})",
        words.subject, n.description, n.site_name
    ));
    let date = now.format(&Rfc2822).unwrap_or_else(|_| String::new());
    let date = if date.is_empty() {
        String::new()
    } else {
        format!("Date: {date}\r\n")
    };
    let url = format!("https://{}.{domain}/t/{}", n.site_subdomain, n.token);
    let body = format!(
        "{hello} {name},\r\n\
         \r\n\
         {thanks} {site}.\r\n\
         \r\n\
         {what}\r\n\
         {seats}: {quantity}\r\n\
         {paid}: {amount}\r\n\
         \r\n\
         {link_line}\r\n\
         {url}\r\n\
         \r\n\
         {footer}\r\n",
        hello = words.hello,
        name = n.buyer_name,
        thanks = words.thanks,
        site = n.site_name,
        what = n.description,
        seats = words.seats,
        quantity = n.quantity,
        paid = words.paid,
        amount = amount_text(n.amount_cents, &n.currency, words.decimal),
        link_line = words.link_line,
        footer = words.footer,
    );
    let body_b64 = wrap76(&B64.encode(body));
    format!(
        "From: {from}\r\n\
         {reply_to}\
         To: {to}\r\n\
         Subject: {subject}\r\n\
         {date}\
         Message-ID: <{id}@{sub}.{domain}>\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         Content-Transfer-Encoding: base64\r\n\
         \r\n\
         {body_b64}\r\n",
        id = n.fulfilment.as_str(),
        sub = n.site_subdomain,
    )
}

/// Integer cents as the buyer reads money: `170.00 EUR` in English,
/// `170,00 EUR` in French and Dutch. Cents are never negative here — the
/// order's amount is checked non-negative at the door.
fn amount_text(cents: i64, currency: &str, decimal: char) -> String {
    format!("{}{decimal}{:02} {currency}", cents / 100, cents % 100)
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
    use alo_store::{SiteId, SiteTicketFulfilmentId, TenantId, UserId};
    use time::{Date, Month};

    fn now() -> OffsetDateTime {
        Date::from_calendar_date(2026, Month::September, 1)
            .unwrap()
            .with_hms(8, 0, 0)
            .unwrap()
            .assume_utc()
    }

    fn notification() -> TicketMailNotification {
        TicketMailNotification {
            tenant: TenantId::new("t-1"),
            owner: UserId::new("u-1"),
            site: SiteId::new("site-1"),
            site_name: "Harbour Studio".to_owned(),
            site_subdomain: "harbour".to_owned(),
            default_locale: "en".to_owned(),
            fulfilment: SiteTicketFulfilmentId::new("ful-1"),
            token: "tok-abc123".to_owned(),
            description: "Letterpress workshop — 2026-09-16".to_owned(),
            buyer_name: "Maud Adams".to_owned(),
            buyer_email: "maud@example.test".to_owned(),
            quantity: 2,
            amount_cents: 17_000,
            currency: "EUR".to_owned(),
        }
    }

    fn decoded_body(raw: &str) -> String {
        let (_, body) = raw.split_once("\r\n\r\n").unwrap();
        let joined: String = body.split("\r\n").collect();
        String::from_utf8(B64.decode(joined.trim()).unwrap()).unwrap()
    }

    #[test]
    fn the_mail_carries_the_platform_from_and_the_owner_reply_to() {
        let raw = build_ticket_mail(
            &notification(),
            "tickets@alomails.test",
            Some("owner@studio.test"),
            now(),
        );
        let (headers, _) = raw.split_once("\r\n\r\n").unwrap();
        assert!(
            headers.contains("From: \"Harbour Studio\" <tickets@alomails.test>")
                || headers.contains("From: Harbour Studio <tickets@alomails.test>"),
            "the From address is the platform's, the display name the site's: {headers}"
        );
        assert!(
            headers.contains("<owner@studio.test>"),
            "the buyer's reply reaches the seller: {headers}"
        );
        assert!(headers.contains("<maud@example.test>"), "{headers}");
    }

    #[test]
    fn the_body_holds_the_sale_the_link_and_the_money() {
        let raw = build_ticket_mail(&notification(), "tickets@alomails.test", None, now());
        let body = decoded_body(&raw);
        assert!(body.contains("Letterpress workshop — 2026-09-16"), "{body}");
        assert!(body.contains("Seats: 2"), "{body}");
        assert!(body.contains("Paid: 170.00 EUR"), "{body}");
        assert!(
            body.contains("/t/tok-abc123"),
            "the ticket page link is the capability: {body}"
        );
        assert!(
            !raw.contains("Reply-To:"),
            "no owner address resolved, none written: {raw}"
        );
    }

    #[test]
    fn the_buyer_reads_the_sites_own_language() {
        let mut n = notification();
        n.default_locale = "fr-BE".to_owned();
        let body = decoded_body(&build_ticket_mail(&n, "t@a.test", None, now()));
        assert!(body.contains("Bonjour Maud Adams"), "{body}");
        assert!(
            body.contains("Payé : ") || body.contains("Payé: 170,00 EUR"),
            "{body}"
        );
        assert!(body.contains("170,00 EUR"), "{body}");
        n.default_locale = "NL".to_owned();
        let body = decoded_body(&build_ticket_mail(&n, "t@a.test", None, now()));
        assert!(body.contains("Bedankt voor je aankoop"), "{body}");
        n.default_locale = "de".to_owned();
        let body = decoded_body(&build_ticket_mail(&n, "t@a.test", None, now()));
        assert!(
            body.contains("Hello"),
            "unknown languages fall back to English: {body}"
        );
    }

    #[test]
    fn a_hostile_site_or_buyer_name_cannot_inject_headers() {
        let mut hostile = notification();
        hostile.site_name = "Studio\r\nBcc: victim@example.test".to_owned();
        hostile.buyer_name = "Maud\r\nX-Evil: yes".to_owned();
        hostile.description = "Show\r\nCc: victim@example.test".to_owned();
        let raw = build_ticket_mail(&hostile, "tickets@alomails.test", None, now());
        let (headers, _) = raw.split_once("\r\n\r\n").unwrap();
        assert!(
            !headers.split("\r\n").any(|line| {
                let l = line.to_ascii_lowercase();
                l.starts_with("bcc:") || l.starts_with("x-evil:") || l.starts_with("cc:")
            }),
            "a header was injected: {headers}"
        );
    }

    #[test]
    fn only_a_plausible_address_may_become_an_envelope_recipient() {
        assert!(addr_ok("maud@example.test"));
        assert!(!addr_ok(""));
        assert!(!addr_ok("no-at-sign"));
        assert!(!addr_ok("two@at@signs"));
        assert!(!addr_ok("@leading.test"));
        assert!(!addr_ok("trailing@"));
        assert!(!addr_ok("spaced out@example.test"));
        assert!(!addr_ok("ctrl\r\n@example.test"));
    }

    #[test]
    fn money_is_printed_from_cents_never_floats() {
        assert_eq!(amount_text(17_000, "EUR", '.'), "170.00 EUR");
        assert_eq!(amount_text(8_505, "EUR", ','), "85,05 EUR");
        assert_eq!(amount_text(5, "EUR", '.'), "0.05 EUR");
    }
}
