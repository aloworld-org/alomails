//! `POST /sites/shop-config/propose` — the shop-setup proposal on the wire
//! (S3.05b2, ADR 0041).
//!
//! One business description in, one [`alo_ai::site_shop_config`] envelope
//! out, **returned for review and never applied here**: the route reads no
//! shop state and writes none, so approving a proposal is exclusively the
//! approval screen's act, through the already-owned Billing product, ticket
//! and shop-settings routes.
//!
//! Who may knock: workspace members only. A restricted site-editor
//! collaborator (S2.03a) must never see Billing-side facts, and this proposal
//! names prices and VAT — the static path is deliberately outside the
//! site-editor allowlist in [`crate::scoped_roles`], so the one middleware
//! that scopes that role refuses it at both mounts before this handler runs.
//!
//! The envelope's honesty is the parser's, not this route's: prices are only
//! ever the description's own figures or flagged blanks, VAT is structurally
//! a guess, shipping follows the goods. Nothing here re-checks that — a
//! second validator would drift from the one that counts.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_ai::InferenceError;
use alo_ai::site_shop_config::{ShopConfigError, propose_shop_config};

use crate::ai::tenant_ai_config;
use crate::error::Problem;
use crate::sites::MAX_SITE_DESCRIPTION_CHARS;
use crate::state::{AppState, authenticate};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposeBody {
    description: String,
}

/// The typed branch the UI keeps the manual setup path beside (the S1.28a
/// shape): 503 + `{"reason":"unconfigured"}` when no AI backend is wired.
fn unconfigured() -> Problem {
    Problem::with(
        StatusCode::SERVICE_UNAVAILABLE,
        "Shop setup proposals are not configured. You can set up the catalog by hand instead.",
    )
    .with_extra(json!({ "reason": "unconfigured" }))
}

fn proposal_problem(error: &ShopConfigError) -> Problem {
    match error {
        ShopConfigError::Inference(InferenceError::Disabled | InferenceError::NotConfigured) => {
            unconfigured()
        }
        ShopConfigError::Inference(
            InferenceError::Backend(_) | InferenceError::Transport | InferenceError::Empty,
        ) => Problem::with(
            StatusCode::BAD_GATEWAY,
            "Shop setup could not reach the configured AI service. Try again shortly.",
        )
        .with_extra(json!({ "reason": "unreachable" })),
        ShopConfigError::MissingObject
        | ShopConfigError::UnsupportedVersion(_)
        | ShopConfigError::Shape(_)
        | ShopConfigError::Invalid(_)
        | ShopConfigError::RepairFailed(_) => Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "AI could not draft a valid shop setup. Nothing was changed; refine the description and try again.",
        )
        .with_extra(json!({ "reason": "invalid_proposal" })),
    }
}

/// `POST /sites/shop-config/propose` `{description}` → `{"proposal": …}`.
///
/// The reply is the parser-enforced envelope verbatim: every price either
/// `{"state":"stated"}` with the description's own amount or a
/// `{"state":"needs_input"}` blank, every VAT treatment under the `vat_guess`
/// key with the basis sentence the owner's accountant judges. Applying an
/// approved proposal is the caller's job through the owned routes; this
/// handler persists nothing on any path.
pub async fn propose(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: ProposeBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let description = req.description.trim();
    if description.is_empty() {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "Describe the business whose shop should be proposed.",
        ));
    }
    if description.chars().count() > MAX_SITE_DESCRIPTION_CHARS {
        return Err(Problem::with(
            StatusCode::PAYLOAD_TOO_LARGE,
            "The business description is too long. Shorten it and try again.",
        ));
    }

    let config = tenant_ai_config(&account).await.map_err(|problem| {
        if problem.status == StatusCode::SERVICE_UNAVAILABLE {
            unconfigured()
        } else {
            problem
        }
    })?;
    let proposal = propose_shop_config(&config, description)
        .await
        .map_err(|error| proposal_problem(&error))?;

    Ok(Json(json!({ "proposal": proposal })))
}
