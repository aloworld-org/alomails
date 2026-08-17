//! `GET /campaigns/audience` and `GET /campaigns/audience/tally` (ADR 0044,
//! wave C1) — who this tenant could reach, and the number with its exclusions
//! beside it.
//!
//! **The whole surface is two reads of one question.** The conditions on the
//! URL are a segment ([`crate::campaigns`]), saved or not; `/audience` lists
//! the people it selects and `/audience/tally` counts them. The screen asks
//! both as the question is refined, which is why the tally takes conditions
//! rather than a saved id.
//!
//! Two shapes here are the item's rule rather than a convenience:
//!
//! - **Everybody the conditions selected is listed, including the people who
//!   will not be mailed**, each carrying `exclusionReason`. A list that quietly
//!   dropped them would make the count unauditable: somebody who unsubscribed
//!   is usually still a customer the tenant invoices, and "why is it 412 and
//!   not 500" is the question this screen exists to answer.
//! - **`mailable` is computed here from the same precedence the store's tally
//!   applies** — suppression outranks a missing consent record — so a client
//!   never re-derives it and the list and the count cannot disagree about one
//!   person. It is a read-only convenience: the rule that decides a send lives
//!   in SQL, and nothing a client sends can widen it.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use serde_json::{Value, json};

use alo_store::{
    AudienceMember, AudienceSource, ConsentEvidence, ExclusionReason, SegmentTally,
    SuppressionEvidence,
};

use crate::billing::{iso, map_store_err};
use crate::campaigns::{ConditionsQuery, conditions_from, page_from};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// The consent behind a person, as the audience carries it — the record's
/// handle and when they agreed, never the statement.
///
/// The statement is the tenant's own wording and can be long; the audience is a
/// page of up to five hundred people. A caller that wants it reads
/// `GET /campaigns/consent/{address}`, which is the question "how do we know"
/// asked about one person.
fn consent_json(evidence: &ConsentEvidence) -> Value {
    json!({
        "recordId": evidence.record.as_str(),
        "source": evidence.source.as_str(),
        "occurredAt": iso(evidence.occurred_at),
    })
}

/// Why somebody may never be mailed again, as the audience carries it.
fn suppression_json(evidence: &SuppressionEvidence) -> Value {
    json!({
        "recordId": evidence.record.as_str(),
        "reason": evidence.reason.as_str(),
        "occurredAt": iso(evidence.occurred_at),
    })
}

/// One person as JSON.
///
/// `sources` names every kind of record that holds this address, because a
/// person who is a customer, a deal contact and a form submitter is one row
/// here and three rows to whoever goes looking for them.
pub(crate) fn member_json(member: &AudienceMember) -> Value {
    let exclusion = ExclusionReason::for_member(member);
    json!({
        "address": member.address,
        "name": member.name,
        "country": member.country,
        "sources": member
            .sources
            .iter()
            .map(AudienceSource::as_str)
            .collect::<Vec<_>>(),
        "firstSeenAt": iso(member.first_seen_at),
        "lastSeenAt": iso(member.last_seen_at),
        "consent": member.consent.as_ref().map(consent_json),
        "suppression": member.suppression.as_ref().map(suppression_json),
        "mailable": exclusion.is_none(),
        "exclusionReason": exclusion.map(|reason| reason.token()),
    })
}

/// A tally as JSON: the honest number, everybody it left out, and why.
///
/// `matched` is emitted although it is the sum of the rest, because the screen
/// states "412 of 500 will be mailed" and a client computing the denominator
/// itself is a client that can compute it differently. There is still no stored
/// total — [`SegmentTally::matched`] adds the parts up on every read, so the
/// number and its explanation cannot drift.
pub(crate) fn tally_json(tally: &SegmentTally) -> Value {
    json!({
        "mailable": tally.mailable,
        "matched": tally.matched(),
        "excluded": tally
            .excluded
            .iter()
            .map(|exclusion| json!({
                "reason": exclusion.reason.token(),
                "people": exclusion.people,
            }))
            .collect::<Vec<_>>(),
    })
}

/// The conditions and the page, in one extractor.
///
/// Flat rather than two nested structs: axum allows a handler exactly one
/// `Query`, and `serde_urlencoded` does not support `#[serde(flatten)]` — a
/// nested shape deserialises to an error rather than to a question. The parsing
/// itself is still [`crate::campaigns`]'s, so this route and the tally route
/// cannot disagree about what `purchase=bought` means.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudienceQuery {
    #[serde(default)]
    countries: Option<String>,
    #[serde(default)]
    purchase: Option<String>,
    #[serde(default)]
    within_days: Option<String>,
    #[serde(default)]
    after: Option<String>,
    #[serde(default)]
    limit: Option<String>,
}

/// `GET /campaigns/audience[?countries&purchase&withinDays&after&limit]` →
/// `{"people":[…]}` — one page of the people the conditions select, mailable or
/// not, in address order.
///
/// With no conditions this is the whole audience: the three tenant-wide sources
/// (billing customers, CRM deal contacts, site form submissions) with one row
/// per person however many of them hold that address. The per-user address book
/// is not among them and cannot become one — that is a property of the store's
/// SQL, tested there.
pub async fn list_audience(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AudienceQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let conditions = conditions_from(
        query.countries.as_deref(),
        query.purchase.as_deref(),
        query.within_days.as_deref(),
    )?;
    let page = page_from(query.after.as_deref(), query.limit.as_deref())?;
    let people = account
        .acc
        .campaign_segment_members(&conditions, &page)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "people": people.iter().map(member_json).collect::<Vec<_>>(),
    })))
}

/// `GET /campaigns/audience/tally[?countries&purchase&withinDays]` →
/// `{"tally":{…}}` — how many people the question reaches, and who it leaves
/// out with the reason.
///
/// The read the screen makes on every keystroke of a segment being refined, and
/// the reason [`campaign_segment_tally`](alo_store::AccountStore::campaign_segment_tally)
/// takes conditions rather than a saved id.
pub async fn audience_tally(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ConditionsQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let conditions = query.conditions()?;
    let tally = account
        .acc
        .campaign_segment_tally(&conditions)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "tally": tally_json(&tally) })))
}
