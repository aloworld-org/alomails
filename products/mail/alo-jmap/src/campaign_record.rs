//! `/campaigns/campaigns` (ADR 0044, wave C3.1) — the letter itself: subject,
//! preview text, and a body in the Docs block model.
//!
//! **Nothing on this surface sends.** There is no `POST …/send`, no schedule and
//! no recipient list, and their absence is the wave's boundary rather than an
//! unbuilt screen: ADR 0044 §1 requires a second egress IP for campaign mail,
//! and that is a purchase. What this surface does is let a colleague write the
//! mail and read it back byte for byte.
//!
//! **Why the route says `campaigns` twice.** `/campaigns/*` is the product, and
//! every sibling under it is a static prefix — `audience`, `consent`,
//! `segments`, `suppressions`. Hanging the record on `/campaigns/{id}` instead
//! would make every future sibling a possible collision with a generated id, and
//! the collision would be silent: a static segment wins the match, so the day
//! somebody adds `/campaigns/reports` is the day one campaign becomes
//! unreachable with no error anywhere. The clumsy path is the honest one.
//!
//! **The body is the Docs body.** `content` travels exactly as
//! [`alo_store::CampaignContent`] stores it —
//! `{"schema_version": 1, "blocks": [ … ]}`, with each block in the shape the
//! Docs editor writes (`{"type":"paragraph","id":…,"text":…}`). It is the one
//! camelCase exception on this surface, for the same reason the sites API keeps
//! `schema_version`: the envelope is a stored document format, and a wire name
//! that differed from the stored one would be a translation layer nobody asked
//! for between an editor and its own output.
//!
//! **The edit is whole-record, stated field by stated field.** `PATCH` merges
//! onto the **stored** campaign: a caller that states nothing changes nothing,
//! and a caller that states `content` replaces the body whole. Merging *into* a
//! body — appending blocks, patching one by id — is how a mail loses its last
//! paragraph without anybody deciding it should, so it is not offered.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};

use alo_store::{
    AccountStore, Campaign, CampaignContent, CampaignId, CampaignSummary, NewCampaign,
};

use crate::billing::{iso, map_store_err, parse_body};
use crate::campaigns::unprocessable;
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// A whole campaign as JSON, body included.
fn campaign_json(campaign: &Campaign) -> Value {
    json!({
        "id": campaign.id.as_str(),
        "subject": campaign.subject,
        "preheader": campaign.preheader,
        "topic": campaign.topic,
        // Serialising a validated body cannot fail (it is strings and numbers
        // all the way down), and a `null` here would be visibly wrong rather
        // than a plausible empty body — which is the point of not substituting
        // one.
        "content": serde_json::to_value(&campaign.content).unwrap_or(Value::Null),
        "createdBy": campaign.created_by.as_str(),
        "createdAt": iso(campaign.created_at),
        "updatedAt": iso(campaign.updated_at),
    })
}

/// A campaign in a list — everything except the body, plus how much of a body
/// there is.
///
/// The body is omitted rather than truncated: half a body is a thing a client
/// can accidentally save back over the whole one.
fn summary_json(campaign: &CampaignSummary) -> Value {
    json!({
        "id": campaign.id.as_str(),
        "subject": campaign.subject,
        "preheader": campaign.preheader,
        "topic": campaign.topic,
        "blocks": campaign.blocks,
        "createdBy": campaign.created_by.as_str(),
        "createdAt": iso(campaign.created_at),
        "updatedAt": iso(campaign.updated_at),
    })
}

/// Reads a field that must tell "absent" and "null" apart.
///
/// `preheader` is the only one that needs it: **absent means leave the preview
/// text alone, `null` (or `""`) means remove it.** With a plain `Option`, serde
/// folds both into `None`, and a client clearing the field the obvious way
/// would silently keep the old text on the next send.
fn stated_field<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// The writable fields of a campaign.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CampaignBody {
    #[serde(default)]
    subject: Option<String>,
    /// Absent leaves it; `null` or `""` removes it; a string sets it.
    #[serde(default, deserialize_with = "stated_field")]
    preheader: Option<Option<String>>,
    #[serde(default)]
    topic: Option<String>,
    /// The whole envelope, replaced whole when stated.
    #[serde(default)]
    content: Option<Value>,
}

/// The campaign a request means, given what is already stored.
struct Stated {
    subject: String,
    preheader: Option<String>,
    topic: String,
    content: CampaignContent,
}

impl CampaignBody {
    /// What this request means (`None` stored = a create).
    ///
    /// # Errors
    /// `422` when a create states no subject or no topic, or when the content is
    /// not a body this build can read — the store's own rules, applied by the
    /// store's own validator, so the composer and a script get one answer to
    /// "is this a campaign" rather than two that can drift.
    fn apply(self, stored: Option<&Campaign>) -> Result<Stated, Problem> {
        let subject = match stated(self.subject) {
            Some(subject) => subject,
            None => stored
                .map(|campaign| campaign.subject.clone())
                .ok_or_else(|| {
                    unprocessable("subject is required — it is what arrives in the inbox")
                })?,
        };
        let topic = match stated(self.topic) {
            Some(topic) => topic,
            None => stored
                .map(|campaign| campaign.topic.clone())
                .ok_or_else(|| {
                    unprocessable(
                        "topic is required — it is the kind of mail a recipient can stop without \
                     stopping all of it",
                    )
                })?,
        };
        let preheader = match self.preheader {
            Some(stated) => stated.filter(|value| !value.trim().is_empty()),
            None => stored.and_then(|campaign| campaign.preheader.clone()),
        };
        let content = match self.content {
            Some(value) => CampaignContent::from_value(value).map_err(map_store_err)?,
            None => stored
                .map(|campaign| campaign.content.clone())
                .unwrap_or_default(),
        };
        Ok(Stated {
            subject,
            preheader,
            topic,
            content,
        })
    }
}

/// A stated string: trimmed, and blank treated as not stated.
fn stated(raw: Option<String>) -> Option<String> {
    raw.map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// The campaign behind an id, or the `404` an absent one and another tenant's
/// one both get.
async fn load(account: &AccountStore, id: &CampaignId) -> Result<Campaign, Problem> {
    account
        .campaign(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "not found"))
}

/// `GET /campaigns/campaigns` → `{"campaigns":[…]}` — what this tenant has
/// written, newest first, without the bodies.
pub async fn list_campaigns(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let campaigns = account
        .acc
        .campaigns(alo_store::CAMPAIGN_PAGE_MAX)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "campaigns": campaigns.iter().map(summary_json).collect::<Vec<_>>(),
    })))
}

/// `POST /campaigns/campaigns` `{subject, topic, preheader?, content?}` →
/// `{"campaign":{…}}`.
///
/// `content` may be omitted: a campaign named and not yet written is a real
/// state, and the composer opens on it.
pub async fn create_campaign(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let request: CampaignBody = parse_body(&body)?;
    let stated = request.apply(None)?;
    let campaign = account
        .acc
        .create_campaign(&NewCampaign {
            subject: &stated.subject,
            preheader: stated.preheader.as_deref(),
            topic: &stated.topic,
            content: stated.content,
        })
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "campaign": campaign_json(&campaign) })))
}

/// `GET /campaigns/campaigns/{id}` → `{"campaign":{…}}` — the whole letter,
/// body included.
pub async fn get_campaign(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let campaign = load(&account.acc, &CampaignId::new(id)).await?;
    Ok(Json(json!({ "campaign": campaign_json(&campaign) })))
}

/// `PATCH /campaigns/campaigns/{id}` `{subject?, preheader?, topic?, content?}`
/// → `{"campaign":{…}}`.
///
/// Every stated field replaces the stored one whole; `content` replaces the
/// body whole (see the module docs).
pub async fn update_campaign(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = CampaignId::new(id);
    let stored = load(&account.acc, &id).await?;
    let request: CampaignBody = parse_body(&body)?;
    let stated = request.apply(Some(&stored))?;
    let campaign = account
        .acc
        .update_campaign(
            &id,
            &NewCampaign {
                subject: &stated.subject,
                preheader: stated.preheader.as_deref(),
                topic: &stated.topic,
                content: stated.content,
            },
        )
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "campaign": campaign_json(&campaign) })))
}

/// `DELETE /campaigns/campaigns/{id}` → `{"deleted":true}`.
///
/// Deleting a campaign deletes a letter, never evidence: consent records,
/// suppressions and topic opt-outs are separate tables and are untouched.
pub async fn delete_campaign(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_campaign(&CampaignId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "deleted": true })))
}

#[cfg(test)]
mod tests {
    use super::CampaignBody;
    use serde_json::json;

    fn body(value: serde_json::Value) -> Option<CampaignBody> {
        serde_json::from_value(value).ok()
    }

    #[test]
    fn a_preheader_can_be_removed_and_left_alone_and_the_two_are_different() {
        // The whole reason `stated_field` exists: with a plain Option, both of
        // these read as "not stated", and a client clearing the preview text
        // would find it still there on the next send.
        assert_eq!(
            body(json!({ "preheader": null })).map(|b| b.preheader),
            Some(Some(None)),
            "null is a removal"
        );
        assert_eq!(
            body(json!({ "subject": "Spring" })).map(|b| b.preheader),
            Some(None),
            "an absent field leaves the stored one alone"
        );
        assert_eq!(
            body(json!({ "preheader": "Ten per cent off" })).map(|b| b.preheader),
            Some(Some(Some("Ten per cent off".to_owned()))),
        );
    }
}
