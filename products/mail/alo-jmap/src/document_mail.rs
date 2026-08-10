//! The **covering letter** a business document travels in (ADR 0035) — the
//! short email that carries an invoice to a customer (B1.18) or an order to a
//! supplier (B5.05a2), and the one place either is composed.
//!
//! It is the mail-side half of the party generalisation the printed document
//! made ([`crate::billing_print`]): the paper does not care whether the party
//! it names is a customer or a supplier, and neither does the letter it is
//! attached to. Both are written from a [`PrintDocument`] and nothing else, so
//! the subject line, the file name and the title on the page always say the
//! same thing, and a figure quoted in the email is the figure printed on the
//! document — never a second calculation.
//!
//! Three things are the server's and not the caller's, because a request must
//! not be able to choose where a business document goes:
//!
//! - **The recipient** is the party's own stored address ([`recipient`]).
//!   There is no `to` field on any route that uses this module.
//! - **The author** is the caller's own canonical address
//!   ([`crate::drafts::from_address`]).
//! - **The attachment** is rendered by the caller from the stored document —
//!   never uploaded, never referenced by a client-supplied id.
//!
//! And **nothing here sends**. [`save`] writes the message into the caller's
//! Drafts with the `$draft` keyword ([`crate::drafts`]) for a human to read,
//! change and send through the one submission path that signs, records and is
//! audited — ADR 0034's standing rule, applied to every letter this product
//! composes.

use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::billing_print::{DocumentKind, PrintDocument, Strings, amount, date, document_heading};
use crate::drafts;
use crate::error::Problem;
use crate::mime::{Addr, Attachment, Outgoing};
use crate::state::Account;

/// The words a covering email is written in.
///
/// Its own table rather than a corner of [`Strings`]: the document is a legal
/// artefact whose wording is fixed by what it is, and the email around it is a
/// note between two people. They are translated by different people, with
/// different latitude, so they are kept apart even though a request picks both
/// with one `?lang=`.
pub struct MailStrings {
    /// BCP 47 tag this table is written in.
    pub lang: &'static str,
    /// Subject line, given the document's heading and the issuer's name.
    pub subject: fn(&str, &str) -> String,
    /// Salutation, given the party's name.
    pub greeting: fn(&str) -> String,
    /// An invoice with a due date: heading, money, date.
    pub invoice_due: fn(&str, &str, &str) -> String,
    /// An invoice with no due date yet: heading, money, payment terms in days.
    pub invoice_terms: fn(&str, &str, i32) -> String,
    /// A credit note: heading, money, the number it corrects.
    pub credit_note: fn(&str, &str, &str) -> String,
    /// An order with an expected delivery day: heading, money, date.
    pub order_expected: fn(&str, &str, &str) -> String,
    /// Anything that states no date and corrects nothing: heading, money.
    pub document_plain: fn(&str, &str) -> String,
    /// The party's own reference, when the document carries one.
    pub reference: fn(&str) -> String,
    /// Our own reference, on a document we are the buyer of.
    pub own_reference: fn(&str) -> String,
    /// Sign-off, above the issuer's name.
    pub regards: &'static str,
}

/// The default table. Short on purpose: a covering note is read in a preview
/// pane, and everything a party needs to act on is on the attached document.
static EN: MailStrings = MailStrings {
    lang: "en",
    subject: |heading, issuer| format!("{heading} \u{2014} {issuer}"),
    greeting: |name| format!("Dear {name},"),
    invoice_due: |heading, money, due| {
        format!("Please find attached {heading} for {money}, payable by {due}.")
    },
    invoice_terms: |heading, money, days| {
        format!("Please find attached {heading} for {money}, payable within {days} days.")
    },
    credit_note: |heading, money, corrects| {
        format!("Please find attached {heading} for {money}, which corrects invoice {corrects}.")
    },
    order_expected: |heading, money, expected| {
        format!(
            "Please find attached {heading} for {money}. Please confirm it, and deliver by {expected}."
        )
    },
    document_plain: |heading, money| format!("Please find attached {heading} for {money}."),
    reference: |reference| format!("Your reference: {reference}"),
    own_reference: |reference| format!("Our reference: {reference}"),
    regards: "Kind regards,",
};

/// The French covering note (B1.27).
///
/// Every sentence opens with the document's own heading ("Facture
/// INV-2026-00001") rather than an article, because the heading's gender is
/// not something a format string can know — `la facture` and `l'avoir` would
/// otherwise need two sentences per case, and one of them would eventually be
/// wrong.
static FR: MailStrings = MailStrings {
    lang: "fr",
    subject: |heading, issuer| format!("{heading} \u{2014} {issuer}"),
    greeting: |name| format!("Bonjour {name},"),
    invoice_due: |heading, money, due| {
        format!("{heading} de {money}, à régler avant le {due}. Le document est en pièce jointe.")
    },
    invoice_terms: |heading, money, days| {
        format!(
            "{heading} de {money}, à régler sous {days} jours. Le document est en pièce jointe."
        )
    },
    credit_note: |heading, money, corrects| {
        format!(
            "{heading} de {money}, qui corrige la facture {corrects}. Le document est en pièce jointe."
        )
    },
    order_expected: |heading, money, expected| {
        format!(
            "{heading} de {money}. Merci de confirmer et de livrer avant le {expected}. Le document est en pièce jointe."
        )
    },
    document_plain: |heading, money| {
        format!("{heading} de {money}. Le document est en pièce jointe.")
    },
    reference: |reference| format!("Votre référence : {reference}"),
    own_reference: |reference| format!("Notre référence : {reference}"),
    regards: "Cordialement,",
};

/// The Dutch covering note (B1.27). Built the same way round as [`FR`], and for
/// the same reason: `de factuur` / `het document` is a gender the heading
/// carries and the sentence cannot.
static NL: MailStrings = MailStrings {
    lang: "nl",
    subject: |heading, issuer| format!("{heading} \u{2014} {issuer}"),
    greeting: |name| format!("Beste {name},"),
    invoice_due: |heading, money, due| {
        format!("{heading} van {money}, te voldoen vóór {due}. Het document vindt u in de bijlage.")
    },
    invoice_terms: |heading, money, days| {
        format!(
            "{heading} van {money}, te voldoen binnen {days} dagen. Het document vindt u in de bijlage."
        )
    },
    credit_note: |heading, money, corrects| {
        format!(
            "{heading} van {money}, die factuur {corrects} corrigeert. Het document vindt u in de bijlage."
        )
    },
    order_expected: |heading, money, expected| {
        format!(
            "{heading} van {money}. Graag bevestigen en leveren vóór {expected}. Het document vindt u in de bijlage."
        )
    },
    document_plain: |heading, money| {
        format!("{heading} van {money}. Het document vindt u in de bijlage.")
    },
    reference: |reference| format!("Uw referentie: {reference}"),
    own_reference: |reference| format!("Onze referentie: {reference}"),
    regards: "Met vriendelijke groet,",
};

/// The words for a language tag, falling back to the default table.
///
/// The same seam as [`crate::billing_print::strings_for`]: one `?lang=` picks
/// the document and the note it travels in, so a French invoice is never
/// introduced in English.
#[must_use]
pub fn mail_strings_for(tag: &str) -> &'static MailStrings {
    let primary = tag.split(['-', '_']).next().unwrap_or_default();
    match primary.to_ascii_lowercase().as_str() {
        "fr" => &FR,
        "nl" => &NL,
        _ => &EN,
    }
}

/// The address the document goes to: the party's own, or the `422` that says
/// why it cannot be sent.
///
/// The store validates the shape of an address when it is written
/// ([`alo_store::billing_customers`], [`alo_store::inv_suppliers`]); it is
/// checked again here, against the same rule submission uses, because this is
/// the point where the value becomes a header and a header is not a place to
/// trust a stored string.
///
/// Whom the refusal *names* is the document's own business
/// ([`DocumentKind::party_noun`]): telling a buyer to fill in "the customer's"
/// address for an order they placed with a supplier is a wrong instruction,
/// not a wrong word.
pub(crate) fn recipient(document: &PrintDocument<'_>) -> Result<String, Problem> {
    let noun = document.kind.party_noun();
    let address = document
        .party
        .email
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("this {noun} has no email address"),
            )
        })?;
    if !crate::submission::valid_addr(address) {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("this {noun}'s email address cannot be used as a recipient"),
        ));
    }
    Ok(address.to_owned())
}

/// The subject: what the document calls itself, and who it is from.
///
/// The heading is the document's own ([`document_heading`]), so the subject
/// line, the file name and the title on the page all say the same thing — a
/// customer searching their mail for "INV-2026-00001" finds it. A tenant that
/// has not saved its legal name yet gets the heading alone rather than a
/// dangling separator.
pub(crate) fn subject(
    document: &PrintDocument<'_>,
    strings: &Strings,
    words: &MailStrings,
) -> String {
    let heading = document_heading(document, strings);
    let issuer = document.issuer.legal_name.trim();
    if issuer.is_empty() {
        return heading;
    }
    (words.subject)(&heading, issuer)
}

/// The covering note.
///
/// Every figure in it is the document's own, formatted by the document's own
/// formatters, so the sentence in the email and the total on the page can never
/// disagree. Nothing is computed here.
pub(crate) fn body(document: &PrintDocument<'_>, strings: &Strings, words: &MailStrings) -> String {
    let heading = document_heading(document, strings);
    let money = format!(
        "{} {}",
        document.currency,
        amount(document.totals.gross_cents, strings)
    );
    let sentence = match document.kind {
        DocumentKind::CreditNote => match document.credits_number {
            Some(corrects) => (words.credit_note)(&heading, &money, corrects),
            None => (words.document_plain)(&heading, &money),
        },
        // A quote is never mailed by this machinery — sending one is a
        // lifecycle transition that touches no mail — so it is written as the
        // plain case rather than left to fall through to an invoice's payment
        // wording.
        DocumentKind::Quote => (words.document_plain)(&heading, &money),
        DocumentKind::Invoice => match (document.secondary_date, document.payment_terms_days) {
            (Some(due), _) => (words.invoice_due)(&heading, &money, &date(due)),
            (None, Some(days)) => (words.invoice_terms)(&heading, &money, days),
            (None, None) => (words.document_plain)(&heading, &money),
        },
        // An order asks for two things a bill does not: confirmation, and the
        // goods by a day. With no day agreed it asks for neither rather than
        // inventing one.
        DocumentKind::PurchaseOrder => match document.secondary_date {
            Some(expected) => (words.order_expected)(&heading, &money, &date(expected)),
            None => (words.document_plain)(&heading, &money),
        },
    };

    let mut lines = vec![
        (words.greeting)(document.party.name),
        String::new(),
        sentence,
    ];
    let reference = document.reference.trim();
    if !reference.is_empty() {
        lines.push(String::new());
        // The same stored field, read from whichever side of the table this
        // document is written from.
        let line = match document.kind {
            DocumentKind::PurchaseOrder => (words.own_reference)(reference),
            _ => (words.reference)(reference),
        };
        lines.push(line);
    }
    lines.push(String::new());
    lines.push(words.regards.to_owned());
    let issuer = document.issuer.legal_name.trim();
    if !issuer.is_empty() {
        lines.push(issuer.to_owned());
    }
    lines.join("\n")
}

/// A composed letter, ready to be written into Drafts.
pub(crate) struct Letter {
    /// The address it goes to — the party's stored one.
    to: String,
    /// The subject line, kept beside the message so the response can state it
    /// without re-parsing what was built.
    subject: String,
    /// The name of the attached file.
    file_name: String,
    /// Its size, for the response.
    size_bytes: usize,
    /// The message itself.
    outgoing: Outgoing,
}

/// Composes the covering letter for `document` with `file` attached.
///
/// `from` is the caller's own canonical address ([`drafts::from_address`]) and
/// the recipient is resolved from the document — so nothing a request carries
/// decides who this letter is from or where it goes.
///
/// # Errors
/// The `422` [`recipient`] gives when the party has no usable address.
pub(crate) fn compose(
    document: &PrintDocument<'_>,
    strings: &Strings,
    words: &MailStrings,
    from: String,
    file_name: String,
    file: Vec<u8>,
) -> Result<Letter, Problem> {
    let to = recipient(document)?;
    let subject = subject(document, strings, words);
    let body = body(document, strings, words);
    let size_bytes = file.len();
    let message_id_domain = crate::api::domain_of(&from);
    Ok(Letter {
        to: to.clone(),
        subject: subject.clone(),
        file_name: file_name.clone(),
        size_bytes,
        outgoing: Outgoing {
            from: Addr {
                name: None,
                email: from,
            },
            to: vec![Addr {
                name: Some(document.party.name.to_owned()).filter(|n| !n.trim().is_empty()),
                email: to,
            }],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject,
            in_reply_to: Vec::new(),
            references: Vec::new(),
            body_text: body,
            body_html: None,
            attachments: vec![Attachment {
                name: file_name,
                content_type: "application/pdf".to_owned(),
                bytes: file,
            }],
            message_id_domain,
            message_id_token: crate::api::new_message_token(),
        },
    })
}

/// Writes the letter into the caller's Drafts and describes what was written.
///
/// The one JSON shape every document-mail route answers with:
/// `{"id","to","subject","attachment":{"name","sizeBytes"}}` — so a client that
/// can show "a draft to them, with this file" for an invoice can show it for an
/// order without learning a second contract.
///
/// **Nothing is sent.** The draft carries `$draft`, and only the user's own
/// submission puts it on the wire.
///
/// # Errors
/// The `422` [`drafts::save`] gives when the mailbox cannot be opened or the
/// message cannot be stored.
pub(crate) async fn save(account: &Account, letter: &Letter) -> Result<Value, Problem> {
    let draft = drafts::save(account, &letter.outgoing).await?;
    Ok(json!({
        "id": draft.as_str(),
        "to": letter.to,
        "subject": letter.subject,
        "attachment": { "name": letter.file_name, "sizeBytes": letter.size_bytes },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    use alo_store::billing_settings::BillingSettings;
    use alo_store::billing_totals::{LineFigures, Totals, totals};
    use alo_store::{BillingCustomerId, BillingLineId, Customer, Line};
    use time::{Date, Month, OffsetDateTime};

    use crate::billing_print::{Party, strings_for};

    fn day(year: i32, month: u8, day: u8) -> Date {
        Date::from_calendar_date(year, Month::try_from(month).unwrap_or(Month::January), day)
            .unwrap_or(Date::MIN)
    }

    fn customer() -> Customer {
        Customer {
            id: BillingCustomerId::new("cus-1".to_owned()),
            name: "Kunde & Söhne GmbH".to_owned(),
            address_line1: "Hauptstraße 5".to_owned(),
            address_line2: String::new(),
            postal_code: "10115".to_owned(),
            city: "Berlin".to_owned(),
            country: "DE".to_owned(),
            vat_id: Some("DE811907980".to_owned()),
            email: Some("buchhaltung@kunde.test".to_owned()),
            payment_terms_days: 14,
            currency: "EUR".to_owned(),
            contact_id: None,
            archived_at: None,
            created_by: "u1".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn issuer() -> BillingSettings {
        BillingSettings {
            legal_name: "Alo Werkplaats B.V.".to_owned(),
            country: "NL".to_owned(),
            ..Default::default()
        }
    }

    fn lines() -> Vec<Line> {
        vec![Line {
            id: BillingLineId::new("l-1".to_owned()),
            line_order: 0,
            description: "Consulting".to_owned(),
            unit: "hour".to_owned(),
            qty_milli: 12_500,
            unit_price_cents: 12_000,
            vat_rate_bp: 2100,
        }]
    }

    fn figures(lines: &[Line]) -> Totals {
        totals(
            &lines
                .iter()
                .map(|l| LineFigures {
                    qty_milli: l.qty_milli,
                    unit_price_cents: l.unit_price_cents,
                    vat_rate_bp: l.vat_rate_bp,
                })
                .collect::<Vec<_>>(),
        )
    }

    fn invoice<'a>(
        customer: &'a Customer,
        issuer: &'a BillingSettings,
        lines: &'a [Line],
        totals: &'a Totals,
    ) -> PrintDocument<'a> {
        PrintDocument {
            kind: DocumentKind::Invoice,
            banner: None,
            number: Some("INV-2026-00001"),
            primary_date: Some(day(2026, 8, 7)),
            secondary_date: Some(day(2026, 8, 21)),
            reference: "PO-42",
            note: "Thank you.",
            currency: "EUR",
            payment_terms_days: Some(14),
            credits_number: None,
            party: Party::customer(customer),
            lines,
            totals,
            restated: None,
            issuer,
        }
    }

    #[test]
    fn the_recipient_is_the_partys_own_address_and_never_a_missing_one() {
        let (mut c, i) = (customer(), issuer());
        let l = lines();
        let t = figures(&l);
        assert_eq!(
            recipient(&invoice(&c, &i, &l, &t)).ok(),
            Some("buchhaltung@kunde.test".to_owned())
        );
        // Whitespace around a stored address is not a reason to fail.
        c.email = Some("  buchhaltung@kunde.test \n".to_owned());
        assert_eq!(
            recipient(&invoice(&c, &i, &l, &t)).ok(),
            Some("buchhaltung@kunde.test".to_owned())
        );
        for missing in [None, Some(String::new()), Some("   ".to_owned())] {
            c.email = missing.clone();
            let problem = recipient(&invoice(&c, &i, &l, &t))
                .err()
                .unwrap_or_else(|| panic!("{missing:?} is not an address"));
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(
                problem.detail.as_deref(),
                Some("this customer has no email address")
            );
        }
    }

    #[test]
    fn a_refusal_names_the_party_the_document_actually_has() {
        // The same missing address, on an order: a buyer is sent to the
        // supplier's screen, not the customer's.
        let (c, i) = (customer(), issuer());
        let l = lines();
        let t = figures(&l);
        let order = PrintDocument {
            kind: DocumentKind::PurchaseOrder,
            party: Party {
                email: None,
                ..Party::customer(&c)
            },
            ..invoice(&c, &i, &l, &t)
        };
        let problem = recipient(&order)
            .err()
            .unwrap_or_else(|| panic!("an order with no address must be refused"));
        assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            problem.detail.as_deref(),
            Some("this supplier has no email address")
        );
    }

    #[test]
    fn nothing_that_could_reach_an_envelope_survives_the_recipient_check() {
        // The store validates an address on write; this is the second gate, at
        // the point where the value becomes a header.
        let (mut c, i) = (customer(), issuer());
        let l = lines();
        let t = figures(&l);
        for bad in [
            "buchhaltung@kunde.test\r\nBcc: thief@evil.test",
            "<buchhaltung@kunde.test>",
            "buchhaltung kunde.test",
            "no-at-sign",
        ] {
            c.email = Some(bad.to_owned());
            let problem = recipient(&invoice(&c, &i, &l, &t))
                .err()
                .unwrap_or_else(|| panic!("{bad:?} must be refused"));
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[test]
    fn the_subject_is_what_the_document_calls_itself_and_who_it_is_from() {
        let (c, mut i) = (customer(), issuer());
        let l = lines();
        let t = figures(&l);
        let (s, w) = (strings_for("en"), mail_strings_for("en"));
        assert_eq!(
            subject(&invoice(&c, &i, &l, &t), s, w),
            "Invoice INV-2026-00001 \u{2014} Alo Werkplaats B.V."
        );
        // A tenant that has not saved its name yet gets no dangling separator.
        i.legal_name = String::new();
        assert_eq!(
            subject(&invoice(&c, &i, &l, &t), s, w),
            "Invoice INV-2026-00001"
        );
    }

    #[test]
    fn the_note_states_the_documents_own_total_and_due_date() {
        let (c, i) = (customer(), issuer());
        let l = lines();
        let t = figures(&l);
        let (s, w) = (strings_for("en"), mail_strings_for("en"));
        let text = body(&invoice(&c, &i, &l, &t), s, w);
        assert!(text.starts_with("Dear Kunde & Söhne GmbH,"), "{text}");
        // 12.5 × 120.00 = 1 500.00 net, 21% VAT → 1 815.00 gross, and the
        // figure in the sentence is the document's own formatter's.
        assert_eq!(t.gross_cents, 181_500);
        let money = format!("EUR {}", amount(t.gross_cents, s));
        assert!(text.contains(&money), "{text}");
        assert!(
            text.contains("Please find attached Invoice INV-2026-00001"),
            "{text}"
        );
        assert!(text.contains("payable by 2026-08-21"), "{text}");
        assert!(text.contains("Your reference: PO-42"), "{text}");
        assert!(
            text.ends_with("Kind regards,\nAlo Werkplaats B.V."),
            "{text}"
        );
    }

    #[test]
    fn a_document_with_no_due_date_states_its_terms_instead() {
        let (c, i) = (customer(), issuer());
        let l = lines();
        let t = figures(&l);
        let (s, w) = (strings_for("en"), mail_strings_for("en"));
        let text = body(
            &PrintDocument {
                secondary_date: None,
                ..invoice(&c, &i, &l, &t)
            },
            s,
            w,
        );
        assert!(text.contains("payable within 14 days"), "{text}");
        // …and one with neither says nothing about when, rather than guessing.
        let text = body(
            &PrintDocument {
                secondary_date: None,
                payment_terms_days: None,
                ..invoice(&c, &i, &l, &t)
            },
            s,
            w,
        );
        assert!(text.contains("Please find attached Invoice"), "{text}");
        assert!(!text.contains("payable"), "{text}");
    }

    #[test]
    fn a_credit_note_names_the_invoice_it_corrects() {
        let (c, i) = (customer(), issuer());
        let l = lines();
        let t = figures(&l);
        let (s, w) = (strings_for("en"), mail_strings_for("en"));
        let text = body(
            &PrintDocument {
                kind: DocumentKind::CreditNote,
                number: Some("INV-2026-00002"),
                credits_number: Some("INV-2026-00001"),
                ..invoice(&c, &i, &l, &t)
            },
            s,
            w,
        );
        assert!(
            text.contains("Please find attached Credit note INV-2026-00002"),
            "{text}"
        );
        assert!(
            text.contains("which corrects invoice INV-2026-00001"),
            "{text}"
        );
        assert!(!text.contains("payable by"), "a credit note owes nothing");
    }

    #[test]
    fn an_order_asks_for_confirmation_and_the_goods_and_says_the_reference_is_ours() {
        let (c, i) = (customer(), issuer());
        let l = lines();
        let t = figures(&l);
        let (s, w) = (strings_for("en"), mail_strings_for("en"));
        let order = PrintDocument {
            kind: DocumentKind::PurchaseOrder,
            number: Some("PO-2026-00001"),
            secondary_date: Some(day(2026, 8, 24)),
            reference: "Project Falkenstein",
            ..invoice(&c, &i, &l, &t)
        };
        let text = body(&order, s, w);
        assert!(
            text.contains("Please find attached Purchase order PO-2026-00001"),
            "{text}"
        );
        assert!(text.contains("deliver by 2026-08-24"), "{text}");
        assert!(text.contains("confirm it"), "{text}");
        // Ours, not theirs: the same stored field, the other side of the table.
        assert!(
            text.contains("Our reference: Project Falkenstein"),
            "{text}"
        );
        assert!(!text.contains("Your reference"), "{text}");
        assert!(!text.contains("payable"), "an order owes nobody anything");

        // With no expected day it asks for neither rather than inventing one.
        let text = body(
            &PrintDocument {
                secondary_date: None,
                ..order
            },
            s,
            w,
        );
        assert!(
            text.contains("Please find attached Purchase order PO-2026-00001 for EUR"),
            "{text}"
        );
        assert!(!text.contains("deliver by"), "{text}");
    }

    #[test]
    fn a_document_with_no_reference_leaves_the_line_out_entirely() {
        let (c, i) = (customer(), issuer());
        let l = lines();
        let t = figures(&l);
        let (s, w) = (strings_for("en"), mail_strings_for("en"));
        let text = body(
            &PrintDocument {
                reference: "  ",
                ..invoice(&c, &i, &l, &t)
            },
            s,
            w,
        );
        assert!(!text.contains("Your reference"), "{text}");
        // No blank paragraph is left where the reference would have been.
        assert!(!text.contains("\n\n\n"), "{text:?}");
    }

    #[test]
    fn the_note_and_the_document_it_carries_are_always_the_same_language() {
        // One `?lang=` picks both tables, so a French invoice can never arrive
        // under an English covering note.
        for tag in ["en", "EN", "en-GB", "", "zz", "fr", "fr-BE", "nl", "nl_BE"] {
            assert_eq!(
                mail_strings_for(tag).lang,
                strings_for(tag).lang,
                "{tag}: the note and the document disagree"
            );
        }
        for tag in ["fr", "FR", "fr-BE"] {
            assert_eq!(mail_strings_for(tag).lang, "fr", "{tag}");
        }
        for tag in ["nl", "NL", "nl-BE"] {
            assert_eq!(mail_strings_for(tag).lang, "nl", "{tag}");
        }
    }

    #[test]
    fn a_language_we_do_not_ship_still_writes_the_note() {
        for tag in ["en", "EN", "en-GB", "", "zz", "🙂"] {
            assert_eq!(mail_strings_for(tag).lang, "en", "{tag}");
        }
    }

    #[test]
    fn every_table_writes_every_sentence_it_is_given() {
        // A forgotten field is a compile error; a sentence that drops what it
        // was handed is not, and it would silently lose a due date or a
        // delivery day off a translated letter.
        for tag in ["en", "fr", "nl"] {
            let w = mail_strings_for(tag);
            assert!(
                (w.subject)("Invoice X", "Us").contains("Invoice X"),
                "{tag}"
            );
            assert!((w.greeting)("Kunde").contains("Kunde"), "{tag}");
            assert!(
                (w.invoice_due)("Invoice X", "EUR 1.00", "2026-08-21").contains("2026-08-21"),
                "{tag}"
            );
            assert!(
                (w.invoice_terms)("Invoice X", "EUR 1.00", 14).contains("14"),
                "{tag}"
            );
            assert!(
                (w.credit_note)("Credit note Y", "EUR 1.00", "INV-1").contains("INV-1"),
                "{tag}"
            );
            assert!(
                (w.order_expected)("Purchase order Z", "CHF 1.00", "2026-08-24")
                    .contains("2026-08-24"),
                "{tag}"
            );
            assert!(
                (w.document_plain)("Quote Q", "EUR 1.00").contains("Quote Q"),
                "{tag}"
            );
            assert!((w.reference)("R-1").contains("R-1"), "{tag}");
            assert!((w.own_reference)("R-1").contains("R-1"), "{tag}");
            assert!(!w.regards.trim().is_empty(), "{tag}");
        }
    }
}
