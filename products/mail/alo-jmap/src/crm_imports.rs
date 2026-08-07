//! Importing a lead list over HTTP (alo CRM, ADR 0035, wave B2.09) — the
//! preview and the commit, over [`alo_store::crm_lead_import`].
//!
//! Two routes, one file, one answer shape:
//!
//! - `POST /crm/imports/leads/preview` — what this file **would** do. Writes
//!   nothing, and is the screen a person corrects their column mapping on.
//! - `POST /crm/imports/leads` — the same reading, committed in one
//!   transaction. Every lead lands or none does; a row that cannot be imported
//!   answers `422` **with the same report**, naming the line and the rule, so
//!   the client shows the fix rather than a sentence about a file.
//!
//! Three things this edge owns, and nothing else — every rule about what a
//! readable file is lives in the store, so a second caller cannot get a weaker
//! definition of one.
//!
//! - **The file is the body, and the mapping is the query string.** What a
//!   person has is a file; asking a client to escape a spreadsheet into a JSON
//!   string first would be a worse surface for no gain (the same decision
//!   `POST /billing/bills/import` made, B1.24). The mapping is a handful of
//!   column *names* — `?company=Firma&email=Kontakt` — which is a URL a script
//!   can quote and a browser can build from the preview's own answer.
//! - **No mapping at all means "guess"**, and the answer always states the
//!   mapping that was used. A client that previews, shows the guess, lets a
//!   person correct it and then commits with the corrected one is the whole
//!   interaction; a commit never silently re-guesses something a person
//!   changed, because the mapping is sent back with it.
//! - **`422` carries the report.** A refusal a person cannot act on is the one
//!   thing an importer must never answer.

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, body::Bytes};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::crm_lead_import::{
    LeadImportReport, LeadImportRequest, LeadMapping, MAX_IMPORT_BYTES,
};
use alo_store::{CrmPipelineId, CrmStageId};

use crate::billing::{iso_date, map_store_err};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// The board, the column, and which column of the file is which field.
///
/// Every mapping field is a **column name from the file's header**, matched
/// case- and space-insensitively by the store; a name the file does not have is
/// a `422` rather than a silently unmapped field.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportQuery {
    /// The board the leads land on. Required.
    #[serde(default)]
    pipeline_id: Option<String>,
    /// The column they land in; the board's first live column when absent.
    #[serde(default)]
    stage_id: Option<String>,
    /// The column naming the opportunity.
    #[serde(default)]
    title: Option<String>,
    /// The column naming the company.
    #[serde(default)]
    company: Option<String>,
    /// The column naming the person.
    #[serde(default)]
    contact_name: Option<String>,
    /// The column holding their address.
    #[serde(default)]
    email: Option<String>,
    /// The column holding what the opportunity is worth.
    #[serde(default)]
    value: Option<String>,
    /// The column holding the currency it is worth it in.
    #[serde(default)]
    currency: Option<String>,
    /// The column holding the expected close date.
    #[serde(default)]
    expected_close: Option<String>,
    /// The column holding where the lead came from.
    #[serde(default)]
    source: Option<String>,
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
    fn read(self) -> Result<LeadImportRequest, Problem> {
        let pipeline_id = stated(self.pipeline_id).ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "pipelineId is required: an import has to land on a board",
            )
        })?;
        Ok(LeadImportRequest {
            pipeline_id: CrmPipelineId::new(pipeline_id),
            stage_id: stated(self.stage_id).map(CrmStageId::new),
            mapping: LeadMapping {
                title: stated(self.title),
                company: stated(self.company),
                contact_name: stated(self.contact_name),
                email: stated(self.email),
                value: stated(self.value),
                currency: stated(self.currency),
                expected_close: stated(self.expected_close),
                source: stated(self.source),
            },
        })
    }
}

/// The mapping as the client sent it back to itself — the shape a preview
/// screen re-submits with the commit.
fn mapping_json(mapping: &LeadMapping) -> Value {
    json!({
        "title": mapping.title,
        "company": mapping.company,
        "contactName": mapping.contact_name,
        "email": mapping.email,
        "value": mapping.value,
        "currency": mapping.currency,
        "expectedClose": mapping.expected_close,
        "source": mapping.source,
    })
}

/// The whole report: what was read, what would be (or was) created, what was
/// skipped, and what cannot be imported.
///
/// The leads carry the **server's** reading of every field — the trimmed title,
/// the integer cents, the ISO day — so a preview screen shows what will be
/// stored rather than what was typed.
fn report_json(report: &LeadImportReport) -> Value {
    json!({
        "committed": report.committed,
        "encoding": report.encoding,
        "delimiter": report.delimiter.to_string(),
        "columns": report.columns,
        "mapping": mapping_json(&report.mapping),
        "totalRows": report.total_rows,
        "counts": {
            "create": report.leads.len(),
            "duplicates": report.duplicates.len(),
            "errors": report.errors.len(),
        },
        "leads": report.leads.iter().map(|lead| json!({
            "line": lead.line,
            "id": lead.id.as_ref().map(alo_store::CrmDealId::as_str),
            "title": lead.deal.title,
            "companyName": lead.deal.company_name,
            "contactName": lead.deal.contact_name,
            "contactEmail": lead.deal.contact_email,
            "valueCents": lead.deal.value_cents,
            "currency": lead.deal.currency,
            "expectedClose": lead.deal.expected_close.map(iso_date),
            "source": lead.deal.source,
        })).collect::<Vec<_>>(),
        "duplicates": report.duplicates.iter().map(|row| json!({
            "line": row.line,
            "reason": row.reason.as_str(),
            "source": row.source.as_str(),
            "matched": row.matched,
        })).collect::<Vec<_>>(),
        "errors": report.errors.iter().map(|row| json!({
            "line": row.line,
            "rule": row.rule,
        })).collect::<Vec<_>>(),
    })
}

/// Refuses an upload larger than the store's cap before it is decoded — the
/// same courtesy `POST /billing/bills/import` does, and the reason the route is
/// also given a body limit in `server.rs`.
fn check_size(body: &Bytes) -> Result<(), Problem> {
    if body.len() > MAX_IMPORT_BYTES {
        return Err(Problem::with(
            StatusCode::PAYLOAD_TOO_LARGE,
            "the file is too large to be a lead list",
        ));
    }
    Ok(())
}

/// `POST /crm/imports/leads/preview?pipelineId&stageId&…` (the CSV as the body)
/// → `{"import":{…}}` — what the file would do. Nothing is written.
pub async fn preview_leads(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ImportQuery>,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let request = query.read()?;
    check_size(&body)?;
    let report = account
        .acc
        .preview_crm_lead_import(&request, &body)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "import": report_json(&report) })))
}

/// `POST /crm/imports/leads?pipelineId&stageId&…` (the CSV as the body) →
/// `{"import":{…}}` — the commit.
///
/// `200` when the leads were written (duplicates skipped and reported), `422`
/// with the same report when any row is invalid and therefore **nothing** was
/// written. The all-or-nothing rule is the store's, and this is what it looks
/// like on the wire.
pub async fn import_leads(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ImportQuery>,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let request = query.read()?;
    check_size(&body)?;
    let report = account
        .acc
        .import_crm_leads(&request, &body)
        .await
        .map_err(map_store_err)?;
    let answer = json!({ "import": report_json(&report) });
    if report.committed {
        return Ok(Json(answer));
    }
    Err(Problem::with(
        StatusCode::UNPROCESSABLE_ENTITY,
        "some rows cannot be imported; nothing was written",
    )
    .with_extra(answer))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use alo_store::crm_lead_import::{DuplicateReason, DuplicateRow, DuplicateSource, RowError};

    /// The query as axum hands it over, built through the same serde
    /// attributes the extractor uses. Percent-decoding is the extractor's own
    /// job and is exercised for real by `tests/crm_import_http.rs`.
    fn query(fields: Value) -> ImportQuery {
        serde_json::from_value(fields).expect("a readable query")
    }

    #[test]
    fn a_query_without_a_board_is_refused_before_the_file_is_read() {
        let problem = query(json!({ "company": "Firma" }))
            .read()
            .expect_err("no pipelineId");
        assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        // And a blank one is no board at all.
        let problem = query(json!({ "pipelineId": "  " }))
            .read()
            .expect_err("a blank pipelineId");
        assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn the_mapping_is_the_query_string_and_blanks_are_unstated() {
        let request = query(json!({
            "pipelineId": "pip_1",
            "stageId": "stg_1",
            "company": "Firma",
            "email": "",
            "value": "Umsatz",
        }))
        .read()
        .expect("a full mapping");
        assert_eq!(request.pipeline_id.as_str(), "pip_1");
        assert_eq!(
            request.stage_id.as_ref().map(CrmStageId::as_str),
            Some("stg_1")
        );
        assert_eq!(request.mapping.company.as_deref(), Some("Firma"));
        assert_eq!(request.mapping.value.as_deref(), Some("Umsatz"));
        assert_eq!(request.mapping.email, None, "an empty field maps nothing");
        assert_eq!(request.mapping.title, None);
    }

    #[test]
    fn a_column_name_is_trimmed_and_otherwise_taken_verbatim() {
        let request = query(json!({
            "pipelineId": "pip_1",
            "company": " Société ",
            "value": "Montant HT",
        }))
        .read()
        .expect("a readable mapping");
        assert_eq!(request.mapping.company.as_deref(), Some("Société"));
        assert_eq!(request.mapping.value.as_deref(), Some("Montant HT"));
    }

    #[test]
    fn no_mapping_at_all_asks_the_store_to_guess() {
        let request = query(json!({ "pipelineId": "pip_1" }))
            .read()
            .expect("a bare request");
        assert!(request.mapping.is_empty(), "an empty mapping is the guess");
        assert_eq!(request.stage_id, None, "and the board's first column");
    }

    #[test]
    fn the_report_names_every_line_and_never_quotes_a_row() {
        let report = LeadImportReport {
            committed: false,
            encoding: "windows-1252",
            delimiter: ';',
            columns: vec!["Firma".to_owned(), "Kontakt".to_owned()],
            mapping: LeadMapping {
                company: Some("Firma".to_owned()),
                ..LeadMapping::default()
            },
            total_rows: 3,
            leads: Vec::new(),
            duplicates: vec![DuplicateRow {
                line: 2,
                reason: DuplicateReason::Domain,
                source: DuplicateSource::Crm,
                matched: "acme.example".to_owned(),
            }],
            errors: vec![RowError {
                line: 3,
                rule: "the row states neither a title nor a company".to_owned(),
            }],
        };
        let json = report_json(&report);
        assert_eq!(json["committed"], false);
        assert_eq!(json["encoding"], "windows-1252");
        assert_eq!(json["delimiter"], ";");
        assert_eq!(json["counts"]["duplicates"], 1);
        assert_eq!(json["counts"]["errors"], 1);
        assert_eq!(json["counts"]["create"], 0);
        assert_eq!(json["duplicates"][0]["line"], 2);
        assert_eq!(json["duplicates"][0]["reason"], "domain");
        assert_eq!(json["duplicates"][0]["source"], "crm");
        assert_eq!(json["errors"][0]["line"], 3);
        assert_eq!(json["mapping"]["company"], "Firma");
        assert_eq!(json["mapping"]["email"], Value::Null);
    }
}
