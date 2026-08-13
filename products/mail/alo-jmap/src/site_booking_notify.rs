//! Owner notification for new website appointments (ADR 0036, S2.13b2), the
//! sibling of [`crate::site_notify`] and [`crate::site_order_notify`].
//!
//! An appointment taken on a published site (stored by the public `alo-sites`
//! booking door) is delivered to the **site owner's own inbox** as an ordinary
//! message — by INTERNAL delivery through the account door, exactly like SMTP
//! local delivery. Nothing here sends outbound mail: the visitor's address
//! appears only as `Reply-To`, so confirming or moving an appointment is one
//! deliberate reply by the owner through their normal send path.
//!
//! Runs as a background sweep from `main.rs`:
//! [`alo_store::Store::claim_booking_notifications`] marks rows notified as it
//! claims them (at-most-once — see that module's doc), then each claimed
//! appointment becomes one RFC 5322 message saying who booked what, when, in
//! the clock they were offered it in, and what they answered. A delivery
//! failure is logged — never with addresses or content (Law 1) — and the sweep
//! moves on; the appointment is in the owner's Agenda calendar either way,
//! which is why losing a notification loses nothing.
//!
//! Like the form and order notifications, the framing text is English-only for
//! now; localizing server-generated mail is flagged for the wave review (the
//! web i18n catalogs do not reach this process).

use alo_store::{BookingAnswer, BookingNotification, Store, local_wall_clock};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc2822;

use crate::mime::{Addr, encode_unstructured, format_addr};

/// How many appointments one sweep round claims. A round that claims the full
/// batch is immediately followed by another in the same tick, so a backlog
/// drains fast without an unbounded single query.
const BATCH: i64 = 100;

/// Claims every appointment awaiting notification and delivers each to its site
/// owner's inbox. Returns the number delivered.
pub async fn run_due(store: &Store) -> usize {
    let mut delivered = 0;
    loop {
        let due = match store.claim_booking_notifications(BATCH).await {
            Ok(due) => due,
            Err(error) => {
                tracing::warn!(%error, "booking-notification sweep: claim failed");
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
                    tracing::warn!(%error, "booking-notification sweep: delivery failed");
                }
            }
        }
        if batch_len < BATCH as usize {
            return delivered;
        }
    }
}

/// One notification as a complete RFC 5322 message. Every stored field crosses
/// into the message safely: the visitor's name reaches headers only through the
/// RFC 2047 path (which strips CR/LF), the address was gated to contain no
/// whitespace or control characters, and the body travels base64-encoded — a
/// booking cannot inject headers or structure.
fn build_notification(n: &BookingNotification, owner_email: Option<&str>) -> String {
    let domain = crate::sites::sites_domain();
    // A display identity on the site's own (sub)domain; nothing ever sends
    // outbound from it, and replies go to the visitor instead.
    let from = format_addr(&Addr {
        name: Some(format!("{} bookings", n.site_name)),
        email: format!("no-reply@{}.{domain}", n.site_subdomain),
    });
    let reply_to = format_addr(&Addr {
        name: Some(n.visitor_name.clone()),
        email: n.visitor_email.clone(),
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
        "New booking: {} with {} ({})",
        n.booking_name, n.visitor_name, n.site_name
    ));
    let date = n
        .created_at
        .format(&Rfc2822)
        .unwrap_or_else(|_| String::new());
    let date = if date.is_empty() {
        String::new()
    } else {
        format!("Date: {date}\r\n")
    };
    let body = format!(
        "{name} <{email}> booked \"{service}\" on {site}.\r\n\
         \r\n\
         When: {when}\r\n\
         {answers}\
         \r\n\
         It is already in your calendar. Reply to this email to reach {name}.\r\n",
        name = n.visitor_name,
        email = n.visitor_email,
        service = n.booking_name,
        site = n.site_name,
        when = when_text(n),
        answers = n.answers.iter().map(answer_text).collect::<String>(),
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
        id = n.appointment.as_str(),
        sub = n.site_subdomain,
    )
}

/// The appointment written in the clock the visitor was offered it in — the
/// service's own zone, named, so an owner reading this on holiday is not
/// guessing which nine o'clock it is.
fn when_text(n: &BookingNotification) -> String {
    let day = local_day_text(n.starts_at, &n.time_zone);
    let start = local_clock(n.starts_at, &n.time_zone);
    let end = local_clock(n.ends_at, &n.time_zone);
    format!("{day} {start}–{end} ({})", n.time_zone)
}

/// One answer as a plain-text row, under the label the visitor actually read.
fn answer_text(answer: &BookingAnswer) -> String {
    format!("{}: {}\r\n", answer.label, answer.value)
}

/// An instant's local calendar day as `YYYY-MM-DD`, falling back to the UTC day
/// when the zone cannot be resolved — a notification never shows a blank date.
fn local_day_text(instant: OffsetDateTime, time_zone: &str) -> String {
    let day = local_wall_clock(instant, time_zone)
        .map(|(day, _)| day)
        .unwrap_or_else(|| instant.date());
    format!(
        "{:04}-{:02}-{:02}",
        day.year(),
        u8::from(day.month()),
        day.day()
    )
}

/// An instant's local wall clock as `HH:MM`, falling back to UTC for the same
/// reason as [`local_day_text`].
fn local_clock(instant: OffsetDateTime, time_zone: &str) -> String {
    match local_wall_clock(instant, time_zone) {
        Some((_, (hour, minute))) => format!("{hour:02}:{minute:02}"),
        None => format!("{:02}:{:02}", instant.hour(), instant.minute()),
    }
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
    use alo_store::{SiteBookingAppointmentId, TenantId, UserId};
    use time::{Date, Month};

    fn utc(day: u8, hour: u8, minute: u8) -> OffsetDateTime {
        Date::from_calendar_date(2026, Month::September, day)
            .unwrap()
            .with_hms(hour, minute, 0)
            .unwrap()
            .assume_utc()
    }

    fn notification() -> BookingNotification {
        BookingNotification {
            tenant: TenantId::new("t-1"),
            owner: UserId::new("u-1"),
            site_name: "Harbour Studio".to_owned(),
            site_subdomain: "harbour".to_owned(),
            appointment: SiteBookingAppointmentId::new("appt-1"),
            booking_name: "Consultation".to_owned(),
            visitor_name: "Ada Lovelace".to_owned(),
            visitor_email: "ada@example.test".to_owned(),
            // 09:00–09:30 in Brussels, which is 07:00–07:30 UTC in September.
            starts_at: utc(16, 7, 0),
            ends_at: utc(16, 7, 30),
            time_zone: "Europe/Brussels".to_owned(),
            answers: vec![BookingAnswer {
                key: "phone".to_owned(),
                label: "Phone".to_owned(),
                value: "+32 2 555 01".to_owned(),
            }],
            created_at: utc(1, 8, 0),
        }
    }

    fn decoded_body(raw: &str) -> String {
        let (_, body) = raw.split_once("\r\n\r\n").unwrap();
        let joined: String = body.split("\r\n").collect();
        String::from_utf8(B64.decode(joined.trim()).unwrap()).unwrap()
    }

    #[test]
    fn the_message_answers_the_visitor_and_says_what_was_booked_when() {
        let raw = build_notification(&notification(), Some("owner@studio.test"));
        assert!(
            raw.contains("Reply-To: \"Ada Lovelace\" <ada@example.test>")
                || raw.contains("Reply-To: Ada Lovelace <ada@example.test>"),
            "the reply goes to the visitor: {raw}"
        );
        assert!(raw.contains("To: owner@studio.test"), "{raw}");
        let body = decoded_body(&raw);
        assert!(body.contains("booked \"Consultation\""), "{body}");
        assert!(
            body.contains("When: 2026-09-16 09:00–09:30 (Europe/Brussels)"),
            "the owner reads the clock the visitor was offered: {body}"
        );
        assert!(body.contains("Phone: +32 2 555 01"), "{body}");
        assert!(
            body.contains("already in your calendar"),
            "the owner is told the appointment is not waiting on them: {body}"
        );
    }

    #[test]
    fn a_visitor_cannot_inject_headers_through_their_own_name() {
        let mut hostile = notification();
        hostile.visitor_name = "Ada\r\nBcc: victim@example.test".to_owned();
        let raw = build_notification(&hostile, None);
        let (headers, _) = raw.split_once("\r\n\r\n").unwrap();
        assert!(
            !headers
                .split("\r\n")
                .any(|line| line.to_ascii_lowercase().starts_with("bcc:")),
            "a header was injected: {headers}"
        );
    }

    #[test]
    fn an_unresolvable_zone_still_produces_a_time() {
        let mut broken = notification();
        broken.time_zone = "Mars/Olympus".to_owned();
        let body = decoded_body(&build_notification(&broken, None));
        assert!(
            body.contains("When: 2026-09-16 07:00–07:30 (Mars/Olympus)"),
            "the UTC clock is the fallback, never a blank: {body}"
        );
    }

    #[test]
    fn a_service_with_no_questions_asks_none() {
        let mut plain = notification();
        plain.answers.clear();
        let body = decoded_body(&build_notification(&plain, None));
        assert!(!body.contains("Phone"), "{body}");
        assert!(body.contains("When: "), "{body}");
    }
}
