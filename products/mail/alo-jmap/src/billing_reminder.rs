//! The payment reminder (alo Billing, ADR 0035, waves B1.25 and B1.26) — the
//! letter, and the route the overdue view chases one invoice from.
//!
//! Chasing money is the part of billing a small business does badly, and the
//! reason is never that the figures are hard: it is that writing the note is
//! awkward. So the note is written from the document itself and lands in the
//! user's **Drafts**, where they read it, change a word, and send it. Nothing
//! here puts mail on the wire, and nothing here changes the invoice — a
//! reminder is a letter about a document, not an event on it.
//!
//! Every figure in the letter is the document's own, formatted by the
//! document's own formatters ([`crate::billing_print`]), so the sentence in the
//! email and the total on the invoice can never disagree. The only arithmetic
//! is counting days between two dates.
//!
//! What it refuses to write is the useful part. A reminder is only ever about
//! **money someone owes us now**: a draft was never issued, a void invoice was
//! cancelled, a settled one is settled, and a credit note is money owed to the
//! *customer* — chasing any of them would be worse than writing nothing.
//!
//! Two doors reach the same letter. `POST /billing/invoices/{id}/reminder` is
//! the one a person clicks in the overdue view (B1.26); the agent's
//! `draft_payment_reminder` tool (B1.25, [`crate::agent_billing`]) resolves an
//! invoice *number* and then walks through here too. Neither may say who the
//! letter goes to, what it is worth, or how late it is — all three are read off
//! the stored document, so a reminder and the invoice it chases cannot disagree.

use serde::Deserialize;
use serde_json::{Value, json};
use time::Date;

use alo_store::BillingInvoiceId;
use alo_store::billing_invoices::InvoiceStatus;
use alo_store::billing_payments::Settlement;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};

use crate::billing_document::today;
use crate::billing_invoices::printable;
use crate::billing_print::{
    DocumentKind, PrintDocument, PrintQuery, Strings, amount, date, document_heading, strings_for,
};
use crate::document_mail::mail_strings_for;
use crate::drafts;
use crate::error::Problem;
use crate::mime::{Addr, Outgoing};
use crate::state::{Account, AppState, authenticate};

/// The longest extra sentence a caller may add to the letter.
///
/// A reminder is three sentences and a sign-off; anything longer is a different
/// email, and an unbounded string reaching a mail body from a model's proposal
/// is not a thing to leave unbounded.
pub const NOTE_MAX_CHARS: usize = 500;

/// What a caller may say about a reminder, which is one sentence and nothing
/// else.
///
/// Everything that matters about the letter — who it goes to, what it is worth,
/// how late it is, what has already been paid — is read off the stored document
/// rather than accepted from the request, so a body cannot chase the wrong
/// person for the wrong money. The whole body is optional: the ordinary click
/// sends none at all.
#[derive(Debug, Default, Deserialize)]
pub struct ReminderRequest {
    /// An extra paragraph, dropped in above the sign-off. Trimmed, and bounded
    /// by [`NOTE_MAX_CHARS`].
    #[serde(default)]
    pub note: Option<String>,
}

/// `POST /billing/invoices/{id}/reminder[?lang=]` →
/// `{"draft":{"id","invoice","to","subject","daysOverdue","outstandingCents"}}`.
///
/// The dunning click of the overdue view: write the reminder for this invoice
/// into the caller's own Drafts and answer what it says. **Nothing is sent**,
/// and the invoice is not touched — clicking twice writes two drafts and
/// changes no billing record, which is what a user who closed the compose
/// window without sending expects.
///
/// The optional JSON body carries a `note`; a request with no body at all is
/// the ordinary case and is not an error.
///
/// # Errors
/// `401` unauthenticated; `404` for an id that is not this tenant's; `409` for
/// a document that owes nothing (a draft, a void one, a settled one, a credit
/// note); `422` when the customer has no usable address or the note is too
/// long.
pub async fn remind_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<PrintQuery>,
    body: Option<Json<ReminderRequest>>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let note = body.and_then(|Json(request)| request.note);
    let reminder = draft_reminder(
        &account,
        &state,
        &BillingInvoiceId::new(id),
        query.lang.as_deref().unwrap_or_default(),
        note.as_deref(),
        today(),
    )
    .await?;
    Ok(Json(json!({
        "draft": {
            "id": reminder.message_id,
            "invoice": reminder.number,
            "to": reminder.to,
            "subject": reminder.subject,
            "daysOverdue": reminder.days_overdue,
            "outstandingCents": reminder.outstanding_cents,
        }
    })))
}

/// The words of a reminder.
///
/// Its own table beside [`MailStrings`] (the covering note that carries an
/// invoice) because it is a different letter with a different job: one presents
/// a document, the other asks for money that is late. They are translated
/// together at B1.27, and both are picked by the same `?lang=`.
pub struct ReminderStrings {
    /// BCP 47 tag this table is written in.
    pub lang: &'static str,
    /// Subject line, given the document's heading.
    pub subject: fn(&str) -> String,
    /// Subject line when the issuer has a name to sign with: heading, issuer.
    pub subject_from: fn(&str, &str) -> String,
    /// The document is past its due date: heading, money, due date, days late.
    pub overdue: fn(&str, &str, &str, i64) -> String,
    /// The document is not late yet: heading, money, due date.
    pub upcoming: fn(&str, &str, &str) -> String,
    /// Part of it has already arrived: money received, money outstanding.
    pub part_paid: fn(&str, &str) -> String,
    /// The customer's own reference, when the document carries one.
    pub reference: fn(&str) -> String,
    /// The line that keeps a reminder polite when the money crossed in the post.
    pub crossed_in_the_post: &'static str,
    /// Sign-off, above the issuer's name.
    pub regards: &'static str,
}

/// The default table.
static EN: ReminderStrings = ReminderStrings {
    lang: "en",
    subject: |heading| format!("Reminder: {heading}"),
    subject_from: |heading, issuer| format!("Reminder: {heading} \u{2014} {issuer}"),
    overdue: |heading, money, due, days| {
        let day = if days == 1 { "day" } else { "days" };
        format!("{heading} for {money} was payable by {due} and is now {days} {day} overdue.")
    },
    upcoming: |heading, money, due| format!("{heading} for {money} is payable by {due}."),
    part_paid: |received, outstanding| {
        format!("{received} has been received against it, leaving {outstanding} outstanding.")
    },
    reference: |reference| format!("Your reference: {reference}"),
    crossed_in_the_post: "If you have already sent the payment, please accept our thanks and ignore this message.",
    regards: "Kind regards,",
};

/// The French reminder (B1.27).
///
/// A first reminder is a courtesy, not a threat: the wording states the facts
/// and thanks a customer who has already paid. Nothing here mentions interest,
/// recovery costs or a deadline — those belong to a formal *mise en demeure*,
/// which is a decision a person takes, not a template.
static FR: ReminderStrings = ReminderStrings {
    lang: "fr",
    subject: |heading| format!("Rappel : {heading}"),
    subject_from: |heading, issuer| format!("Rappel : {heading} \u{2014} {issuer}"),
    overdue: |heading, money, due, days| {
        let day = if days == 1 { "jour" } else { "jours" };
        format!(
            "{heading} de {money} était à régler avant le {due} et accuse désormais {days} {day} de retard."
        )
    },
    upcoming: |heading, money, due| format!("{heading} de {money} est à régler avant le {due}."),
    part_paid: |received, outstanding| {
        format!("{received} ont déjà été reçus, laissant {outstanding} à régler.")
    },
    reference: |reference| format!("Votre référence : {reference}"),
    crossed_in_the_post: "Si vous avez déjà effectué le règlement, nous vous en remercions et vous prions de ne pas tenir compte de ce message.",
    regards: "Cordialement,",
};

/// The Dutch reminder (B1.27), written with the same restraint as [`FR`].
static NL: ReminderStrings = ReminderStrings {
    lang: "nl",
    subject: |heading| format!("Herinnering: {heading}"),
    subject_from: |heading, issuer| format!("Herinnering: {heading} \u{2014} {issuer}"),
    overdue: |heading, money, due, days| {
        let day = if days == 1 { "dag" } else { "dagen" };
        format!(
            "{heading} van {money} moest vóór {due} zijn voldaan en is nu {days} {day} over de vervaldatum."
        )
    },
    upcoming: |heading, money, due| {
        format!("{heading} van {money} dient vóór {due} te zijn voldaan.")
    },
    part_paid: |received, outstanding| {
        format!("Hiervan is {received} ontvangen, zodat {outstanding} openstaat.")
    },
    reference: |reference| format!("Uw referentie: {reference}"),
    crossed_in_the_post: "Hebt u de betaling al gedaan, dan danken wij u daarvoor en kunt u dit bericht als niet verzonden beschouwen.",
    regards: "Met vriendelijke groet,",
};

/// The words for a language tag, falling back to the default table — the same
/// seam as [`crate::billing_print::strings_for`], moved at the same moment
/// (B1.27), so one `?lang=` picks the document, the covering note and the
/// reminder together.
#[must_use]
pub fn reminder_strings_for(tag: &str) -> &'static ReminderStrings {
    let primary = tag.split(['-', '_']).next().unwrap_or_default();
    match primary.to_ascii_lowercase().as_str() {
        "fr" => &FR,
        "nl" => &NL,
        _ => &EN,
    }
}

/// What writing a reminder produced: the draft in the user's Drafts, and the
/// facts a caller reports back without having to re-read the document.
#[derive(Debug, Clone)]
pub struct ReminderDraft {
    /// The saved draft's message id.
    pub message_id: String,
    /// The document the letter is about.
    pub number: String,
    /// Who it is addressed to.
    pub to: String,
    /// The subject line.
    pub subject: String,
    /// How many days past its due date the document is; `0` when it is not
    /// late yet.
    pub days_overdue: i64,
    /// What is still owed, in cents.
    pub outstanding_cents: i64,
}

/// Write a reminder about one of the tenant's invoices into the caller's
/// Drafts, and answer what it says.
///
/// The whole path: read the document through the account door (a foreign id is
/// the `404` it is everywhere else), refuse the states that owe nothing, resolve
/// both addresses, compose, save. **Nothing is sent and the invoice is not
/// touched** — calling it twice writes two drafts and changes no billing record.
///
/// `note` is an optional extra sentence from the caller, trimmed and bounded
/// ([`NOTE_MAX_CHARS`]); `today` is the server's own date, never a caller's.
///
/// # Errors
/// `404` for an id that is not this tenant's; `409` for a document that owes
/// nothing (draft, void, settled, or a credit note); `422` when the customer
/// has no usable address or the note is too long.
pub async fn draft_reminder(
    account: &Account,
    state: &AppState,
    id: &BillingInvoiceId,
    lang: &str,
    note: Option<&str>,
    today: Date,
) -> Result<ReminderDraft, Problem> {
    let printable = printable(&account.acc, id).await?;
    let settlement = printable.settlement();
    let document = printable.as_document();
    remindable(printable.status(), document.kind, &settlement)?;
    let note = bounded_note(note)?;

    let strings = strings_for(lang);
    let words = reminder_strings_for(lang);
    let to = crate::document_mail::recipient(&document)?;
    let from = drafts::from_address(account, state).await?;

    let days_overdue = days_overdue(document.secondary_date, today);
    let subject = subject(&document, strings, words);
    let body = body(
        &document,
        &settlement,
        days_overdue,
        note.as_deref(),
        strings,
        words,
    );

    let outgoing = Outgoing {
        from: Addr {
            name: None,
            email: from.clone(),
        },
        to: vec![Addr {
            name: Some(document.party.name.to_owned()).filter(|n| !n.trim().is_empty()),
            email: to.clone(),
        }],
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: subject.clone(),
        in_reply_to: Vec::new(),
        references: Vec::new(),
        body_text: body,
        body_html: None,
        attachments: Vec::new(),
        message_id_domain: crate::api::domain_of(&from),
        message_id_token: crate::api::new_message_token(),
    };
    let saved = drafts::save(account, &outgoing).await?;
    Ok(ReminderDraft {
        message_id: saved.as_str().to_owned(),
        number: document.number.unwrap_or_default().to_owned(),
        to,
        subject,
        days_overdue,
        outstanding_cents: settlement.outstanding_cents,
    })
}

/// Whether this document is one somebody still owes us money on.
///
/// The four refusals are each a `409` that names the state, because "409" alone
/// leaves a user guessing whether to issue the document, raise a new one, or
/// look at their bank statement again.
fn remindable(
    status: InvoiceStatus,
    kind: DocumentKind,
    settlement: &Settlement,
) -> Result<(), Problem> {
    if matches!(kind, DocumentKind::CreditNote) {
        return Err(Problem::with(
            StatusCode::CONFLICT,
            "a credit note is money owed to the customer; there is nothing to remind them of",
        ));
    }
    match status {
        InvoiceStatus::Draft => {
            return Err(Problem::with(
                StatusCode::CONFLICT,
                "a draft has not been issued, so nobody owes it yet — issue it first",
            ));
        }
        InvoiceStatus::Void => {
            return Err(Problem::with(
                StatusCode::CONFLICT,
                "a void invoice has been cancelled; there is nothing to remind about",
            ));
        }
        InvoiceStatus::Paid => {
            return Err(Problem::with(
                StatusCode::CONFLICT,
                "this invoice is settled; there is nothing to remind about",
            ));
        }
        InvoiceStatus::Issued => {}
    }
    if settlement.outstanding_cents <= 0 {
        return Err(Problem::with(
            StatusCode::CONFLICT,
            "nothing is outstanding on this invoice; there is nothing to remind about",
        ));
    }
    Ok(())
}

/// The caller's extra sentence, trimmed; `None` when it says nothing.
fn bounded_note(note: Option<&str>) -> Result<Option<String>, Problem> {
    let Some(note) = note.map(str::trim).filter(|n| !n.is_empty()) else {
        return Ok(None);
    };
    if note.chars().count() > NOTE_MAX_CHARS {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("the added note may be at most {NOTE_MAX_CHARS} characters"),
        ));
    }
    Ok(Some(note.to_owned()))
}

/// How many days past its due date the document is, as of `today`; `0` when it
/// is not late (or carries no due date, which an issued document never does).
///
/// Whole days, and never negative: "in 3 days" is not overdueness, and the
/// sentence a reader gets is chosen from this being positive or not.
fn days_overdue(due: Option<Date>, today: Date) -> i64 {
    due.map_or(0, |due| (today - due).whole_days().max(0))
}

/// The subject: what the document calls itself, marked as a reminder, and who
/// it is from.
fn subject(document: &PrintDocument<'_>, s: &Strings, words: &ReminderStrings) -> String {
    let heading = document_heading(document, s);
    let issuer = document.issuer.legal_name.trim();
    if issuer.is_empty() {
        return (words.subject)(&heading);
    }
    (words.subject_from)(&heading, issuer)
}

/// The letter.
///
/// One sentence about the document, one about what has already arrived (only
/// when something has), the caller's own sentence (only when they wrote one),
/// the polite escape hatch, and the sign-off. Nothing is computed here except
/// the choice between the late and the not-yet-late sentence.
fn body(
    document: &PrintDocument<'_>,
    settlement: &Settlement,
    days_overdue: i64,
    note: Option<&str>,
    s: &Strings,
    words: &ReminderStrings,
) -> String {
    let heading = document_heading(document, s);
    let money = |cents: i64| format!("{} {}", document.currency, amount(cents, s));
    let mail = mail_strings_for(s.lang);
    let sentence = match (document.secondary_date, days_overdue > 0) {
        (Some(due), true) => (words.overdue)(
            &heading,
            &money(settlement.gross_cents),
            &date(due),
            days_overdue,
        ),
        (Some(due), false) => {
            (words.upcoming)(&heading, &money(settlement.gross_cents), &date(due))
        }
        // An issued invoice always carries a due date; a document that somehow
        // has none is stated plainly rather than given an invented one.
        (None, _) => (mail.document_plain)(&heading, &money(settlement.gross_cents)),
    };

    let mut lines = vec![
        (mail.greeting)(document.party.name),
        String::new(),
        sentence,
    ];
    if settlement.paid_cents > 0 {
        lines.push(String::new());
        lines.push((words.part_paid)(
            &money(settlement.paid_cents),
            &money(settlement.outstanding_cents),
        ));
    }
    let reference = document.reference.trim();
    if !reference.is_empty() {
        lines.push(String::new());
        lines.push((words.reference)(reference));
    }
    if let Some(note) = note {
        lines.push(String::new());
        lines.push(note.to_owned());
    }
    lines.push(String::new());
    lines.push(words.crossed_in_the_post.to_owned());
    lines.push(String::new());
    lines.push(words.regards.to_owned());
    let issuer = document.issuer.legal_name.trim();
    if !issuer.is_empty() {
        lines.push(issuer.to_owned());
    }
    lines.join("\n")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use alo_store::billing_settings::BillingSettings;
    use alo_store::billing_totals::{LineFigures, Totals, totals};
    use alo_store::{BillingCustomerId, BillingLineId, Customer, Line};
    use time::{Month, OffsetDateTime};

    use crate::billing_print::Party;

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
            content: None,
        }
    }

    #[test]
    fn a_request_may_add_a_sentence_and_nothing_else() {
        let empty: ReminderRequest = serde_json::from_str("{}").expect("an empty body is the norm");
        assert_eq!(empty.note, None);
        let stated: ReminderRequest =
            serde_json::from_str(r#"{"note":"As agreed on the phone."}"#).expect("a note");
        assert_eq!(stated.note.as_deref(), Some("As agreed on the phone."));
        // Nothing about the money is a caller's to state. A body that tried to
        // name the recipient, the sum owed or the lateness is accepted as the
        // empty request it is — those three are read off the stored document,
        // so there is no field here for a request to reach them through.
        let hostile: ReminderRequest = serde_json::from_str(
            r#"{"to":"thief@evil.test","outstandingCents":1,"daysOverdue":99}"#,
        )
        .expect("unknown fields are not the request");
        assert_eq!(hostile.note, None);
    }

    #[test]
    fn only_money_somebody_still_owes_us_is_chased() {
        let owed = Settlement::of(181_500, 0);
        assert!(remindable(InvoiceStatus::Issued, DocumentKind::Invoice, &owed).is_ok());
        // Part paid is still owed — for the remainder.
        let part = Settlement::of(181_500, 50_000);
        assert!(remindable(InvoiceStatus::Issued, DocumentKind::Invoice, &part).is_ok());

        for (status, kind, settlement, hint) in [
            (
                InvoiceStatus::Draft,
                DocumentKind::Invoice,
                Settlement::of(181_500, 0),
                "issue it first",
            ),
            (
                InvoiceStatus::Void,
                DocumentKind::Invoice,
                Settlement::of(181_500, 0),
                "cancelled",
            ),
            (
                InvoiceStatus::Paid,
                DocumentKind::Invoice,
                Settlement::of(181_500, 181_500),
                "settled",
            ),
            (
                InvoiceStatus::Issued,
                DocumentKind::CreditNote,
                Settlement::of(-181_500, 0),
                "owed to the customer",
            ),
            (
                InvoiceStatus::Issued,
                DocumentKind::Invoice,
                Settlement::of(0, 0),
                "nothing is outstanding",
            ),
        ] {
            let problem = remindable(status, kind, &settlement)
                .err()
                .unwrap_or_else(|| panic!("{status:?}/{kind:?} must not be chased"));
            assert_eq!(problem.status, StatusCode::CONFLICT);
            assert!(
                problem.detail.as_deref().unwrap_or_default().contains(hint),
                "the refusal must say why: {:?}",
                problem.detail
            );
        }
    }

    #[test]
    fn lateness_is_whole_days_and_never_negative() {
        let due = day(2026, 8, 21);
        assert_eq!(days_overdue(Some(due), day(2026, 8, 21)), 0, "due today");
        assert_eq!(days_overdue(Some(due), day(2026, 8, 22)), 1);
        assert_eq!(days_overdue(Some(due), day(2026, 9, 4)), 14);
        assert_eq!(days_overdue(Some(due), day(2026, 8, 1)), 0, "not late yet");
        assert_eq!(days_overdue(None, day(2026, 8, 22)), 0);
    }

    #[test]
    fn the_letter_states_the_documents_own_total_and_how_late_it_is() {
        let (c, i) = (customer(), issuer());
        let l = lines();
        let t = figures(&l);
        let (s, w) = (strings_for("en"), reminder_strings_for("en"));
        let settlement = Settlement::of(t.gross_cents, 0);
        let text = body(&invoice(&c, &i, &l, &t), &settlement, 14, None, s, w);
        assert!(text.starts_with("Dear Kunde & Söhne GmbH,"), "{text}");
        assert_eq!(t.gross_cents, 181_500);
        assert!(
            text.contains(&format!("EUR {}", amount(181_500, s))),
            "{text}"
        );
        assert!(
            text.contains("was payable by 2026-08-21 and is now 14 days overdue"),
            "{text}"
        );
        assert!(text.contains("Your reference: PO-42"), "{text}");
        assert!(text.contains("please accept our thanks"), "{text}");
        assert!(
            text.ends_with("Kind regards,\nAlo Werkplaats B.V."),
            "{text}"
        );
        // Nothing has been paid, so nothing is said about part payment.
        assert!(!text.contains("has been received"), "{text}");
    }

    #[test]
    fn one_day_late_is_a_day_not_days() {
        let (c, i) = (customer(), issuer());
        let l = lines();
        let t = figures(&l);
        let (s, w) = (strings_for("en"), reminder_strings_for("en"));
        let text = body(
            &invoice(&c, &i, &l, &t),
            &Settlement::of(t.gross_cents, 0),
            1,
            None,
            s,
            w,
        );
        assert!(text.contains("is now 1 day overdue"), "{text}");
    }

    #[test]
    fn a_document_not_late_yet_is_asked_for_politely_without_a_count() {
        let (c, i) = (customer(), issuer());
        let l = lines();
        let t = figures(&l);
        let (s, w) = (strings_for("en"), reminder_strings_for("en"));
        let text = body(
            &invoice(&c, &i, &l, &t),
            &Settlement::of(t.gross_cents, 0),
            0,
            None,
            s,
            w,
        );
        assert!(text.contains("is payable by 2026-08-21."), "{text}");
        assert!(!text.contains("overdue"), "{text}");
    }

    #[test]
    fn a_part_paid_document_is_chased_for_the_remainder_only() {
        let (c, i) = (customer(), issuer());
        let l = lines();
        let t = figures(&l);
        let (s, w) = (strings_for("en"), reminder_strings_for("en"));
        let settlement = Settlement::of(t.gross_cents, 50_000);
        let text = body(&invoice(&c, &i, &l, &t), &settlement, 3, None, s, w);
        assert_eq!(settlement.outstanding_cents, 131_500);
        assert!(
            text.contains(&format!(
                "EUR {} has been received against it, leaving EUR {} outstanding.",
                amount(50_000, s),
                amount(131_500, s)
            )),
            "{text}"
        );
        // The document's own worth is still what the first sentence states.
        assert!(
            text.contains(&format!("for EUR {}", amount(181_500, s))),
            "{text}"
        );
    }

    #[test]
    fn the_callers_sentence_is_carried_verbatim_and_bounded() {
        let (c, i) = (customer(), issuer());
        let l = lines();
        let t = figures(&l);
        let (s, w) = (strings_for("en"), reminder_strings_for("en"));
        let text = body(
            &invoice(&c, &i, &l, &t),
            &Settlement::of(t.gross_cents, 0),
            2,
            Some("We can arrange payment in two instalments if that helps."),
            s,
            w,
        );
        assert!(
            text.contains("We can arrange payment in two instalments if that helps."),
            "{text}"
        );
        // Blank or whitespace-only adds no paragraph…
        assert_eq!(bounded_note(None).unwrap(), None);
        assert_eq!(bounded_note(Some("  \n ")).unwrap(), None);
        assert_eq!(bounded_note(Some("  hi  ")).unwrap(), Some("hi".to_owned()));
        // …and an essay is refused rather than mailed.
        let essay = "x".repeat(NOTE_MAX_CHARS + 1);
        let problem = bounded_note(Some(&essay)).expect_err("too long");
        assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            bounded_note(Some(&"é".repeat(NOTE_MAX_CHARS))).is_ok(),
            "chars, not bytes"
        );
    }

    #[test]
    fn the_subject_marks_it_a_reminder_and_names_the_document() {
        let (c, mut i) = (customer(), issuer());
        let l = lines();
        let t = figures(&l);
        let (s, w) = (strings_for("en"), reminder_strings_for("en"));
        assert_eq!(
            subject(&invoice(&c, &i, &l, &t), s, w),
            "Reminder: Invoice INV-2026-00001 \u{2014} Alo Werkplaats B.V."
        );
        i.legal_name = String::new();
        assert_eq!(
            subject(&invoice(&c, &i, &l, &t), s, w),
            "Reminder: Invoice INV-2026-00001"
        );
    }

    #[test]
    fn a_tag_picks_its_table_and_anything_else_falls_back() {
        for tag in ["en", "EN", "en-GB", "", "zz", "🙂"] {
            assert_eq!(reminder_strings_for(tag).lang, "en", "{tag}");
        }
        for tag in ["fr", "FR", "fr-BE", "fr_CH"] {
            assert_eq!(reminder_strings_for(tag).lang, "fr", "{tag}");
        }
        for tag in ["nl", "NL", "nl-BE", "nl_BE"] {
            assert_eq!(reminder_strings_for(tag).lang, "nl", "{tag}");
        }
    }

    #[test]
    fn every_table_says_how_late_a_document_is_in_its_own_plural() {
        // A reminder that says "1 days" reads as machinery, and the one that
        // says "0 jour" would be a reminder about something not yet late — so
        // the singular is pinned per language rather than left to the caller.
        for (tag, one, many) in [
            ("en", "1 day overdue", "14 days overdue"),
            ("fr", "1 jour de retard", "14 jours de retard"),
            ("nl", "1 dag over", "14 dagen over"),
        ] {
            let w = reminder_strings_for(tag);
            let single = (w.overdue)("Invoice INV-2026-00001", "EUR 1 815,00", "2026-07-24", 1);
            let plural = (w.overdue)("Invoice INV-2026-00001", "EUR 1 815,00", "2026-07-24", 14);
            assert!(single.contains(one), "{tag}: {single}");
            assert!(plural.contains(many), "{tag}: {plural}");
        }
    }

    #[test]
    fn no_reminder_threatens_anybody() {
        // The letter states facts and thanks a customer who has already paid.
        // Interest, recovery costs and formal notice are a human decision
        // (`docs/design/billing.md`), never a template — asserted so a later
        // "helpful" edit to one language has to face this test.
        for tag in ["en", "fr", "nl"] {
            let w = reminder_strings_for(tag);
            let letter = format!(
                "{} {} {} {}",
                (w.subject)("Invoice INV-2026-00001"),
                (w.overdue)("Invoice INV-2026-00001", "EUR 1 815,00", "2026-07-24", 14),
                w.crossed_in_the_post,
                w.regards,
            )
            .to_lowercase();
            for word in [
                "interest",
                "intérêt",
                "rente",
                "legal",
                "juridique",
                "incasso",
                "mise en demeure",
                "court",
                "tribunal",
                "deurwaarder",
            ] {
                assert!(!letter.contains(word), "{tag} reminder mentions {word}");
            }
        }
    }
}
