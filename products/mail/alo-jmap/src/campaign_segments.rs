//! `/campaigns/segments` (ADR 0044, wave C1) — the saved question, and nothing
//! else.
//!
//! **A segment stores conditions, never people.** There is no membership list
//! to fetch and no cached count to invalidate: `GET /campaigns/segments/{id}`
//! answers with the conditions, and the caller counts them through
//! `GET /campaigns/audience/tally` with those same conditions on the URL. So a
//! saved segment and a half-typed one are counted by one code path, and consent
//! and suppression apply at the moment of asking rather than at the moment of
//! saving — which is how somebody who unsubscribed on Monday would otherwise be
//! mailed on Tuesday.
//!
//! **The edit is whole-record, not a `PATCH`.** A segment is one sentence, and
//! a partial update is how "customers in Belgium who have not bought" turns
//! into "customers in Belgium" without anybody deciding it should — a bigger
//! send arrived at by omission. The store's `update_campaign_segment` takes the
//! whole record for the same reason, so `PUT` here would be the second half of
//! one rule; `PATCH` is offered instead and merges onto the **stored** record,
//! which means a caller that states nothing changes nothing and a caller that
//! states `conditions` replaces them whole.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::{
    AccountStore, CampaignSegment, CampaignSegmentId, NewCampaignSegment, PurchaseCondition,
    PurchaseWindow, SegmentConditions,
};

use crate::billing::{iso, map_store_err, parse_body};
use crate::campaigns::{conditions_json, unprocessable};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// A saved segment as JSON.
///
/// `createdBy` is who to ask what the question meant — never a claim that
/// anybody the segment selects agreed to anything. That is consent, and it is
/// carried per person by the audience.
fn segment_json(segment: &CampaignSegment) -> Value {
    json!({
        "id": segment.id.as_str(),
        "name": segment.name,
        "conditions": conditions_json(&segment.conditions),
        "createdBy": segment.created_by.as_str(),
        "createdAt": iso(segment.created_at),
        "updatedAt": iso(segment.updated_at),
    })
}

/// The purchase condition as a client writes it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PurchaseBody {
    #[serde(default)]
    condition: Option<String>,
    #[serde(default)]
    within_days: Option<i32>,
}

/// The conditions as a client writes them — the mirror of
/// [`conditions_json`](crate::campaigns::conditions_json), so a segment read
/// back and posted again is the same segment.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConditionsBody {
    #[serde(default)]
    countries: Option<Vec<String>>,
    /// `null` or absent is **no purchase condition**; an object states one.
    #[serde(default)]
    purchase: Option<PurchaseBody>,
}

impl ConditionsBody {
    /// The conditions this body states.
    ///
    /// # Errors
    /// `422` when the purchase condition is not one this build knows. Whether a
    /// country is a country and whether a period is in range are the store's
    /// rules, applied by the same validation the tally goes through — one
    /// answer to "is BE a country", not two that can drift.
    fn conditions(self) -> Result<SegmentConditions, Problem> {
        let purchase = match self.purchase {
            None => None,
            Some(body) => {
                let token = body
                    .condition
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        unprocessable(
                            "a purchase condition says bought or not_bought; omit purchase \
                             entirely to ask about everybody",
                        )
                    })?;
                Some(PurchaseWindow {
                    condition: PurchaseCondition::parse(&token.to_ascii_lowercase()).ok_or_else(
                        || unprocessable("purchase.condition must be bought or not_bought"),
                    )?,
                    within_days: body.within_days,
                })
            }
        };
        Ok(SegmentConditions {
            countries: self.countries.unwrap_or_default(),
            purchase,
        })
    }
}

/// The writable fields of a segment.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SegmentBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    conditions: Option<ConditionsBody>,
}

impl SegmentBody {
    /// The name and conditions this request means, given what is already
    /// stored (`None` on create — a segment with no conditions asks about the
    /// whole audience, which is a legitimate first draft).
    ///
    /// # Errors
    /// `422` when a create states no name, or when the conditions are not ones
    /// this build can read.
    fn apply(
        self,
        stored: Option<&CampaignSegment>,
    ) -> Result<(String, SegmentConditions), Problem> {
        let name = match self
            .name
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            Some(name) => name.to_owned(),
            None => stored
                .map(|segment| segment.name.clone())
                .ok_or_else(|| unprocessable("name is required"))?,
        };
        let conditions = match self.conditions {
            Some(body) => body.conditions()?,
            None => stored
                .map(|segment| segment.conditions.clone())
                .unwrap_or_default(),
        };
        Ok((name, conditions))
    }
}

/// The segment behind an id, or the `404` an absent one and another tenant's
/// one both get.
async fn load(account: &AccountStore, id: &CampaignSegmentId) -> Result<CampaignSegment, Problem> {
    account
        .campaign_segment(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "not found"))
}

/// `GET /campaigns/segments` → `{"segments":[…]}` — this tenant's saved
/// questions, by name.
pub async fn list_segments(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let segments = account
        .acc
        .campaign_segments(alo_store::SEGMENT_PAGE_MAX)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "segments": segments.iter().map(segment_json).collect::<Vec<_>>(),
    })))
}

/// `POST /campaigns/segments` `{name, conditions}` → `{"segment":{…}}`.
///
/// A duplicate name is a `409` naming the one uniqueness rule this table has,
/// because "send it to the Belgian customers" must name one thing.
pub async fn create_segment(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let request: SegmentBody = parse_body(&body)?;
    let (name, conditions) = request.apply(None)?;
    let segment = account
        .acc
        .create_campaign_segment(&NewCampaignSegment {
            name: &name,
            conditions,
        })
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "segment": segment_json(&segment) })))
}

/// `GET /campaigns/segments/{id}` → `{"segment":{…}}` — the question, not the
/// people. The count comes from `GET /campaigns/audience/tally` with these
/// conditions.
pub async fn get_segment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let segment = load(&account.acc, &CampaignSegmentId::new(id)).await?;
    Ok(Json(json!({ "segment": segment_json(&segment) })))
}

/// `PATCH /campaigns/segments/{id}` `{name?, conditions?}` →
/// `{"segment":{…}}` — rename the question, or rewrite it.
///
/// `conditions` is replaced whole when stated: a segment is one sentence, and
/// merging half of one produces a question nobody wrote.
pub async fn update_segment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = CampaignSegmentId::new(id);
    let stored = load(&account.acc, &id).await?;
    let request: SegmentBody = parse_body(&body)?;
    let (name, conditions) = request.apply(Some(&stored))?;
    let segment = account
        .acc
        .update_campaign_segment(
            &id,
            &NewCampaignSegment {
                name: &name,
                conditions,
            },
        )
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "segment": segment_json(&segment) })))
}

/// `DELETE /campaigns/segments/{id}` → `{"deleted":true}`.
///
/// Deleting a segment deletes a question, never evidence: consent records and
/// suppressions live in their own tables and are untouched, so a tenant tidying
/// up its segments cannot lose the reason somebody may or may not be mailed.
pub async fn delete_segment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_campaign_segment(&CampaignSegmentId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "deleted": true })))
}
