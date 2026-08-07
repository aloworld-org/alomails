//! Sending an invoice **to the customer** (alo Billing, ADR 0035, wave B1.18) —
//! the step between a document that exists and a document that has been put in
//! front of the person who owes it.
//!
//! `POST /billing/invoices/{id}/send` composes a short covering email to the
//! customer's own address, attaches the invoice PDF ([`crate::billing_pdf`]),
//! and **saves it as a draft** in the user's Drafts folder. It does not send.
//! That is the same rule the agent's draft tools follow (ADR 0034): anything
//! this product writes on a user's behalf lands where they can read it, change
//! it, and send it themselves through the ordinary submission path — which is
//! the one path that signs, records and is audited. A billing route that put
//! mail on the wire would be a second send path, drifting from the audited one,
//! for no gain a review step does not already give.
//!
//! Three things are the server's and not the caller's, each for the same
//! reason — a request must not be able to choose where an invoice goes:
//!
//! - **The recipient** is the customer's stored invoice address. There is no
//!   `to` field on this route; a document is sent to the party it names.
//! - **The author** is the caller's own canonical address
//!   ([`crate::drafts::from_address`]).
//! - **The attachment** is rendered here, now, from the stored document — never
//!   uploaded, never referenced by a client-supplied id.
//!
//! The one thing that *is* the caller's is `?lang=`, which picks the words of
//! both the covering note and the document, exactly as on `/print` and `/pdf`.
//!
//! **Not to be confused with `POST /billing/quotes/{id}/send`**, which is a
//! lifecycle transition (a quote becomes *sent*) and touches no mail. This
//! route writes a draft and changes nothing about the invoice.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use time::OffsetDateTime;

use alo_store::BillingInvoiceId;
use alo_store::billing_invoices::InvoiceStatus;

use crate::billing_invoices::printable;
use crate::billing_pdf as pdf;
use crate::billing_print::{
    DocumentKind, PrintDocument, PrintQuery, Strings, amount, date, document_heading,
};
use crate::drafts;
use crate::error::Problem;
use crate::mime::{Addr, Attachment, Outgoing};
use crate::state::{AppState, authenticate};

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
    /// Salutation, given the customer's name.
    pub greeting: fn(&str) -> String,
    /// An invoice with a due date: heading, money, date.
    pub invoice_due: fn(&str, &str, &str) -> String,
    /// An invoice with no due date yet: heading, money, payment terms in days.
    pub invoice_terms: fn(&str, &str, i32) -> String,
    /// A credit note: heading, money, the number it corrects.
    pub credit_note: fn(&str, &str, &str) -> String,
    /// Anything that states no date and corrects nothing: heading, money.
    pub document_plain: fn(&str, &str) -> String,
    /// The customer's own reference, when the document carries one.
    pub reference: fn(&str) -> String,
    /// Sign-off, above the issuer's name.
    pub regards: &'static str,
}

/// The default table. Short on purpose: a covering note is read in a preview
/// pane, and everything a customer needs to act on is on the attached document.
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
    document_plain: |heading, money| format!("Please find attached {heading} for {money}."),
    reference: |reference| format!("Your reference: {reference}"),
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
    document_plain: |heading, money| {
        format!("{heading} de {money}. Le document est en pièce jointe.")
    },
    reference: |reference| format!("Votre référence : {reference}"),
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
    document_plain: |heading, money| {
        format!("{heading} van {money}. Het document vindt u in de bijlage.")
    },
    reference: |reference| format!("Uw referentie: {reference}"),
    regards: "Met vriendelijke groet,",
};

/// The words for a language tag, falling back to the default table.
///
/// The same seam as [`crate::billing_print::strings_for`], moving with it at
/// the wave review (B1.27): one `?lang=` picks the document and the note it
/// travels in, so a French invoice is never introduced in English.
#[must_use]
pub fn mail_strings_for(tag: &str) -> &'static MailStrings {
    let primary = tag.split(['-', '_']).next().unwrap_or_default();
    match primary.to_ascii_lowercase().as_str() {
        "fr" => &FR,
        "nl" => &NL,
        _ => &EN,
    }
}

/// `POST /billing/invoices/{id}/send[?lang=]` →
/// `{"draft":{"id","to","subject","attachment":{"name","sizeBytes"}}}`.
///
/// Writes a draft covering email with the invoice PDF attached into the
/// caller's Drafts. **Nothing is sent**, and the invoice itself is not
/// modified: calling this twice writes two drafts and changes no billing
/// record, which is the behaviour a user who closed the compose window without
/// sending expects.
///
/// The refusals are the ones a document's own state dictates: a **draft**
/// invoice carries no number and prints a DRAFT banner, and a **void** one has
/// been cancelled — neither is a document to put in front of a customer, so
/// both are a `409` naming the state rather than a draft nobody should send.
pub async fn send_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<PrintQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let printable = printable(&account.acc, &BillingInvoiceId::new(id)).await?;
    sendable(printable.status())?;

    let document = printable.as_document();
    let strings = query.strings();
    let words = mail_strings_for(query.lang.as_deref().unwrap_or_default());

    // Both addresses are resolved before anything is rendered: a document with
    // nowhere to go should fail before we spend a PDF on it.
    let to = recipient(&document)?;
    let from = drafts::from_address(&account, &state).await?;

    let file = pdf::render(&document, strings, pdf::stamp(OffsetDateTime::now_utc()));
    let size_bytes = file.len();
    let file_name = pdf::file_name(&document, strings);
    let subject = subject(&document, strings, words);
    let body = body(&document, strings, words);

    let message_id_domain = crate::api::domain_of(&from);
    let outgoing = Outgoing {
        from: Addr {
            name: None,
            email: from,
        },
        to: vec![Addr {
            name: Some(document.customer.name.clone()).filter(|n| !n.trim().is_empty()),
            email: to.clone(),
        }],
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: subject.clone(),
        in_reply_to: Vec::new(),
        references: Vec::new(),
        body_text: body,
        body_html: None,
        attachments: vec![Attachment {
            name: file_name.clone(),
            content_type: "application/pdf".to_owned(),
            bytes: file,
        }],
        message_id_domain,
        message_id_token: crate::api::new_message_token(),
    };
    let draft = drafts::save(&account, &outgoing).await?;
    Ok(Json(json!({
        "draft": {
            "id": draft.as_str(),
            "to": to,
            "subject": subject,
            "attachment": { "name": file_name, "sizeBytes": size_bytes },
        }
    })))
}

/// Whether a document in this state is one to put in front of a customer.
///
/// Issued and paid both are: a paid invoice is legitimately re-sent as a copy
/// for the customer's own records. A draft and a void one are not, and the
/// refusal says which, because "409" alone leaves a user guessing whether to
/// issue the document or to raise a new one.
fn sendable(status: InvoiceStatus) -> Result<(), Problem> {
    match status {
        InvoiceStatus::Issued | InvoiceStatus::Paid => Ok(()),
        InvoiceStatus::Draft => Err(Problem::with(
            StatusCode::CONFLICT,
            "a draft is not sent to a customer — issue it first",
        )),
        InvoiceStatus::Void => Err(Problem::with(
            StatusCode::CONFLICT,
            "a void invoice is not sent to a customer",
        )),
    }
}

/// The address the document goes to: the customer's own, or the `422` that
/// says why it cannot be sent.
///
/// The store validates the shape of an invoice address when it is written
/// ([`alo_store::billing_customers`]); it is checked again here, against the
/// same rule submission uses, because this is the point where the value
/// becomes a header and a header is not a place to trust a stored string.
///
/// Shared with the reminder ([`crate::billing_reminder`]): both letters go to
/// the party the document names, and there must be exactly one rule for which
/// stored string may become an envelope.
pub(crate) fn recipient(document: &PrintDocument<'_>) -> Result<String, Problem> {
    let address = document
        .customer
        .email
        .as_deref()
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "this customer has no email address",
            )
        })?;
    if !crate::submission::valid_addr(address) {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "this customer's email address cannot be used as a recipient",
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
fn subject(document: &PrintDocument<'_>, strings: &Strings, words: &MailStrings) -> String {
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
fn body(document: &PrintDocument<'_>, strings: &Strings, words: &MailStrings) -> String {
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
        // A quote never reaches this route; it is written as the plain case
        // rather than left to fall through to an invoice's payment wording.
        DocumentKind::Quote => (words.document_plain)(&heading, &money),
        DocumentKind::Invoice => match (document.secondary_date, document.payment_terms_days) {
            (Some(due), _) => (words.invoice_due)(&heading, &money, &date(due)),
            (None, Some(days)) => (words.invoice_terms)(&heading, &money, days),
            (None, None) => (words.document_plain)(&heading, &money),
        },
    };

    let mut lines = vec![
        (words.greeting)(&document.customer.name),
        String::new(),
        sentence,
    ];
    let reference = document.reference.trim();
    if !reference.is_empty() {
        lines.push(String::new());
        lines.push((words.reference)(reference));
    }
    lines.push(String::new());
    lines.push(words.regards.to_owned());
    let issuer = document.issuer.legal_name.trim();
    if !issuer.is_empty() {
        lines.push(issuer.to_owned());
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    use alo_store::billing_settings::BillingSettings;
    use alo_store::billing_totals::{LineFigures, Totals, totals};
    use alo_store::{BillingCustomerId, BillingLineId, Customer, Line};
    use time::{Date, Month};

    use crate::billing_print::strings_for;

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
            customer,
            lines,
            totals,
            restated: None,
            issuer,
        }
    }

    #[test]
    fn only_a_document_a_customer_should_see_may_be_sent() {
        assert!(sendable(InvoiceStatus::Issued).is_ok());
        assert!(
            sendable(InvoiceStatus::Paid).is_ok(),
            "a copy is legitimate"
        );
        for (status, hint) in [
            (InvoiceStatus::Draft, "issue it first"),
            (InvoiceStatus::Void, "void"),
        ] {
            let problem = sendable(status)
                .err()
                .unwrap_or_else(|| panic!("{status:?} must not be sendable"));
            assert_eq!(problem.status, StatusCode::CONFLICT);
            assert!(
                problem.detail.as_deref().unwrap_or_default().contains(hint),
                "the refusal must say which state: {:?}",
                problem.detail
            );
        }
    }

    #[test]
    fn the_recipient_is_the_customers_own_address_and_never_a_missing_one() {
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
}
