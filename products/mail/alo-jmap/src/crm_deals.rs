//! CRM deals HTTP surface (alo CRM, ADR 0035, wave B2) — the opportunities
//! that move across a board, on top of [`alo_store::crm_deals`].
//!
//! Three rules from the design note (`docs/design/crm.md` § Routes) are what
//! this module is shaped by:
//!
//! - **A lifecycle change is its own `POST`, never a field on the `PATCH`.**
//!   Moving a deal writes a history row and can close the deal, so `stageId`,
//!   `position`, `state`, `lostReason` and `closedAt` are not writable by
//!   `PATCH` — like any unknown field they are ignored, and the answer carries
//!   the stored record so a caller sees that they did nothing.
//! - **Money is only ever written as integer cents.** `valueCents` is a JSON
//!   integer; a client that sends `1250.5` gets a `400`, not a rounded deal.
//! - **Filters are strict.** A `state` or a board/column id the tenant does not
//!   have is a `422`, not a silently widened list: a sales manager reading
//!   "everything" when they asked for "mine" is a wrong number on a screen.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::AccountStore;
use alo_store::ThreadId;
use alo_store::crm_deals::{Deal, DealFilter, DealState, NewDeal, StageEvent, StageMove};
use alo_store::{BillingCustomerId, ContactId, CrmDealId, CrmPipelineId, CrmStageId};

use crate::billing::{
    absent_or_null, blank_to_none, iso, iso_date, map_store_err, parse_body, parse_iso_date,
};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// A deal as JSON.
///
/// `state` is **derived on read** from the deal's own snapshot, in the same
/// spirit as an invoice's `overdue`: the client is told where the deal stands
/// rather than being left to join it to the board's flags and get it wrong.
pub(crate) fn deal_json(d: &Deal) -> Value {
    json!({
        "id": d.id.as_str(),
        "pipelineId": d.pipeline_id.as_str(),
        "stageId": d.stage_id.as_str(),
        "title": d.title,
        "customerId": d.customer_id.as_ref().map(BillingCustomerId::as_str),
        "contactId": d.contact_id.as_ref().map(ContactId::as_str),
        "companyName": d.company_name,
        "contactName": d.contact_name,
        "contactEmail": d.contact_email,
        "valueCents": d.value_cents,
        "currency": d.currency,
        "expectedClose": d.expected_close.map(iso_date),
        "ownerUserId": d.owner_user_id,
        "source": d.source,
        "position": d.position,
        "state": d.state().as_str(),
        "closed": d.is_closed(),
        "lostReason": d.lost_reason,
        "closedAt": d.closed_at.map(iso),
        "createdBy": d.created_by,
        "createdAt": iso(d.created_at),
        "updatedAt": iso(d.updated_at),
    })
}

/// One move a deal made. The row written when the deal was created carries no
/// `fromStageId`, which is how a reader tells "raised here" from "moved here".
pub(crate) fn event_json(e: &StageEvent) -> Value {
    json!({
        "id": e.id.as_str(),
        "dealId": e.deal_id.as_str(),
        "fromStageId": e.from_stage_id.as_ref().map(CrmStageId::as_str),
        "toStageId": e.to_stage_id.as_str(),
        "movedBy": e.moved_by,
        "movedAt": iso(e.moved_at),
    })
}

/// The stored record as writable input — the base a `PATCH` merges onto, so a
/// partial edit replays every field the caller did not mention exactly as it
/// stands.
fn editable(d: &Deal) -> NewDeal {
    NewDeal {
        title: d.title.clone(),
        customer_id: d.customer_id.clone(),
        contact_id: d.contact_id.clone(),
        company_name: d.company_name.clone(),
        contact_name: d.contact_name.clone(),
        contact_email: d.contact_email.clone(),
        value_cents: d.value_cents,
        currency: d.currency.clone(),
        expected_close: d.expected_close,
        owner_user_id: Some(d.owner_user_id.clone()),
        source: d.source.clone(),
    }
}

/// The writable fields of a deal, every one optional.
///
/// The same body serves `POST` (merged onto [`NewDeal::default`] — an unpriced
/// EUR opportunity owned by the acting user) and `PATCH` (merged onto the
/// stored record), so a field can never mean one thing on create and another on
/// edit. `pipelineId` and `stageId` are read from the body on create only: they
/// say where the card is raised, and afterwards moving it is its own route.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DealBody {
    #[serde(default)]
    pipeline_id: Option<String>,
    #[serde(default)]
    stage_id: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default, deserialize_with = "absent_or_null")]
    customer_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "absent_or_null")]
    contact_id: Option<Option<String>>,
    #[serde(default)]
    company_name: Option<String>,
    #[serde(default)]
    contact_name: Option<String>,
    #[serde(default)]
    contact_email: Option<String>,
    #[serde(default)]
    value_cents: Option<i64>,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default, deserialize_with = "absent_or_null")]
    expected_close: Option<Option<String>>,
    #[serde(default, deserialize_with = "absent_or_null")]
    owner_user_id: Option<Option<String>>,
    #[serde(default)]
    source: Option<String>,
    /// The Mail conversation this opportunity is raised from. Create-only;
    /// when stated, deal and link are committed together.
    #[serde(default)]
    thread_id: Option<String>,
}

impl DealBody {
    /// Merges the stated fields onto `base`, leaving the rest as they were.
    ///
    /// # Errors
    /// `422` when `expectedClose` is stated and is not exactly `YYYY-MM-DD` —
    /// the one field this edge parses itself, because a day that a client is
    /// allowed to write as a timestamp is a day that lands on the wrong side of
    /// somebody's midnight.
    fn apply(self, base: NewDeal) -> Result<NewDeal, Problem> {
        let expected_close = match self.expected_close {
            None => base.expected_close,
            Some(None) => None,
            Some(Some(raw)) => match blank_to_none(Some(raw)) {
                None => None,
                Some(text) => Some(parse_iso_date(&text).ok_or_else(|| {
                    Problem::with(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "expectedClose must be a date written YYYY-MM-DD",
                    )
                })?),
            },
        };
        Ok(NewDeal {
            title: self.title.unwrap_or(base.title),
            customer_id: self.customer_id.map_or(base.customer_id, |v| {
                blank_to_none(v).map(BillingCustomerId::new)
            }),
            contact_id: self
                .contact_id
                .map_or(base.contact_id, |v| blank_to_none(v).map(ContactId::new)),
            company_name: self.company_name.unwrap_or(base.company_name),
            contact_name: self.contact_name.unwrap_or(base.contact_name),
            contact_email: self.contact_email.unwrap_or(base.contact_email),
            value_cents: self.value_cents.unwrap_or(base.value_cents),
            currency: self.currency.unwrap_or(base.currency),
            expected_close,
            // An explicit `null` hands the deal back to the acting user, which
            // is what the store reads `None` as.
            owner_user_id: self.owner_user_id.map_or(base.owner_user_id, blank_to_none),
            source: self.source.unwrap_or(base.source),
        })
    }
}

/// Loads one of the tenant's deals, or fails with the `404` an id from another
/// tenant gets.
async fn load(acc: &AccountStore, id: &CrmDealId) -> Result<Deal, Problem> {
    acc.crm_deal(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such deal"))
}

/// Query string of the list route. Every filter is optional and they compose,
/// so "my open deals on the New Business board" is one read.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListQuery {
    #[serde(default)]
    pipeline_id: Option<String>,
    #[serde(default)]
    stage_id: Option<String>,
    #[serde(default)]
    owner_user_id: Option<String>,
    /// `state=open|won|lost`; absent lists everything.
    #[serde(default)]
    state: Option<String>,
}

/// Trims a filter value and treats a blank one as absent — a UI whose select is
/// on "all" sends an empty parameter.
fn stated(raw: Option<&str>) -> Option<&str> {
    raw.map(str::trim).filter(|v| !v.is_empty())
}

/// Reads the state filter, refusing a value that is not one of the three.
fn state_filter(raw: Option<&str>) -> Result<Option<DealState>, Problem> {
    let Some(raw) = stated(raw) else {
        return Ok(None);
    };
    DealState::parse(&raw.to_ascii_lowercase())
        .map(Some)
        .ok_or_else(|| {
            Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "state must be one of open, won, lost",
            )
        })
}

/// Resolves the board and column filters through the tenant's own door,
/// refusing an id this tenant does not have with a `422` that names the
/// parameter.
///
/// Strict rather than silently empty, and a `422` rather than a `404`, because
/// this is a malformed *question* about a list that does exist — and because a
/// board of another tenant answers exactly as one that never existed, so the
/// strictness is not an existence oracle.
async fn scope_filter(acc: &AccountStore, q: &ListQuery) -> Result<DealFilter, Problem> {
    let mut filter = DealFilter {
        owner_user_id: stated(q.owner_user_id.as_deref()).map(str::to_owned),
        state: state_filter(q.state.as_deref())?,
        ..DealFilter::default()
    };
    if let Some(raw) = stated(q.pipeline_id.as_deref()) {
        let id = CrmPipelineId::new(raw.to_owned());
        if acc
            .crm_pipeline(&id)
            .await
            .map_err(map_store_err)?
            .is_none()
        {
            return Err(Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "pipelineId is not a pipeline of this tenant",
            ));
        }
        filter.pipeline_id = Some(id);
    }
    if let Some(raw) = stated(q.stage_id.as_deref()) {
        let id = CrmStageId::new(raw.to_owned());
        if acc.crm_stage(&id).await.map_err(map_store_err)?.is_none() {
            return Err(Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "stageId is not a stage of this tenant",
            ));
        }
        filter.stage_id = Some(id);
    }
    Ok(filter)
}

/// `GET /crm/deals[?pipelineId&stageId&ownerUserId&state]` → `{"deals":[…]}` —
/// the tenant's deals in **board order**, column by column, card by card.
///
/// `ownerUserId` is an exact match and is the one filter that is not resolved
/// first: an owner is a user of the tenant, not a CRM record, and the id of one
/// who owns nothing legitimately answers with an empty list.
pub async fn list_deals(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let filter = scope_filter(&account.acc, &q).await?;
    let deals = account
        .acc
        .crm_deals(&filter)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "deals": deals.iter().map(deal_json).collect::<Vec<_>>(),
    })))
}

/// `POST /crm/deals` `{pipelineId, stageId, title, …}` → `{"deal":{…}}` — raise
/// a card in a column of a board.
///
/// `pipelineId` and `stageId` are required: a deal that names no board is not a
/// deal, and guessing the tenant's first board for it would put work somewhere
/// nobody asked for. A deal is always created **open**, whatever the column's
/// flags — closing is a move, and a deal that was never worked was never won.
pub async fn create_deal(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: DealBody = parse_body(&body)?;
    let pipeline = required_id(req.pipeline_id.as_deref(), "pipelineId")?;
    let stage = required_id(req.stage_id.as_deref(), "stageId")?;
    let thread = stated(req.thread_id.as_deref()).map(|value| ThreadId::new(value.to_owned()));
    let input = req.apply(NewDeal::default())?;
    let pipeline = CrmPipelineId::new(pipeline);
    let stage = CrmStageId::new(stage);
    let id = match thread {
        Some(thread) => {
            account
                .acc
                .create_crm_deal_from_thread(&pipeline, &stage, &input, &thread)
                .await
        }
        None => account.acc.create_crm_deal(&pipeline, &stage, &input).await,
    }
    .map_err(map_store_err)?;
    let deal = load(&account.acc, &id).await?;
    Ok(Json(json!({ "deal": deal_json(&deal) })))
}

/// Reads an id a request must state, refusing a blank one with the `422` the
/// design note's error map publishes.
fn required_id(raw: Option<&str>, field: &str) -> Result<String, Problem> {
    stated(raw).map(str::to_owned).ok_or_else(|| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{field} is required"),
        )
    })
}

/// `GET /crm/deals/{id}` → `{"deal":{…}}`.
pub async fn get_deal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let deal = load(&account.acc, &CrmDealId::new(id)).await?;
    Ok(Json(json!({ "deal": deal_json(&deal) })))
}

/// `PATCH /crm/deals/{id}` `{title?, valueCents?, …}` → `{"deal":{…}}` — merge
/// the stated fields onto the stored record.
///
/// It cannot move the card, reposition it, or close it: those are
/// `POST /crm/deals/{id}/stage`, so a stale edit form can never win a deal.
pub async fn update_deal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: DealBody = parse_body(&body)?;
    let id = CrmDealId::new(id);
    let stored = load(&account.acc, &id).await?;
    let input = req.apply(editable(&stored))?;
    account
        .acc
        .update_crm_deal(&id, &input)
        .await
        .map_err(map_store_err)?;
    let deal = load(&account.acc, &id).await?;
    Ok(Json(json!({ "deal": deal_json(&deal) })))
}

/// The body of the move route.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoveBody {
    #[serde(default)]
    stage_id: Option<String>,
    /// Where in the target column; absent appends to the end of it.
    #[serde(default)]
    position: Option<f64>,
    /// Why the deal was lost. Required when the target column is flagged
    /// `isLost`, refused otherwise.
    #[serde(default)]
    lost_reason: Option<String>,
}

/// `POST /crm/deals/{id}/stage` `{stageId, position?, lostReason?}` →
/// `{"deal":{…}}` — move a card, and write the one history row that says so.
///
/// The column must be on the deal's **own** board (`422` otherwise: a board is
/// not a place to lose a deal into another team's funnel) and not archived.
/// Landing in a flagged column writes the closing snapshot in the same
/// transaction; a losing column demands a reason and every other column refuses
/// one. Moving a closed deal back to an open column reopens it and leaves both
/// history rows standing.
pub async fn move_deal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: MoveBody = parse_body(&body)?;
    let stage = required_id(req.stage_id.as_deref(), "stageId")?;
    let id = CrmDealId::new(id);
    let mv = StageMove {
        stage_id: CrmStageId::new(stage),
        position: req.position,
        lost_reason: req.lost_reason,
    };
    account
        .acc
        .move_crm_deal(&id, &mv)
        .await
        .map_err(map_store_err)?;
    let deal = load(&account.acc, &id).await?;
    Ok(Json(json!({ "deal": deal_json(&deal) })))
}

/// `GET /crm/deals/{id}/history` → `{"events":[…]}` — one deal's stage history,
/// oldest first, starting with the row written when it was raised.
///
/// A deal that is not this tenant's is the same `404` an id that never existed
/// gets, never an empty list.
pub async fn deal_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let events = account
        .acc
        .crm_deal_history(&CrmDealId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "events": events.iter().map(event_json).collect::<Vec<_>>(),
    })))
}

/// `DELETE /crm/deals/{id}` → `{"deleted":true}` — a deal raised by mistake
/// leaves no trace, and its history goes with it.
///
/// The one CRM record that is deleted rather than archived: it is our own
/// private note of an opportunity, not a document anybody else holds. A deal
/// that was really worked is *lost*, which is a move.
pub async fn delete_deal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_crm_deal(&CrmDealId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "deleted": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(json: Value) -> DealBody {
        serde_json::from_value(json).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    fn merged(json: Value, base: NewDeal) -> NewDeal {
        body(json)
            .apply(base)
            .unwrap_or_else(|e| panic!("rejected: {:?}", e.detail))
    }

    fn stored() -> NewDeal {
        NewDeal {
            title: "Renewal — Acme GmbH".to_owned(),
            customer_id: Some(BillingCustomerId::new("cus_1")),
            contact_id: Some(ContactId::new("con_1")),
            company_name: "Acme GmbH".to_owned(),
            contact_name: "Ada".to_owned(),
            contact_email: "ada@acme.test".to_owned(),
            value_cents: 250_000,
            currency: "EUR".to_owned(),
            expected_close: parse_iso_date("2026-09-30"),
            owner_user_id: Some("usr_1".to_owned()),
            source: "Referral".to_owned(),
        }
    }

    #[test]
    fn an_empty_patch_changes_nothing() {
        let d = merged(json!({}), stored());
        assert_eq!(d.title, "Renewal — Acme GmbH");
        assert_eq!(d.value_cents, 250_000);
        assert_eq!(
            d.customer_id.map(|c| c.as_str().to_owned()),
            Some("cus_1".to_owned())
        );
        assert_eq!(d.expected_close, parse_iso_date("2026-09-30"));
        assert_eq!(d.owner_user_id.as_deref(), Some("usr_1"));
    }

    #[test]
    fn zero_is_a_stated_value_not_an_absent_one() {
        // An opportunity whose price was withdrawn is worth nothing, not what
        // it was worth yesterday.
        let d = merged(json!({ "valueCents": 0 }), stored());
        assert_eq!(d.value_cents, 0);
    }

    #[test]
    fn a_value_with_a_decimal_point_is_refused_never_rounded() {
        assert!(serde_json::from_value::<DealBody>(json!({"valueCents": 1250.5})).is_err());
        assert!(serde_json::from_value::<DealBody>(json!({"valueCents": "1250"})).is_err());
    }

    #[test]
    fn a_nullable_link_can_be_cleared_and_a_blank_string_means_null() {
        let cleared = merged(json!({ "customerId": null, "contactId": null }), stored());
        assert!(cleared.customer_id.is_none() && cleared.contact_id.is_none());
        let blanked = merged(json!({ "customerId": "  " }), stored());
        assert!(blanked.customer_id.is_none());
        let set = merged(json!({ "customerId": "cus_2" }), stored());
        assert_eq!(
            set.customer_id.map(|c| c.as_str().to_owned()),
            Some("cus_2".to_owned())
        );
    }

    #[test]
    fn clearing_the_owner_hands_the_deal_to_the_acting_user() {
        // `None` is what the store reads as "the caller owns it", so an
        // explicit null is how a UI says "mine".
        let d = merged(json!({ "ownerUserId": null }), stored());
        assert!(d.owner_user_id.is_none());
        let named = merged(json!({ "ownerUserId": "usr_2" }), stored());
        assert_eq!(named.owner_user_id.as_deref(), Some("usr_2"));
    }

    #[test]
    fn an_expected_close_is_a_plain_day_or_a_refusal() {
        let set = merged(json!({ "expectedClose": "2026-12-01" }), stored());
        assert_eq!(set.expected_close, parse_iso_date("2026-12-01"));
        let cleared = merged(json!({ "expectedClose": null }), stored());
        assert_eq!(cleared.expected_close, None);
        let blanked = merged(json!({ "expectedClose": "" }), stored());
        assert_eq!(blanked.expected_close, None);
        for bad in ["2026-12-01T00:00:00Z", "20261201", "01/12/2026", "soon"] {
            let problem = body(json!({ "expectedClose": bad }))
                .apply(stored())
                .err()
                .unwrap_or_else(|| panic!("accepted {bad}"));
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
        }
    }

    #[test]
    fn a_patch_cannot_move_close_or_reposition_a_deal() {
        // Every one of these is ignored exactly as an unknown field is; the
        // move route is the only door to any of them.
        let d = merged(
            json!({
                "stageId": "stg_won",
                "pipelineId": "pip_2",
                "position": 9.5,
                "state": "won",
                "lostReason": "price",
                "closedAt": "2026-01-01T00:00:00Z",
                "title": "Renewal — Acme GmbH",
            }),
            stored(),
        );
        assert_eq!(d.title, "Renewal — Acme GmbH");
        assert_eq!(d.value_cents, 250_000);
    }

    #[test]
    fn create_starts_from_an_unpriced_opportunity_owned_by_the_caller() {
        let d = merged(json!({ "title": "New lead" }), NewDeal::default());
        assert_eq!(d.title, "New lead");
        assert_eq!(d.value_cents, 0);
        assert_eq!(d.currency, "EUR");
        assert!(d.owner_user_id.is_none(), "the acting user owns it");
    }

    #[test]
    fn a_state_filter_is_exact_or_a_422() {
        assert_eq!(state_filter(None).ok(), Some(None));
        assert_eq!(state_filter(Some("  ")).ok(), Some(None));
        assert_eq!(state_filter(Some("won")).ok(), Some(Some(DealState::Won)));
        assert_eq!(state_filter(Some(" WON ")).ok(), Some(Some(DealState::Won)));
        for bad in ["winning", "closed", "all", "0"] {
            let problem = state_filter(Some(bad))
                .err()
                .unwrap_or_else(|| panic!("accepted {bad}"));
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
        }
    }

    #[test]
    fn an_id_a_request_must_state_is_never_blank() {
        assert_eq!(
            required_id(Some(" pip_1 "), "pipelineId").ok(),
            Some("pip_1".to_owned())
        );
        for absent in [None, Some(""), Some("   ")] {
            let problem = required_id(absent, "stageId")
                .err()
                .unwrap_or_else(|| panic!("accepted {absent:?}"));
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(problem.detail.as_deref(), Some("stageId is required"));
        }
    }

    #[test]
    fn a_move_states_a_stage_and_may_state_the_rest() {
        let full: MoveBody = serde_json::from_value(
            json!({ "stageId": "stg_2", "position": 1.5, "lostReason": "Price" }),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(full.stage_id.as_deref(), Some("stg_2"));
        assert_eq!(full.position, Some(1.5));
        assert_eq!(full.lost_reason.as_deref(), Some("Price"));
        let bare: MoveBody =
            serde_json::from_value(json!({ "stageId": "stg_2" })).unwrap_or_else(|e| panic!("{e}"));
        assert!(bare.position.is_none() && bare.lost_reason.is_none());
    }
}
