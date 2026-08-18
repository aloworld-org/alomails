//! `/campaigns/campaigns/{id}/preview`, `/test`, and `/campaigns/merge-fields`
//! (ADR 0044, wave C3.6) — reading the letter as one person will receive it,
//! and putting a copy of it in your own Drafts.
//!
//! **Still nothing here sends.** The seed test writes a **draft** into the
//! caller's own Drafts folder and stops, which is the same rule every other
//! server-composed message in this product follows (`crate::drafts`, ADR 0034):
//! anything alo writes on a user's behalf lands where they can read it, change
//! it, and send it themselves through the ordinary submission path — the one
//! path that signs, records and is audited. So this route is not a second send
//! path, and it is not the campaign send either: that needs the second egress
//! IP ADR 0044 §1 requires, which is a purchase.
//!
//! Three decisions worth stating, because each of them is a thing somebody
//! would otherwise reasonably add:
//!
//! - **The test copy goes to the caller and nowhere else.** There is no `to` on
//!   the route. A field naming the recipient of a rendered campaign is the
//!   first half of a sending API, and it would arrive without any of the
//!   things a campaign send owes a recipient — consent checked at send time, a
//!   `List-Unsubscribe` header, a suppression pass. "Within the tenant" at its
//!   strictest is *to yourself*, and the draft is editable by the person who
//!   asked, in their own client, if they want it elsewhere.
//! - **The subject is not marked as a test.** A `[TEST]` prefix would be the
//!   one thing on the screen that is not what a recipient gets, and the subject
//!   line is the most consequential string in the letter — it is what a filter
//!   scores and what an inbox truncates. A test whose subject differs from the
//!   real one does not test the subject.
//! - **`as=` names a person, `as=fallbacks` names nobody.** An address always
//!   contains an `@`, so the literal cannot collide with one. Absent means *the
//!   first person you may actually mail*, and when there is none the answer
//!   says `nobody_to_mail_yet` rather than quietly showing the fallback copy as
//!   though it were somebody's.
//!
//! **`GET /campaigns/merge-fields` returns names and nothing else** — no
//! labels, no descriptions, no example text. Those are user-facing strings and
//! belong in `web/src/i18n`, in three languages, not in a Rust literal that
//! would arrive in English whatever the reader set. What the server owns is the
//! vocabulary itself (`alo_store::CampaignMergeField::ALL`), which is the part
//! a client cannot know and must not hard-code: a field added here has to
//! appear in the composer without a web release.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::{
    CampaignId, CampaignMergeField, CampaignPreview, PreviewAgainst, PreviewAs, ResolvedMergeField,
};

use crate::billing::map_store_err;
use crate::campaigns::{address_of, stated};
use crate::error::Problem;
use crate::mime::{Addr, Outgoing};
use crate::state::{AppState, authenticate};
use crate::{api, drafts};

/// The literal that asks for the copy every recipient with nothing recorded
/// receives. Cannot be an address — an address contains an `@`.
const FALLBACKS: &str = "fallbacks";

/// `?as=` — whose copy of the letter to render.
#[derive(Deserialize)]
pub struct PreviewQuery {
    #[serde(default, rename = "as")]
    against: Option<String>,
}

impl PreviewQuery {
    /// What the caller asked for, or the `422` naming the parameter.
    fn against(&self) -> Result<PreviewAs, Problem> {
        match stated(self.against.as_deref()) {
            None => Ok(PreviewAs::AnyRecipient),
            Some(FALLBACKS) => Ok(PreviewAs::Fallbacks),
            Some(address) => Ok(PreviewAs::Recipient(address_of(address)?)),
        }
    }
}

/// One merge field as this copy printed it.
fn field_json(used: &ResolvedMergeField) -> Value {
    json!({
        "field": used.field.as_str(),
        "value": used.value,
        // The whole reason the report exists: "Hi there," and "Hi Jean," read
        // the same on a screen and only one of them is personalisation.
        "fellBack": used.fell_back,
    })
}

/// Whose values a preview used — a tagged object rather than a nullable
/// address, because *nobody* is an answer with a reason attached and a `null`
/// is not.
fn against_json(against: &PreviewAgainst) -> Value {
    match against {
        PreviewAgainst::Recipient {
            address,
            name,
            country,
        } => json!({
            "kind": "recipient",
            "address": address,
            "name": name,
            "country": country,
        }),
        PreviewAgainst::Fallbacks(reason) => json!({
            "kind": "fallbacks",
            "reason": reason.as_str(),
        }),
    }
}

/// A whole preview as JSON.
fn preview_json(preview: &CampaignPreview) -> Value {
    json!({
        "subject": preview.subject,
        "preheader": preview.preheader,
        "html": preview.html,
        "text": preview.text,
        "fields": preview.fields.iter().map(field_json).collect::<Vec<_>>(),
        "against": against_json(&preview.against),
    })
}

/// `GET /campaigns/merge-fields` → `{"fields":["first_name", …]}`.
///
/// The vocabulary a campaign can personalise with, in the order a composer
/// offers them. Authenticated like the rest of the surface: it is not a secret,
/// but an unauthenticated route on this prefix is one more thing to reason
/// about for no gain.
pub async fn list_merge_fields(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    authenticate(&state, &headers).await?;
    Ok(Json(json!({
        "fields": CampaignMergeField::ALL
            .iter()
            .map(|field| field.as_str())
            .collect::<Vec<_>>(),
    })))
}

/// `GET /campaigns/campaigns/{id}/preview[?as=<address>|fallbacks]` →
/// `{"preview":{…}}`.
///
/// A `404` covers an absent campaign, another tenant's, and an `as=` naming
/// somebody this tenant may not mail — the last of those deliberately, because
/// a preview that distinguished "never heard of them" from "they unsubscribed"
/// would answer a question about a person rather than about a URL.
pub async fn preview_campaign(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<PreviewQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let preview = account
        .acc
        .preview_campaign(&CampaignId::new(id), &query.against()?)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "preview": preview_json(&preview) })))
}

/// `POST /campaigns/campaigns/{id}/test[?as=…]` →
/// `{"draft":{"id","to","subject"}}`.
///
/// Renders the campaign exactly as [`preview_campaign`] does and writes it into
/// the caller's own Drafts, both parts, so it can be read in a real mail client
/// rather than in ours. **Nothing is sent**, and nothing about the campaign is
/// modified: asking twice writes two drafts.
pub async fn test_campaign(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<PreviewQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let preview = account
        .acc
        .preview_campaign(&CampaignId::new(id), &query.against()?)
        .await
        .map_err(map_store_err)?;

    // Resolved before anything else is decided: an account with no send address
    // has nowhere to put a test copy and no author to write it, and that is a
    // `422` naming the fact rather than a draft nobody can use.
    let address = drafts::from_address(&account, &state).await?;
    let outgoing = Outgoing {
        from: Addr {
            name: None,
            email: address.clone(),
        },
        to: vec![Addr {
            name: None,
            email: address.clone(),
        }],
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: preview.subject.clone(),
        in_reply_to: Vec::new(),
        references: Vec::new(),
        body_text: preview.text.clone(),
        // Both parts, so the draft is the `multipart/alternative` a recipient
        // gets and not merely a screenshot of one of its halves.
        body_html: Some(preview.html.clone()),
        attachments: Vec::new(),
        message_id_domain: api::domain_of(&address),
        message_id_token: api::new_message_token(),
    };
    let saved = drafts::save(&account, &outgoing).await?;

    Ok(Json(json!({
        "draft": {
            "id": saved.as_str(),
            "to": address,
            "subject": preview.subject,
        },
        // Echoed back so the screen can say whose copy is now in Drafts — the
        // same sentence it says about the preview, about the same letter.
        "against": against_json(&preview.against),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    fn asked(value: Option<&str>) -> Result<PreviewAs, Problem> {
        PreviewQuery {
            against: value.map(str::to_owned),
        }
        .against()
    }

    #[test]
    fn an_absent_as_asks_for_a_real_record_and_the_literal_asks_for_nobody() {
        assert_eq!(asked(None).ok(), Some(PreviewAs::AnyRecipient));
        // A screen whose select is on "everybody" sends the key with nothing in
        // it; that is the same request as omitting it.
        assert_eq!(asked(Some("  ")).ok(), Some(PreviewAs::AnyRecipient));
        assert_eq!(asked(Some(FALLBACKS)).ok(), Some(PreviewAs::Fallbacks));
    }

    #[test]
    fn an_address_is_folded_on_the_way_in_and_a_non_address_is_the_callers_error() {
        assert_eq!(
            asked(Some(" Ann@Example.TEST ")).ok(),
            Some(PreviewAs::Recipient("ann@example.test".to_owned())),
            "a screen echoing back an address in any casing lands where it means"
        );
        // `fallbacks` is the one non-address this parameter accepts, and it can
        // never collide with a real one — an address contains an `@`.
        assert!(!FALLBACKS.contains('@'));
        let refused = asked(Some("everyone")).err().map(|problem| problem.status);
        assert_eq!(refused, Some(StatusCode::UNPROCESSABLE_ENTITY));
    }
}
