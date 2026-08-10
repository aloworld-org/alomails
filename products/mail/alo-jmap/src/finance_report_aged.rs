//! **Aged receivables and payables** over HTTP (alo Finance, ADR 0035, wave
//! B4.11c): `GET /finance/reports/aged?on&side=receivable|payable` and its
//! `.csv` twin.
//!
//! The gate, the date and the spreadsheet-safety rule are
//! [`crate::finance_reports`]'; what is here is this one report's two
//! representations of [`alo_store::AgedReport`] and nothing else.
//!
//! Four things this layer says out loud.
//!
//! - **The side is stated, never guessed.** `side` is required and is one of two
//!   words. Defaulting to "receivable" would put what we owe under a heading
//!   that says what we are owed the first time somebody mistyped the parameter,
//!   and the two are chased by different people. One route rather than two
//!   because the shape is identical — a party, its documents, five bands —
//!   and every row of both the JSON and the file says which side it is.
//! - **A document carries its own money and the books' money.** `openCents` is
//!   in the currency the document was raised in; `baseOpenCents` is the same
//!   amount in the accounting currency at the rate frozen on the document, and
//!   is `null` when it cannot be restated honestly. Only restated amounts are in
//!   the bands, and `unconvertedCount` says how many documents are therefore in
//!   none of them — a surface that finds it non-zero must say so rather than
//!   print the totals plain.
//! - **The bands are the store's, spelled once.** `current`, `d1_30`, `d31_60`,
//!   `d61_90`, `d90_plus` come from [`alo_store::AgedBucket::as_str`] and
//!   [`alo_store::AGED_BUCKETS`], so the wire, the file and a screen cannot each
//!   choose their own names or their own order.
//! - **The file is one table, not three.** A `document` row per open document, a
//!   `party` row per counterparty, one `total` row — which is what lets a
//!   bookkeeper filter it to the parties and get exactly the summary the screen
//!   shows, and sum a band column and get the figure under it.
//!
//! Personal data: a counterparty's name and their document numbers, which is
//! what an aged listing *is*. No addresses, no contacts, nothing about a person.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::{AGED_BUCKETS, AgedBuckets, AgedDocument, AgedParty, AgedReport, AgedSide};

use crate::billing::{iso_date, map_store_err};
use crate::billing_xml::amount;
use crate::csv;
use crate::error::Problem;
use crate::finance_reports::{day, reader, text};
use crate::state::AppState;

/// What an aged listing is asked for: the day it stands on, and which side of
/// the ledger.
#[derive(Deserialize)]
pub struct AgedQuery {
    #[serde(default)]
    on: Option<String>,
    #[serde(default)]
    side: Option<String>,
}

impl AgedQuery {
    /// The day and the side this query names, or the `422` that says which of
    /// the two is wrong.
    ///
    /// # Errors
    /// [`Problem`] with `422` when `on` is missing or malformed, or when `side`
    /// is missing or is not one of the two words.
    fn read(&self) -> Result<(time::Date, AgedSide), Problem> {
        let on = day("on", self.on.as_deref())?;
        let side = self
            .side
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                Problem::with(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "side is required: an ageing is of receivables or of payables, \
                     and they are different reports"
                        .to_owned(),
                )
            })?;
        let side = AgedSide::parse(side).ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "side must be 'receivable' or 'payable'".to_owned(),
            )
        })?;
        Ok((on, side))
    }
}

/// One open document as JSON.
fn document_json(document: &AgedDocument) -> Value {
    json!({
        "documentId": document.document_id,
        "number": document.number,
        "issueDate": iso_date(document.issue_date),
        "dueDate": iso_date(document.due_date),
        "daysOverdue": document.days_overdue,
        "bucket": document.bucket.as_str(),
        "currency": document.currency,
        "openCents": document.open_cents,
        "baseOpenCents": document.base_open_cents,
        "creditNote": document.is_credit_note,
    })
}

/// The five bands and their total, in the accounting currency.
fn buckets_json(buckets: &AgedBuckets) -> Value {
    json!({
        "currentCents": buckets.current_cents,
        "d1_30Cents": buckets.days_1_30_cents,
        "d31_60Cents": buckets.days_31_60_cents,
        "d61_90Cents": buckets.days_61_90_cents,
        "d90_plusCents": buckets.days_90_plus_cents,
        "totalCents": buckets.total_cents,
    })
}

/// One counterparty as JSON: their bands, and the documents behind them.
fn party_json(party: &AgedParty) -> Value {
    json!({
        "partyId": party.party_id,
        "name": party.name,
        "buckets": buckets_json(&party.buckets),
        "unconvertedCount": party.unconverted_count,
        "documents": party.documents.iter().map(document_json).collect::<Vec<_>>(),
    })
}

/// The whole report as JSON.
fn report_json(report: &AgedReport) -> Value {
    json!({
        "on": iso_date(report.on),
        "side": report.side.as_str(),
        "currency": report.currency,
        "parties": report.parties.iter().map(party_json).collect::<Vec<_>>(),
        "buckets": buckets_json(&report.buckets),
        "unconvertedCount": report.unconverted_count,
        "documentCount": report.document_count,
    })
}

/// The CSV column names — a contract, deliberately not translated.
const COLUMNS: [&str; 17] = [
    // Which kind of row you are reading — one table rather than three files:
    //   `document`  one open document
    //   `party`     one counterparty's bands
    //   `total`     every party added up
    "row",
    // The day the report stands on, and which side. Repeated on every row on
    // purpose: a row lifted into another sheet still says what it is.
    "on",
    "side",
    // The accounting currency the five band columns and `total` are in.
    "currency",
    "party",
    "documentNumber",
    // What the document itself says, which is not always the currency above.
    "documentCurrency",
    "documentAmount",
    "dueDate",
    "daysOverdue",
    // The bands, in the store's own order.
    "current",
    "d1_30",
    "d31_60",
    "d61_90",
    "d90_plus",
    "total",
    // How many documents of this row are in none of the bands because they
    // could not be restated: 1 or 0 on a document row, a count on the others.
    "unconverted",
];

/// A blank cell — used where a figure would be a lie rather than a zero: the
/// band columns of a document that stands in another band, and the money
/// columns of a document nobody could restate.
const BLANK: &str = "";

/// The whole report as one CSV table: each party's documents, then that party's
/// bands, and one total row at the end.
fn report_csv(report: &AgedReport) -> String {
    let on = iso_date(report.on);
    let side = report.side.as_str();
    let mut out = csv::row(&COLUMNS);
    let mut write = |kind: &str,
                     party: &str,
                     number: &str,
                     document_currency: &str,
                     document_amount: &str,
                     due_date: &str,
                     days_overdue: &str,
                     bands: [String; 5],
                     total: &str,
                     unconverted: i64| {
        out.push_str(&csv::row(&[
            kind,
            &on,
            side,
            &report.currency,
            party,
            number,
            document_currency,
            document_amount,
            due_date,
            days_overdue,
            &bands[0],
            &bands[1],
            &bands[2],
            &bands[3],
            &bands[4],
            total,
            &unconverted.to_string(),
        ]));
    };

    for party in &report.parties {
        let name = text(&party.name);
        for document in &party.documents {
            // The open amount stands in exactly one band, and in none at all
            // when it could not be restated — which is what makes a band column
            // add up to the party row under it.
            let bands = AGED_BUCKETS.map(|bucket| match document.base_open_cents {
                Some(cents) if bucket == document.bucket => amount(cents),
                _ => BLANK.to_owned(),
            });
            write(
                "document",
                &name,
                &text(&document.number),
                &document.currency,
                &amount(document.open_cents),
                &iso_date(document.due_date),
                &document.days_overdue.to_string(),
                bands,
                &document
                    .base_open_cents
                    .map_or_else(|| BLANK.to_owned(), amount),
                i64::from(document.base_open_cents.is_none()),
            );
        }
        write(
            "party",
            &name,
            BLANK,
            BLANK,
            BLANK,
            BLANK,
            BLANK,
            AGED_BUCKETS.map(|bucket| amount(party.buckets.of(bucket))),
            &amount(party.buckets.total_cents),
            party.unconverted_count,
        );
    }
    write(
        "total",
        BLANK,
        BLANK,
        BLANK,
        BLANK,
        BLANK,
        BLANK,
        AGED_BUCKETS.map(|bucket| amount(report.buckets.of(bucket))),
        &amount(report.buckets.total_cents),
        report.unconverted_count,
    );
    out
}

/// The file name a saved listing lands under: which side, and the day it stands
/// on, in ASCII, so nothing has to be escaped in the header or on a file system.
fn file_name(report: &AgedReport) -> String {
    format!("aged-{}-{}.csv", report.side.as_str(), iso_date(report.on))
}

/// Reads the report behind both routes — one gate, one store call, so the file
/// an accountant opens and the table on the screen cannot disagree.
async fn read(
    state: &AppState,
    headers: &HeaderMap,
    query: &AgedQuery,
) -> Result<AgedReport, Problem> {
    let account = reader(state, headers).await?;
    let (on, side) = query.read()?;
    account.acc.fin_aged(on, side).await.map_err(map_store_err)
}

/// `GET /finance/reports/aged?on&side` → `{"report":{…}}` — who owes us or whom
/// we owe, for how long, on one day.
///
/// Every band figure is in the tenant's accounting currency, which the report
/// states; every document also carries what it says in its own.
///
/// # Errors
/// `401` without a valid bearer token; `403` for a member who is neither an
/// admin nor an accountant; `422` when `on` or `side` is missing or malformed;
/// `500` on a store failure.
pub async fn aged_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AgedQuery>,
) -> Result<Json<Value>, Problem> {
    let report = read(&state, &headers, &query).await?;
    Ok(Json(json!({ "report": report_json(&report) })))
}

/// `GET /finance/reports/aged.csv?on&side` → the same listing as a CSV file.
///
/// # Errors
/// As [`aged_report`].
pub async fn aged_report_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AgedQuery>,
) -> Result<Response, Problem> {
    let report = read(&state, &headers, &query).await?;
    Ok(csv::attachment(report_csv(&report), &file_name(&report)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alo_store::AgedBucket;
    use time::{Date, Month};

    fn on(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or_else(|e| panic!("{e}"))
    }

    fn document(number: &str, bucket: AgedBucket, open_cents: i64, days: i64) -> AgedDocument {
        AgedDocument {
            document_id: format!("doc-{number}"),
            number: number.to_owned(),
            issue_date: on(2026, Month::June, 1),
            due_date: on(2026, Month::June, 15),
            days_overdue: days,
            bucket,
            currency: "EUR".to_owned(),
            open_cents,
            base_open_cents: Some(open_cents),
            is_credit_note: false,
        }
    }

    /// The five bands, in the store's own order, with their total.
    fn buckets(bands: [i64; 5]) -> AgedBuckets {
        AgedBuckets {
            current_cents: bands[0],
            days_1_30_cents: bands[1],
            days_31_60_cents: bands[2],
            days_61_90_cents: bands[3],
            days_90_plus_cents: bands[4],
            total_cents: bands.iter().sum(),
        }
    }

    /// Two customers, one of them with a document nobody could restate.
    fn listing() -> AgedReport {
        AgedReport {
            on: on(2026, Month::August, 10),
            side: AgedSide::Receivable,
            currency: "EUR".to_owned(),
            parties: vec![
                AgedParty {
                    party_id: "cus-1".to_owned(),
                    name: "Anchor BV".to_owned(),
                    buckets: buckets([12_100, 24_200, 0, 0, 0]),
                    unconverted_count: 0,
                    documents: vec![
                        document("INV-2026-00001", AgedBucket::Current, 12_100, 0),
                        document("INV-2026-00002", AgedBucket::Days1To30, 24_200, 16),
                    ],
                },
                AgedParty {
                    party_id: "cus-2".to_owned(),
                    name: "Zephyr NV".to_owned(),
                    buckets: buckets([0, 0, 0, 21_000, 0]),
                    unconverted_count: 1,
                    documents: vec![
                        document("INV-2026-00003", AgedBucket::Days61To90, 21_000, 65),
                        AgedDocument {
                            currency: "USD".to_owned(),
                            base_open_cents: None,
                            ..document("INV-2026-00004", AgedBucket::Days90Plus, 50_000, 120)
                        },
                    ],
                },
            ],
            buckets: buckets([12_100, 24_200, 0, 21_000, 0]),
            unconverted_count: 1,
            document_count: 4,
        }
    }

    fn query(on: Option<&str>, side: Option<&str>) -> AgedQuery {
        AgedQuery {
            on: on.map(str::to_owned),
            side: side.map(str::to_owned),
        }
    }

    #[test]
    fn the_json_states_the_day_the_side_and_every_figure_in_cents() {
        let value = report_json(&listing());
        assert_eq!(value["on"], "2026-08-10");
        assert_eq!(value["side"], "receivable");
        assert_eq!(value["currency"], "EUR");
        assert_eq!(value["documentCount"], 4);
        assert_eq!(value["unconvertedCount"], 1);
        assert_eq!(
            value["buckets"],
            json!({
                "currentCents": 12_100,
                "d1_30Cents": 24_200,
                "d31_60Cents": 0,
                "d61_90Cents": 21_000,
                "d90_plusCents": 0,
                "totalCents": 57_300,
            })
        );
        assert_eq!(value["parties"][0]["name"], "Anchor BV");
        assert_eq!(
            value["parties"][0]["documents"][1],
            json!({
                "documentId": "doc-INV-2026-00002",
                "number": "INV-2026-00002",
                "issueDate": "2026-06-01",
                "dueDate": "2026-06-15",
                "daysOverdue": 16,
                "bucket": "d1_30",
                "currency": "EUR",
                "openCents": 24_200,
                "baseOpenCents": 24_200,
                "creditNote": false,
            })
        );
    }

    #[test]
    fn a_document_nobody_could_restate_says_so_rather_than_showing_a_guess() {
        let value = report_json(&listing());
        let document = &value["parties"][1]["documents"][1];
        assert_eq!(document["currency"], "USD");
        assert_eq!(document["openCents"], 50_000);
        assert_eq!(document["baseOpenCents"], Value::Null);
        assert_eq!(value["parties"][1]["unconvertedCount"], 1);
        // …and it is in none of the bands: the total is never part invention.
        assert_eq!(value["parties"][1]["buckets"]["d90_plusCents"], 0);
    }

    #[test]
    fn the_csv_is_the_documents_their_parties_and_one_total() {
        let body = report_csv(&listing());
        let lines: Vec<&str> = body.split("\r\n").filter(|l| !l.is_empty()).collect();
        assert_eq!(
            lines[0],
            "row,on,side,currency,party,documentNumber,documentCurrency,documentAmount,\
             dueDate,daysOverdue,current,d1_30,d31_60,d61_90,d90_plus,total,unconverted"
        );
        assert_eq!(
            lines[1],
            "document,2026-08-10,receivable,EUR,Anchor BV,INV-2026-00001,EUR,121.00,\
             2026-06-15,0,121.00,,,,,121.00,0"
        );
        assert_eq!(
            lines[2],
            "document,2026-08-10,receivable,EUR,Anchor BV,INV-2026-00002,EUR,242.00,\
             2026-06-15,16,,242.00,,,,242.00,0"
        );
        assert_eq!(
            lines[3],
            "party,2026-08-10,receivable,EUR,Anchor BV,,,,,,121.00,242.00,0.00,0.00,0.00,363.00,0"
        );
        // The unrestatable document keeps its own money and holds no band.
        assert_eq!(
            lines[5],
            "document,2026-08-10,receivable,EUR,Zephyr NV,INV-2026-00004,USD,500.00,\
             2026-06-15,120,,,,,,,1"
        );
        assert_eq!(
            lines[6],
            "party,2026-08-10,receivable,EUR,Zephyr NV,,,,,,0.00,0.00,0.00,210.00,0.00,210.00,1"
        );
        assert_eq!(
            lines[7],
            "total,2026-08-10,receivable,EUR,,,,,,,121.00,242.00,0.00,210.00,0.00,573.00,1"
        );
        assert_eq!(lines.len(), 8, "no other rows: {body:?}");
        assert_eq!(file_name(&listing()), "aged-receivable-2026-08-10.csv");
    }

    #[test]
    fn an_empty_listing_is_still_a_file_with_its_total() {
        let empty = AgedReport {
            side: AgedSide::Payable,
            parties: Vec::new(),
            buckets: AgedBuckets::default(),
            unconverted_count: 0,
            document_count: 0,
            ..listing()
        };
        let body = report_csv(&empty);
        let lines: Vec<&str> = body.split("\r\n").filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 2, "the header and one row of zeroes");
        assert_eq!(
            lines[1],
            "total,2026-08-10,payable,EUR,,,,,,,0.00,0.00,0.00,0.00,0.00,0.00,0"
        );
        assert_eq!(file_name(&empty), "aged-payable-2026-08-10.csv");
    }

    #[test]
    fn a_hostile_name_cannot_become_a_formula_and_an_amount_stays_a_number() {
        let mut report = listing();
        report.parties[0].name = "=cmd|'/c calc'!A1".to_owned();
        report.parties[0].documents[0].number = "-77".to_owned();
        report.parties[0].documents[0].open_cents = -2_500;
        report.parties[0].documents[0].base_open_cents = Some(-2_500);
        let body = report_csv(&report);
        assert!(body.contains(",'=cmd|'/c calc'!A1,"), "{body}");
        assert!(body.contains(",'-77,EUR,-25.00,"), "{body}");
        assert!(body.contains(",-25.00,,,,,-25.00,0"), "{body}");
    }

    #[test]
    fn a_side_is_required_and_is_one_of_two_words() {
        for missing in [None, Some(""), Some("  ")] {
            let problem = query(Some("2026-08-10"), missing)
                .read()
                .err()
                .unwrap_or_else(|| panic!("{missing:?} should have been refused"));
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
            let detail = problem.detail.unwrap_or_default();
            assert!(detail.starts_with("side is required"), "{detail}");
        }
        for wrong in ["Receivable", "debtors", "both", "ar"] {
            let problem = query(Some("2026-08-10"), Some(wrong))
                .read()
                .err()
                .unwrap_or_else(|| panic!("{wrong:?} should have been refused"));
            assert_eq!(
                problem.detail.as_deref(),
                Some("side must be 'receivable' or 'payable'")
            );
        }
    }

    #[test]
    fn the_day_is_refused_in_the_same_words_as_every_other_report() {
        let problem = query(None, Some("receivable"))
            .read()
            .err()
            .unwrap_or_else(|| panic!("a missing day should have been refused"));
        assert_eq!(
            problem.detail.as_deref(),
            Some("on is required: a report is always for a stated period")
        );
        let problem = query(Some("10/08/2026"), Some("receivable"))
            .read()
            .err()
            .unwrap_or_else(|| panic!("a malformed day should have been refused"));
        assert_eq!(
            problem.detail.as_deref(),
            Some("on must be a date of the form YYYY-MM-DD")
        );
    }

    #[test]
    fn a_well_formed_query_is_read_as_a_day_and_a_side() {
        let (day, side) = query(Some(" 2026-08-10 "), Some(" payable "))
            .read()
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(day, on(2026, Month::August, 10));
        assert_eq!(side, AgedSide::Payable);
    }
}
