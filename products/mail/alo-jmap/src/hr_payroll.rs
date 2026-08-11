//! The payroll export, over HTTP (alo HR, ADR 0035, wave B6.10) — over
//! [`alo_store::hr_payroll_export`] and [`alo_store::hr_payroll_mapping`].
//!
//! # Why drawing a file is a POST
//!
//! Every other export in the suite is a `GET` with a `.csv` twin. This one is a
//! `POST` that files a row first, because **this particular read deserves a line
//! more than most writes do**: it returns every employee's pay, national
//! identifier and bank account in one response, and "who downloaded the payroll
//! file, and when" is a question a works council, a data-protection officer and
//! a fraud investigation all ask. The audit trail records mutations, so the draw
//! is a mutation — `hr.payroll_export.create` is filed by the same middleware
//! that files every other business write, and
//! [`alo_store::TenantStore::record_hr_payroll_export`] keeps the receipt beside
//! it (`docs/design/hr.md`, "The payroll export is a POST — the decision").
//!
//! The rejected alternative was a general read-audit for `/hr/*`, which would
//! file a line each time somebody opened their own leave balance: noise burying
//! the handful of lines that matter.
//!
//! # Three decisions this door makes
//!
//! **The whole surface is HR's** ([`crate::state::Account::require_hr`]) — pay,
//! IBANs and national identifiers, which is the one file in the product where
//! every private field a person has is on one row.
//!
//! **A mapping the caller did not state is their own country's.** The tenant's
//! billing identity already says which country they are in, and a German tenant
//! asking for their payroll file should not have to know the word `de`
//! (`docs/design/ux-principles.md`, recognition over recall). A country alo ships
//! no conventions for gets the neutral ISO sheet, never a guess.
//!
//! **Text is neutralised, figures are not.** Names and job titles are typed by
//! people and go through [`crate::finance_reports::text`]; amounts, hours and
//! dates go through untouched, because a negative amount begins with `-` and
//! must stay a number ([`crate::csv`]).

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Response;
use serde::Deserialize;
use serde_json::{Value, json};
use time::Date;

use alo_store::hr_payroll_export::PayrollLine;
use alo_store::hr_payroll_mapping::ColumnMapping;
use alo_store::{PAYROLL_MAPPINGS, payroll_mapping, payroll_mapping_for_country};

use crate::billing::{iso, iso_date, map_store_err, parse_body};
use crate::csv;
use crate::error::Problem;
use crate::finance_reports::{day, text};
use crate::state::{AppState, authenticate};

/// The body of a draw: two days and, optionally, the sheet to write.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DrawBody {
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
    /// The column mapping's key. Omitted means the tenant's own country's.
    #[serde(default)]
    mapping: Option<String>,
}

/// One mapping as JSON, for a picker: what it is called, whose conventions it
/// follows, and the columns it writes — machine names beside the headings, so a
/// screen can say "this file carries their IBAN" without parsing a heading in a
/// language it does not read.
fn mapping_json(mapping: &ColumnMapping) -> Value {
    json!({
        "key": mapping.key,
        "name": mapping.name,
        "country": mapping.country,
        "delimiter": mapping.delimiter.to_string(),
        "columns": mapping
            .columns
            .iter()
            .map(|(column, heading)| json!({
                "key": column.key(),
                "heading": heading,
                "private": column.is_private(),
            }))
            .collect::<Vec<_>>(),
    })
}

/// The file itself: the headings, then one record per person, under the
/// mapping's own delimiter.
fn file(mapping: &ColumnMapping, lines: &[PayrollLine], from: Date, to: Date) -> String {
    let mut out = csv::row_delimited(mapping.delimiter, &mapping.headings());
    for line in lines {
        let values: Vec<String> = mapping
            .row(line, from, to)
            .into_iter()
            .zip(mapping.columns.iter())
            .map(|(value, (column, _))| {
                if column.is_text() {
                    text(&value)
                } else {
                    value
                }
            })
            .collect();
        let fields: Vec<&str> = values.iter().map(String::as_str).collect();
        out.push_str(&csv::row_delimited(mapping.delimiter, &fields));
    }
    out
}

/// What a saved file lands under: what it is, the days it covers and the sheet
/// it is in, in ASCII — two mappings of one period do not overwrite each other
/// in a downloads folder.
fn file_name(mapping: &ColumnMapping, from: Date, to: Date) -> String {
    format!(
        "payroll-{}-to-{}-{}.csv",
        iso_date(from),
        iso_date(to),
        mapping.key
    )
}

/// `POST /hr/payroll-exports` `{from, to, mapping?}` → the period's CSV file —
/// **HR only**, and recorded.
///
/// The file carries what a payroll bureau needs and **no calculation**: no
/// gross-to-net, no tax, no contributions. Payroll calculation is a permanent
/// non-goal (ADR 0035).
///
/// # Errors
/// `401` without a valid bearer token; `403` for a member who is neither an
/// admin nor HR; `422` when a day is missing or malformed, the period ends
/// before it starts, is longer than a year, names a mapping this build does not
/// ship, or covers nobody who is paid — never an empty file, which would read as
/// "nobody is paid"; `500` on a store failure.
pub async fn draw_export(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Response, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let req: DrawBody = parse_body(&body)?;
    let (from, to) = (
        day("from", req.from.as_deref())?,
        day("to", req.to.as_deref())?,
    );
    let mapping = match req.mapping.as_deref() {
        Some(key) => payroll_mapping(key).map_err(map_store_err)?,
        None => payroll_mapping_for_country(
            &account
                .acc
                .billing_settings()
                .await
                .map_err(map_store_err)?
                .country,
        ),
    };
    let hr = state.store.for_tenant(account.tenant.clone());
    let lines = hr.hr_payroll_lines(from, to).await.map_err(map_store_err)?;
    // The receipt is written before the file leaves: a draw that reached the
    // caller and left no line is the one outcome this route exists to prevent.
    hr.record_hr_payroll_export(from, to, mapping.key, lines.len(), &account.user)
        .await
        .map_err(map_store_err)?;
    Ok(csv::attachment(
        file(mapping, &lines, from, to),
        &file_name(mapping, from, to),
    ))
}

/// `GET /hr/payroll-exports` → `{"exports":[…]}` — **HR only**: who drew the
/// payroll file, for which period, in which sheet, and when.
///
/// No figures and nobody's name: a receipt, not a copy of the file.
///
/// # Errors
/// `401`/`403` per the HR door; `500` on a store failure.
pub async fn list_exports(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let exports = state
        .store
        .for_tenant(account.tenant.clone())
        .hr_payroll_exports()
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "exports": exports
            .iter()
            .map(|export| json!({
                "id": export.id.as_str(),
                "from": iso_date(export.from_day),
                "to": iso_date(export.to_day),
                "mapping": export.mapping_key,
                "lineCount": export.line_count,
                "drawnBy": export.drawn_by,
                "drawnAt": iso(export.created_at),
            }))
            .collect::<Vec<_>>(),
    })))
}

/// `GET /hr/payroll-mappings` → `{"mappings":[…]}` — **HR only**: the sheets
/// this build can write, so a screen offers them instead of asking somebody to
/// remember a key.
///
/// # Errors
/// `401`/`403` per the HR door.
pub async fn list_mappings(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    Ok(Json(json!({
        "mappings": PAYROLL_MAPPINGS.iter().map(mapping_json).collect::<Vec<_>>(),
    })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use alo_store::hr_employments::{ContractKind, PayPeriod};
    use alo_store::{HrEmployeeId, PayrollColumn};
    use time::Month;

    fn on(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or_else(|e| panic!("{e}"))
    }

    fn line(family_name: &str) -> PayrollLine {
        PayrollLine {
            employee_id: HrEmployeeId::new("e".to_owned()),
            staff_number: "0042".to_owned(),
            given_name: "Ada".to_owned(),
            family_name: family_name.to_owned(),
            full_name: format!("Ada {family_name}"),
            national_id: "123456782".to_owned(),
            iban: "NL91ABNA0417164300".to_owned(),
            country: "NL".to_owned(),
            date_of_birth: Some(on(1990, Month::December, 10)),
            job_title: "Systeembeheerder".to_owned(),
            team: "Techniek".to_owned(),
            contract_kind: ContractKind::Permanent,
            started_on: on(2024, Month::March, 4),
            ended_on: None,
            weekly_minutes: 2_400,
            pay_amount_cents: Some(320_050),
            pay_period: PayPeriod::Month,
            pay_currency: "EUR".to_owned(),
            paid_leave_minutes: 960,
            sick_leave_minutes: 0,
            unpaid_leave_minutes: 480,
            expense_cents: 4_500,
            mileage_cents: 3_750,
            claims_other_currency: 0,
        }
    }

    fn march() -> (Date, Date) {
        (on(2026, Month::March, 1), on(2026, Month::March, 31))
    }

    fn drawn(key: &str, lines: &[PayrollLine]) -> Vec<String> {
        let mapping = payroll_mapping(key).unwrap_or_else(|error| panic!("{error}"));
        let (from, to) = march();
        file(mapping, lines, from, to)
            .split("\r\n")
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn the_neutral_file_is_the_headings_and_one_record_per_person() {
        let rows = drawn("alo", &[line("Byron"), line("Zola")]);
        assert_eq!(rows.len(), 3, "a header and two people: {rows:?}");
        assert!(rows[0].starts_with("staffNumber,givenName,familyName,fullName,nationalId,iban,"));
        assert!(rows[1].contains("Ada Byron"), "{}", rows[1]);
        assert!(rows[1].contains("3200.50"), "{}", rows[1]);
        assert!(rows[1].ends_with("2026-03-01,2026-03-31"), "{}", rows[1]);
        assert!(rows[2].contains("Ada Zola"), "{}", rows[2]);
    }

    #[test]
    fn a_german_sheet_is_semicolons_dotted_dates_and_a_decimal_comma() {
        let rows = drawn("de", &[line("Byron")]);
        assert_eq!(
            rows[0],
            "Personalnummer;Nachname;Vorname;Geburtsdatum;Sozialversicherungsnummer;IBAN;\
             Vertragsart;Eintrittsdatum;Austrittsdatum;Wochenstunden;Bruttoentgelt;\
             Entgeltzeitraum;Waehrung;Unbezahlte Fehlzeit Stunden;Krankheitsstunden;\
             Urlaubsstunden;Auslagenerstattung;Kilometergeld"
        );
        assert_eq!(
            rows[1],
            "0042;Byron;Ada;10.12.1990;123456782;NL91ABNA0417164300;permanent;04.03.2024;;\
             40,00;3200,50;month;EUR;8,00;0,00;16,00;45,00;37,50"
        );
    }

    #[test]
    fn a_name_that_would_be_a_formula_is_neutralised_and_an_amount_never_is() {
        let mut mischief = line("=cmd|'/c calc'!A1");
        mischief.expense_cents = -4_500;
        let rows = drawn("alo", &[mischief]);
        assert!(
            rows[1].contains("'=cmd|"),
            "a typed name is neutralised: {}",
            rows[1]
        );
        assert!(
            rows[1].contains("-45.00"),
            "a negative amount is a number: {}",
            rows[1]
        );
    }

    #[test]
    fn a_file_says_what_it_is_the_days_it_covers_and_the_sheet_it_is_in() {
        let (from, to) = march();
        let mapping = payroll_mapping("nl").unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            file_name(mapping, from, to),
            "payroll-2026-03-01-to-2026-03-31-nl.csv"
        );
    }

    #[test]
    fn the_mapping_list_names_its_private_columns_for_the_screen_that_warns() {
        let value = mapping_json(payroll_mapping("alo").unwrap_or_else(|error| panic!("{error}")));
        assert_eq!(value["key"], "alo");
        assert_eq!(value["delimiter"], ",");
        let columns = value["columns"].as_array().expect("an array");
        assert_eq!(columns.len(), PayrollColumn::ALL.len());
        let iban = columns
            .iter()
            .find(|column| column["key"] == "iban")
            .expect("the neutral sheet carries the IBAN");
        assert_eq!(iban["private"], true);
        let team = columns
            .iter()
            .find(|column| column["key"] == "team")
            .expect("and the team");
        assert_eq!(team["private"], false);
    }
}
