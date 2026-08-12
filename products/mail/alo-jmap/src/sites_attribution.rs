//! The Sites → CRM/Billing seam at the HTTP edge (ADR 0036, S2.10b): handing
//! a website enquiry to a sales opportunity, and reading what the website is
//! worth once it has.
//!
//! A separate module from [`crate::sites`] for a separate reason to change:
//! every other `/sites/{id}` route answers about the website, and these four
//! answer about the business behind it. That difference is not cosmetic — it
//! is a **permission boundary**, and it is the whole reason this file exists
//! rather than four more handlers in the sites module.
//!
//! ## Who may pass
//!
//! `/sites/{id}/*` is the one surface a **site editor** is allowed to use
//! ([`crate::scoped_roles`]): an outside collaborator invited to build one
//! website. They may not read Mail, Drive, CRM or Billing (S2.03a), so a route
//! that answers "these are your customer's opportunities and what they were
//! invoiced" must refuse them — the middleware cannot tell one `/sites/{id}`
//! route from another, so the refusal lives here, stated once
//! ([`require_crm_reader`]).
//!
//! Two more gates apply for the same reason:
//!
//! - **The CRM module switch is honoured.** A colleague an admin has switched
//!   CRM off for (migration 0208) does not get CRM data through a website
//!   route. Reaching a module's data through another module's door is exactly
//!   the hole a per-module switch would otherwise have.
//! - **An accountant reads and does not write.** The scoped-role middleware
//!   refuses CRM writes on `/crm/*`; a handoff is a CRM write made at a
//!   `/sites/*` path, so it is refused here in the same words.
//!
//! ## Errors
//!
//! The `/sites/{id}` contract, unchanged: `401` unauthenticated, `403` for a
//! caller who may not see the business behind the website, `404` for a site,
//! submission, deal or link that does not resolve in the caller's tenant —
//! another tenant's is indistinguishable from one that never existed — `422`
//! for a rule the store names, and `400` for a body that is not the shape.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, body::Bytes};
use serde::Deserialize;
use serde_json::{Value, json};
use time::{Duration, OffsetDateTime};

use alo_store::user_modules::AppModule;
use alo_store::{
    CrmDealId, CrmPipelineId, CrmStageId, SiteAttributionReport, SiteAttributionSource,
    SiteFormSubmissionId, SiteId, SiteLeadDraft, SiteLeadLink, SiteLeadLinkId, TenantRole,
};

use crate::billing::iso;
use crate::error::Problem;
use crate::sites::map_store_err;
use crate::state::{Account, AppState, authenticate};

/// What a site editor is told when they reach for the business behind the
/// website they were invited to build.
const SITE_EDITOR_DENIAL: &str =
    "this site editor can use only the websites they have been invited to";

/// Guard for reading CRM and Billing identities through a `/sites` route.
///
/// # Errors
/// [`Problem`] 403 for a site editor, or for a caller whose admin has switched
/// CRM off.
fn require_crm_reader(account: &Account) -> Result<(), Problem> {
    if !account.is_admin && account.has_role(TenantRole::SiteEditor) {
        return Err(Problem::with(StatusCode::FORBIDDEN, SITE_EDITOR_DENIAL));
    }
    if !account.may_open(AppModule::Crm) {
        return Err(Problem::with(
            StatusCode::FORBIDDEN,
            "alo CRM is switched off for this account",
        ));
    }
    Ok(())
}

/// Guard for creating or removing the link. Widens [`require_crm_reader`] by
/// the one rule that separates reading CRM from changing it.
///
/// # Errors
/// [`Problem`] 403 as [`require_crm_reader`], or for an accountant, who reads
/// billing and CRM and does not change them.
fn require_crm_writer(account: &Account) -> Result<(), Problem> {
    require_crm_reader(account)?;
    if !account.is_admin && account.has_role(TenantRole::Accountant) {
        return Err(Problem::with(
            StatusCode::FORBIDDEN,
            "an accountant may read billing and CRM, not change them",
        ));
    }
    Ok(())
}

// ---- the handoff ------------------------------------------------------------

/// Either half of the handoff: name the opportunity this enquiry already
/// became, or state the board and column a new one should be raised in.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HandoffBody {
    /// An opportunity that already exists. Mutually exclusive with the board
    /// fields below.
    deal_id: Option<String>,
    pipeline_id: Option<String>,
    stage_id: Option<String>,
    /// The line on the card. The store writes titles it is handed and invents
    /// none, so a new opportunity needs one.
    title: Option<String>,
    company_name: Option<String>,
    /// What the opportunity is thought to be worth, in integer cents. Money is
    /// never a decimal on this wire.
    value_cents: Option<i64>,
    currency: Option<String>,
    owner_user_id: Option<String>,
    source: Option<String>,
}

/// `POST /sites/:id/submissions/:submission/lead` -> the link, `201`.
///
/// With `dealId`, the enquiry is attached to an opportunity that already
/// exists. With `pipelineId` + `stageId` + `title`, a new one is raised from
/// the enquiry itself — the enquirer's name and address are taken from the
/// submission and never re-typed, which is the point of a handoff.
pub async fn create_lead(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, submission)): Path<(String, String)>,
    body: Bytes,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let account = authenticate(&state, &headers).await?;
    require_crm_writer(&account)?;
    let req: HandoffBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let site = SiteId::new(id);
    let submission = SiteFormSubmissionId::new(submission);

    let link = match (
        req.deal_id
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
        req.pipeline_id
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
        req.stage_id
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
    ) {
        (Some(deal), None, None) => account
            .acc
            .link_site_lead(&site, &submission, &CrmDealId::new(deal.to_owned()))
            .await
            .map_err(map_store_err)?,
        (None, Some(pipeline), Some(stage)) => {
            let draft = SiteLeadDraft {
                title: req.title.unwrap_or_default(),
                company_name: req.company_name.unwrap_or_default(),
                value_cents: req.value_cents.unwrap_or_default(),
                currency: req.currency.unwrap_or_default(),
                owner_user_id: req
                    .owner_user_id
                    .map(|owner| owner.trim().to_owned())
                    .filter(|owner| !owner.is_empty()),
                source: req.source.unwrap_or_default(),
            };
            account
                .acc
                .create_site_lead(
                    &site,
                    &submission,
                    &CrmPipelineId::new(pipeline.to_owned()),
                    &CrmStageId::new(stage.to_owned()),
                    &draft,
                )
                .await
                .map_err(map_store_err)?
        }
        _ => {
            return Err(Problem::with(
                StatusCode::BAD_REQUEST,
                "state either an existing dealId or a pipelineId and stageId to raise one in",
            ));
        }
    };
    Ok((StatusCode::CREATED, Json(link_json(&link))))
}

/// `GET /sites/:id/leads` -> every enquiry of this site that became an
/// opportunity, newest first. The submissions list reads it to say which
/// enquiries have already been dealt with.
pub async fn list_leads(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    require_crm_reader(&account)?;
    let links = account
        .acc
        .site_lead_links(&SiteId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "leads": links.iter().map(link_json).collect::<Vec<_>>(),
    })))
}

/// `DELETE /sites/:id/leads/:link` -> `204`. Unclaims the opportunity for the
/// website; the opportunity itself is CRM's and is left exactly as it is.
pub async fn delete_lead(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, link)): Path<(String, String)>,
) -> Result<StatusCode, Problem> {
    let account = authenticate(&state, &headers).await?;
    require_crm_writer(&account)?;
    account
        .acc
        .unlink_site_lead(&SiteId::new(id), &SiteLeadLinkId::new(link))
        .await
        .map_err(map_store_err)?;
    Ok(StatusCode::NO_CONTENT)
}

fn link_json(link: &SiteLeadLink) -> Value {
    json!({
        "id": link.id.as_str(),
        "siteId": link.site_id.as_str(),
        "sourceKind": link.source_kind,
        "sourceId": link.source_id,
        "submissionId": link.submission_id.as_str(),
        "linkedBy": link.linked_by,
        "linkedAt": iso(link.linked_at),
        "deal": {
            "id": link.deal.id.as_str(),
            "title": link.deal.title,
            "valueCents": link.deal.value_cents,
            "currency": link.deal.currency,
            "state": link.deal.state.as_str(),
        },
    })
}

// ---- the funnel -------------------------------------------------------------

#[derive(Deserialize)]
pub struct AttributionQuery {
    days: Option<u16>,
}

/// `GET /sites/:id/attribution?days=30` -> the funnel from page views to
/// invoices, per conversion point and for the site.
///
/// The period selects the enquiries, not the money: an invoice raised in March
/// for a January enquiry is January's doing, so the window bounds the counts
/// and the handoffs, and the opportunities and documents of those handoffs are
/// then reported as they stand now.
pub async fn get_attribution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<AttributionQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    require_crm_reader(&account)?;
    let days = query.days.unwrap_or(30);
    if !(1..=365).contains(&days) {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "attribution period must be between 1 and 365 days",
        ));
    }
    let to = OffsetDateTime::now_utc().date();
    let from = to - Duration::days(i64::from(days - 1));
    let report = account
        .acc
        .site_attribution(&SiteId::new(id), from, to)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such site"))?;

    // Whether the *invoices* may be shown is a separate question from whether
    // the funnel may be read: an admin can switch Billing off for a colleague
    // who still works the pipeline. That person keeps the pipeline's own
    // figures — a deal's value is CRM's — and loses the documents, which is
    // exactly the line the switch draws. The response says which it gave.
    let billing = account.may_open(AppModule::Billing);
    Ok(Json(report_json(from, to, &report, billing)))
}

/// The currency lines. `invoicedCents` is `null` rather than `0` when Billing
/// is switched off for this person: a screen must be able to say "not yours to
/// see" instead of "nothing was invoiced", which is a different statement.
fn money_json(money: &[alo_store::SiteAttributionMoney], visible: bool) -> Value {
    money
        .iter()
        .map(|line| {
            json!({
                "currency": line.currency,
                "openCents": line.open_cents,
                "wonCents": line.won_cents,
                "invoicedCents": if visible { json!(line.invoiced_cents) } else { Value::Null },
            })
        })
        .collect::<Vec<_>>()
        .into()
}

fn source_json(source: &SiteAttributionSource, billing: bool) -> Value {
    json!({
        "kind": source.kind,
        "id": source.id,
        "name": source.name,
        "views": source.views,
        "starts": source.starts,
        "submits": source.submits,
        "leads": source.leads,
        "dealsOpen": source.deals_open,
        "dealsWon": source.deals_won,
        "dealsLost": source.deals_lost,
        "invoices": if billing { json!(source.invoices) } else { Value::Null },
        "money": money_json(&source.money, billing),
    })
}

/// The funnel as JSON. Stage counts stay flat and named for the reason
/// [`crate::sites_conversions`] gives — they were counted independently — and
/// the business figures are added to each conversion point rather than nested
/// under a second list, so one row of an interface is one object.
fn report_json(
    from: time::Date,
    to: time::Date,
    report: &SiteAttributionReport,
    billing: bool,
) -> Value {
    json!({
        "from": from.to_string(),
        "to": to.to_string(),
        // What the invoice figures mean, said in the payload rather than left
        // for a screen to imply: they are documents raised for the customer a
        // lead became, after it became one.
        "invoiceRule": "customerSinceLead",
        "billingVisible": billing,
        "totals": {
            "views": report.views,
            "starts": report.starts,
            "submits": report.submits,
            "leads": report.leads,
            "dealsOpen": report.deals_open,
            "dealsWon": report.deals_won,
            "dealsLost": report.deals_lost,
            "invoices": if billing { json!(report.invoices) } else { Value::Null },
            "money": money_json(&report.money, billing),
        },
        "sources": report.sources.iter()
            .map(|source| source_json(source, billing))
            .collect::<Vec<_>>(),
    })
}
