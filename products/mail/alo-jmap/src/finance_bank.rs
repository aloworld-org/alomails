//! The bank statement over HTTP (alo Finance, ADR 0035, wave B4.08c) — one
//! upload door for three formats, and the two reads a reconciliation screen
//! lives on.
//!
//! - `POST /finance/imports/bank/preview` — what this file **would** stage.
//!   Writes nothing (the store's reading is a pure function, so it *can* write
//!   nothing), and is the screen a person corrects their column mapping on.
//! - `POST /finance/imports/bank` — the same reading, staged. A file with a row
//!   that cannot be read answers `422` **with the same report**, naming every
//!   broken line, because nothing is imported halfway.
//! - `GET /finance/bank/statements` and `GET /finance/bank/lines` — what has
//!   been imported, and where each line stands.
//!
//! Three things this edge owns, and nothing else — every rule about what a
//! readable statement is lives in the store, so a second caller cannot get a
//! weaker definition of one.
//!
//! - **The file is the body, and everything about it is the query string.**
//!   What a person has is a file; asking a client to escape a spreadsheet into
//!   a JSON string first would be a worse surface for no gain (the decision
//!   `POST /billing/bills/import` and `/crm/imports/leads` both made). The
//!   mapping is a handful of column *names* — `?amount=Betrag&date=Buchungstag`
//!   — which is a URL a script can quote and a browser can build from the
//!   preview's own answer.
//! - **The format is sniffed unless it is stated.** A person uploading their
//!   bank's export should not have to know whether it is CAMT.053; `?format=`
//!   is there for the case where they do.
//! - **`422` carries the report.** A refusal a person cannot act on is the one
//!   thing an importer must never answer.

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, body::Bytes};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::bank_read::{BankFileImport, BankFileReading, BankImportRequest, read_bank_file};
use alo_store::{
    BankCsvDates, BankCsvDecimal, BankCsvMapping, BankImport, BankLine, BankLineStatus, BankSource,
    BankStatement, BankStatementId, MAX_BANK_FILE_BYTES, ParsedLine,
};

use crate::billing::{iso, iso_date, map_store_err};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// How many transactions a report shows. The counts are exact; the rows are a
/// **sample**, because a year of a busy account is thousands of lines and a
/// person confirming a mapping needs to see that the columns line up, not to
/// read the year. The staged lines are then read, paged and filtered, through
/// `GET /finance/bank/lines`.
const SAMPLE_LINES: usize = 50;

/// The account, the two conventions, and which column of the file is which.
///
/// Every mapping field is a **column name from the file's header**, matched
/// case- and space-insensitively by the store; a name the file does not have is
/// a `422` rather than a silently unmapped field. All of it is ignored for a
/// CAMT.053 or MT940 file, which states these things itself.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportQuery {
    /// `camt`, `mt940` or `csv`; sniffed from the file when absent.
    #[serde(default)]
    format: Option<String>,
    /// The account this file is the statement of. Required for a CSV.
    #[serde(default)]
    account: Option<String>,
    /// The statement's currency (CSV); the tenant's default when absent.
    #[serde(default)]
    currency: Option<String>,
    /// `auto`, `dmy`, `mdy` or `ymd`.
    #[serde(default)]
    dates: Option<String>,
    /// `auto`, `comma` or `dot`.
    #[serde(default)]
    decimal: Option<String>,
    /// The column holding the booking date.
    #[serde(default)]
    date: Option<String>,
    /// The column holding the value date.
    #[serde(default)]
    value_date: Option<String>,
    /// The column holding one signed amount.
    #[serde(default)]
    amount: Option<String>,
    /// The column holding money out.
    #[serde(default)]
    debit: Option<String>,
    /// The column holding money in.
    #[serde(default)]
    credit: Option<String>,
    /// The column saying which way an unsigned amount points.
    #[serde(default)]
    sign: Option<String>,
    /// The column holding a per-row currency.
    #[serde(default)]
    currency_column: Option<String>,
    /// The column naming the other party.
    #[serde(default)]
    counterparty: Option<String>,
    /// The column holding their account.
    #[serde(default)]
    iban: Option<String>,
    /// The column holding what was written on the payment.
    #[serde(default)]
    remittance: Option<String>,
    /// The column holding the bank's own reference.
    #[serde(default)]
    reference: Option<String>,
}

/// A blank query parameter is an unstated one: a client that builds the URL
/// from an empty form field must not map a column called "".
fn stated(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_owned())
        .filter(|raw| !raw.is_empty())
}

impl ImportQuery {
    /// The store's request shape, or the `422` a caller can act on.
    fn read(self) -> Result<BankImportRequest, Problem> {
        let source = match stated(self.format) {
            None => None,
            Some(raw) => Some(BankSource::parse(&raw).ok_or_else(|| {
                Problem::with(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "format must be camt, mt940 or csv — or left out, and read from the file",
                )
            })?),
        };
        let dates = match stated(self.dates) {
            None => BankCsvDates::Auto,
            Some(raw) => BankCsvDates::parse(&raw).ok_or_else(|| {
                Problem::with(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "dates must be auto, dmy, mdy or ymd",
                )
            })?,
        };
        let decimal = match stated(self.decimal) {
            None => BankCsvDecimal::Auto,
            Some(raw) => BankCsvDecimal::parse(&raw).ok_or_else(|| {
                Problem::with(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "decimal must be auto, comma or dot",
                )
            })?,
        };
        Ok(BankImportRequest {
            source,
            account_iban: stated(self.account).unwrap_or_default(),
            currency: stated(self.currency),
            dates,
            decimal,
            mapping: BankCsvMapping {
                booked_on: stated(self.date),
                value_on: stated(self.value_date),
                amount: stated(self.amount),
                debit: stated(self.debit),
                credit: stated(self.credit),
                sign: stated(self.sign),
                currency: stated(self.currency_column),
                counterparty_name: stated(self.counterparty),
                counterparty_iban: stated(self.iban),
                remittance: stated(self.remittance),
                bank_ref: stated(self.reference),
            },
        })
    }
}

/// The mapping as the client sends it back to itself — the shape a preview
/// screen re-submits with the commit.
fn mapping_json(mapping: &BankCsvMapping) -> Value {
    json!({
        "date": mapping.booked_on,
        "valueDate": mapping.value_on,
        "amount": mapping.amount,
        "debit": mapping.debit,
        "credit": mapping.credit,
        "sign": mapping.sign,
        "currencyColumn": mapping.currency,
        "counterparty": mapping.counterparty_name,
        "iban": mapping.counterparty_iban,
        "remittance": mapping.remittance,
        "reference": mapping.bank_ref,
    })
}

/// One transaction as the reader understood it — the server's reading of the
/// row, so a preview shows what would be stored rather than what was typed.
fn sample_json(line: &ParsedLine, at: Option<usize>) -> Value {
    json!({
        "line": at,
        "bookedOn": iso_date(line.booked_on),
        "valueOn": iso_date(line.value_on),
        "amountCents": line.amount_cents,
        "currency": line.currency,
        "counterpartyName": line.counterparty_name,
        "counterpartyIban": line.counterparty_iban,
        "remittance": line.remittance,
        "bankRef": line.bank_ref,
    })
}

/// A stored statement — the header a screen lists imports by.
pub(crate) fn statement_json(statement: &BankStatement) -> Value {
    json!({
        "id": statement.id.as_str(),
        "accountIban": statement.account_iban,
        "currency": statement.currency,
        "source": statement.source.as_str(),
        "statementRef": statement.statement_ref,
        "openingBalanceCents": statement.opening_balance_cents,
        "closingBalanceCents": statement.closing_balance_cents,
        "fromDate": iso_date(statement.from_date),
        "toDate": iso_date(statement.to_date),
        "importedBy": statement.imported_by.as_str(),
        "importedAt": iso(statement.imported_at),
        "lineCount": statement.line_count,
    })
}

/// One staged line.
fn line_json(line: &BankLine) -> Value {
    json!({
        "id": line.id.as_str(),
        "statementId": line.statement_id.as_str(),
        "lineNo": line.line_no,
        "bookedOn": iso_date(line.booked_on),
        "valueOn": iso_date(line.value_on),
        "amountCents": line.amount_cents,
        "currency": line.currency,
        "counterpartyName": line.counterparty_name,
        "counterpartyIban": line.counterparty_iban,
        "remittance": line.remittance,
        "bankRef": line.bank_ref,
        "status": line.status.as_str(),
        "createdAt": iso(line.created_at),
    })
}

/// The whole report: how the file was read, what it would stage (or did), what
/// was skipped, and what cannot be read at all.
fn report_json(reading: &BankFileReading, imported: Option<&BankImport>) -> Value {
    let lines = reading
        .statement
        .as_ref()
        .map_or(0, |statement| statement.lines.len());
    let sample: Vec<Value> = reading
        .statement
        .as_ref()
        .map(|statement| {
            statement
                .lines
                .iter()
                .take(SAMPLE_LINES)
                .enumerate()
                .map(|(index, line)| sample_json(line, reading.at.get(index).copied()))
                .collect()
        })
        .unwrap_or_default();
    json!({
        "committed": imported.is_some(),
        "source": reading.source.as_str(),
        "encoding": reading.encoding,
        "delimiter": reading.delimiter.map(|d| d.to_string()),
        "columns": reading.columns,
        "mapping": mapping_json(&reading.mapping),
        "dates": reading.dates.as_str(),
        "decimal": reading.decimal.as_str(),
        "totalRows": reading.total_rows,
        "counts": {
            "lines": lines,
            "skipped": reading.skipped.len(),
            "errors": reading.errors.len(),
            "staged": imported.map(|import| import.staged),
            "duplicates": imported.map(|import| import.duplicates),
            "unbooked": imported.map_or_else(
                || reading.statement.as_ref().map_or(0, |s| s.unbooked),
                |import| import.unbooked,
            ),
        },
        "account": reading.statement.as_ref().map(|s| s.account_iban.clone()),
        "currency": reading.statement.as_ref().map(|s| s.currency.clone()),
        "period": reading.statement.as_ref().map(|s| json!({
            "from": iso_date(s.from_date),
            "to": iso_date(s.to_date),
        })),
        "sample": sample,
        "sampleTruncated": lines > SAMPLE_LINES,
        "skippedLines": reading.skipped,
        "errors": reading.errors.iter().map(|row| json!({
            "line": row.line,
            "rule": row.rule,
        })).collect::<Vec<_>>(),
        "statement": imported.map(|import| statement_json(&import.statement)),
    })
}

/// Refuses an upload larger than the store's cap before it is decoded — the
/// same courtesy `POST /crm/imports/leads` does, and the reason the route is
/// also given a body limit in `server.rs`.
fn check_size(body: &Bytes) -> Result<(), Problem> {
    if body.len() > MAX_BANK_FILE_BYTES {
        return Err(Problem::with(
            StatusCode::PAYLOAD_TOO_LARGE,
            "the file is too large to be a bank statement",
        ));
    }
    Ok(())
}

/// `POST /finance/imports/bank/preview?account&…` (the file as the body) →
/// `{"import":{…}}` — what the file would stage. Nothing is written.
pub async fn preview_bank_import(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ImportQuery>,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    // Authenticated before anything is read: a dry run is still a door, and an
    // unauthenticated caller must not be able to use ours as a CSV parser.
    let _account = authenticate(&state, &headers).await?;
    let request = query.read()?;
    check_size(&body)?;
    let reading = read_bank_file(&request, &body).map_err(map_store_err)?;
    Ok(Json(json!({ "import": report_json(&reading, None) })))
}

/// `POST /finance/imports/bank?account&…` (the file as the body) →
/// `{"import":{…}}` — the commit.
///
/// `200` when the statement was staged (duplicate lines skipped and counted),
/// `422` with the same report when any row cannot be read and therefore
/// **nothing** was written, `409` when these exact bytes were imported before.
pub async fn import_bank_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ImportQuery>,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let request = query.read()?;
    check_size(&body)?;
    let BankFileImport { reading, imported } = account
        .acc
        .import_bank_file(&request, &body)
        .await
        .map_err(map_store_err)?;
    let answer = json!({ "import": report_json(&reading, imported.as_ref()) });
    if imported.is_some() {
        return Ok(Json(answer));
    }
    Err(Problem::with(
        StatusCode::UNPROCESSABLE_ENTITY,
        "some rows of this file cannot be read; nothing was imported",
    )
    .with_extra(answer))
}

/// `GET /finance/bank/statements` → `{"statements":[…]}` — the most recent
/// period first.
pub async fn list_bank_statements(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let statements = account.acc.bank_statements().await.map_err(map_store_err)?;
    Ok(Json(json!({
        "statements": statements.iter().map(statement_json).collect::<Vec<_>>(),
    })))
}

/// What a lines read may be narrowed by.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LinesQuery {
    /// One import.
    #[serde(default)]
    statement: Option<String>,
    /// One state — `unmatched`, `matched` or `ignored`.
    #[serde(default)]
    status: Option<String>,
}

/// `GET /finance/bank/lines?statement=&status=` → `{"lines":[…]}` — oldest
/// first, the order a bookkeeper works a month in.
///
/// An unknown status is a `422` rather than an empty list: silently answering
/// "nothing" to a filter nobody implements is how a screen ends up looking
/// empty for a reason no one can find. An unknown *statement* is an empty list,
/// because a narrowing that matches nothing matches nothing — and answering
/// otherwise would make the filter an oracle for another tenant's ids.
pub async fn list_bank_lines(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LinesQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let status = match stated(query.status) {
        None => None,
        Some(raw) => Some(BankLineStatus::parse(&raw).ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "status must be unmatched, matched or ignored",
            )
        })?),
    };
    let statement = stated(query.statement).map(BankStatementId::new);
    let lines = account
        .acc
        .bank_lines(statement.as_ref(), status)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "lines": lines.iter().map(line_json).collect::<Vec<_>>(),
    })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// The query as axum hands it over, built through the same serde attributes
    /// the extractor uses. Percent-decoding is the extractor's own job and is
    /// exercised for real by `tests/finance_bank_http.rs`.
    fn query(fields: Value) -> ImportQuery {
        serde_json::from_value(fields).expect("a readable query")
    }

    #[test]
    fn the_mapping_is_the_query_string_and_blanks_are_unstated() {
        let request = query(json!({
            "account": " DE02 1203 0000 0000 2020 51 ",
            "date": "Buchungstag",
            "amount": "Betrag",
            "remittance": "",
            "decimal": "comma",
            "dates": "dmy",
        }))
        .read()
        .expect("a readable mapping");
        assert_eq!(request.account_iban, "DE02 1203 0000 0000 2020 51");
        assert_eq!(request.mapping.booked_on.as_deref(), Some("Buchungstag"));
        assert_eq!(request.mapping.amount.as_deref(), Some("Betrag"));
        assert_eq!(
            request.mapping.remittance, None,
            "an empty field maps nothing"
        );
        assert_eq!(request.decimal, BankCsvDecimal::Comma);
        assert_eq!(request.dates, BankCsvDates::Dmy);
        assert_eq!(request.source, None, "the format is read from the file");
    }

    #[test]
    fn no_mapping_at_all_asks_the_store_to_guess() {
        let request = query(json!({ "account": "DE02120300000000202051" }))
            .read()
            .expect("a bare request");
        assert!(request.mapping.is_empty(), "an empty mapping is the guess");
        assert_eq!(request.dates, BankCsvDates::Auto);
        assert_eq!(request.decimal, BankCsvDecimal::Auto);
    }

    #[test]
    fn a_convention_this_reader_does_not_know_is_refused_before_the_file_is_read() {
        for bad in [
            json!({ "format": "ofx" }),
            json!({ "dates": "ddmmyyyy" }),
            json!({ "decimal": "german" }),
        ] {
            let problem = query(bad).read().expect_err("an unknown word");
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[test]
    fn the_report_counts_every_row_and_never_quotes_one() {
        let reading = BankFileReading {
            source: BankSource::Csv,
            encoding: Some("windows-1252"),
            delimiter: Some(';'),
            columns: vec!["Buchungstag".to_owned(), "Betrag".to_owned()],
            mapping: BankCsvMapping {
                booked_on: Some("Buchungstag".to_owned()),
                amount: Some("Betrag".to_owned()),
                ..BankCsvMapping::default()
            },
            dates: BankCsvDates::Dmy,
            decimal: BankCsvDecimal::Comma,
            total_rows: 3,
            at: Vec::new(),
            skipped: vec![4],
            errors: vec![alo_store::RowError {
                line: 3,
                rule: "the row's booking date is missing".to_owned(),
            }],
            statement: None,
        };
        let report = report_json(&reading, None);
        assert_eq!(report["committed"], false);
        assert_eq!(report["source"], "csv");
        assert_eq!(report["encoding"], "windows-1252");
        assert_eq!(report["delimiter"], ";");
        assert_eq!(report["dates"], "dmy");
        assert_eq!(report["decimal"], "comma");
        assert_eq!(report["counts"]["errors"], 1);
        assert_eq!(report["counts"]["skipped"], 1);
        assert_eq!(report["counts"]["lines"], 0);
        assert_eq!(report["counts"]["staged"], Value::Null);
        assert_eq!(report["errors"][0]["line"], 3);
        assert_eq!(report["skippedLines"][0], 4);
        assert_eq!(report["statement"], Value::Null);
        assert_eq!(report["mapping"]["date"], "Buchungstag");
        assert_eq!(report["mapping"]["credit"], Value::Null);
    }
}
