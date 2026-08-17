//! `/campaigns/consent` (ADR 0044 §2, wave C1) — the evidence that a person
//! agreed to be mailed, recorded and read back.
//!
//! **Append-only over the wire as well as in the table.** There is no `PATCH`
//! and no `DELETE` here, and their absence is the feature: a consent record is
//! what the tenant will show when somebody complains, and evidence that can be
//! edited afterwards is not evidence. A statement that turns out to be wrong is
//! corrected by recording the truth, which leaves both rows and the dates they
//! were made on.
//!
//! **The history is read per address, never listed tenant-wide.** "How do we
//! know this person agreed" is a question about one person, and a route that
//! dumped every consent record would be an export of who the tenant is allowed
//! to mail — a file that leaves the building. The audience already answers "who
//! may be mailed" with the evidence id beside each person, which is the shape a
//! screen actually needs.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::{CampaignConsent, ConsentSource, NewCampaignConsent};

use crate::billing::{blank_to_none, iso, map_store_err, parse_body, parse_rfc3339};
use crate::campaigns::{address_of, unprocessable};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// One recorded act of consent as JSON — the whole record, statement included,
/// because this route *is* the answer to "how do we know".
fn consent_json(consent: &CampaignConsent) -> Value {
    json!({
        "id": consent.id.as_str(),
        "address": consent.address,
        "source": consent.source.as_str(),
        "sourceRef": consent.source_ref,
        "statement": consent.statement,
        "recordedBy": consent.recorded_by.as_str(),
        "occurredAt": iso(consent.occurred_at),
        "recordedAt": iso(consent.recorded_at),
    })
}

/// What a caller states when recording consent.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConsentBody {
    #[serde(default)]
    address: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    source_ref: Option<String>,
    #[serde(default)]
    statement: Option<String>,
    /// When the person agreed, RFC 3339. Absent means now — right for a form
    /// submitted this second, wrong for an import, which knows its own date.
    #[serde(default)]
    occurred_at: Option<String>,
}

/// `POST /campaigns/consent` `{address, source, sourceRef, statement,
/// occurredAt}` → `{"consent":{…}}` — records that somebody agreed, with the
/// provenance that makes it evidence.
///
/// `source` is deliberately wider than the audience's three: `import` and
/// `manual` exist because ADR 0044 §2 calls imported lists the dangerous path,
/// and a path that cannot be named as itself cannot be treated as such. An
/// `import` or a `siteForm` that cannot say *which one* is a `422`, not a
/// filled-in column.
///
/// The recorder is the calling account and there is no field for it: a consent
/// record says which colleague's workspace made the claim, and no request can
/// attribute one to somebody else.
pub async fn record_consent(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let request: ConsentBody = parse_body(&body)?;

    let address = address_of(request.address.as_deref().unwrap_or_default())?;
    let source_token = request
        .source
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| {
            unprocessable(
                "source says what kind of thing the agreement came from: \
                 billing_customer, crm_deal, site_form, import or manual",
            )
        })?;
    let source = ConsentSource::parse(&source_token.to_ascii_lowercase()).ok_or_else(|| {
        unprocessable("source must be one of billing_customer, crm_deal, site_form, import, manual")
    })?;
    let occurred_at = match request
        .occurred_at
        .as_deref()
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
    {
        None => None,
        Some(raw) => Some(parse_rfc3339(raw).ok_or_else(|| {
            unprocessable("occurredAt is a full RFC 3339 timestamp, e.g. 2026-03-04T10:00:00Z")
        })?),
    };
    let source_ref = blank_to_none(request.source_ref);
    let statement = request.statement.unwrap_or_default();

    let consent = account
        .acc
        .record_campaign_consent(&NewCampaignConsent {
            address: &address,
            source,
            source_ref: source_ref.as_deref(),
            statement: statement.trim(),
            occurred_at,
        })
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "consent": consent_json(&consent) })))
}

/// `GET /campaigns/consent/{address}` → `{"consent":[…]}` — one person's
/// provenance, freshest first.
///
/// An empty array is a complete answer, not a `404`: this tenant holds no
/// evidence for that address, which is exactly why they are not a recipient.
/// Answering `404` would make the route an oracle for whether an address is
/// known, which is a fact about a person rather than about a URL.
pub async fn consent_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(address): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let address = address_of(&address)?;
    let history = account
        .acc
        .campaign_consent_for(&address)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "consent": history.iter().map(consent_json).collect::<Vec<_>>(),
    })))
}
