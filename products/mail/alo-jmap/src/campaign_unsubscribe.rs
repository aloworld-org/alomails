//! The page at the end of the link (alo Campaigns, ADR 0044 §3, wave C2s.2) —
//! **public**: no account, no login, and no way to get one.
//!
//! Two endpoints, and the difference between them is the point:
//!
//! - `GET  /jmap/campaign-unsubscribe/{token}` — what the page needs to draw
//!   itself. **No side effect whatsoever.**
//! - `POST /jmap/campaign-unsubscribe/{token}` — the act.
//!
//! ## Why the GET does nothing
//!
//! RFC 8058 exists because link-prefetching scanners fetch every URL in a
//! message before a human sees it. A `GET` that unsubscribed would unsubscribe
//! everybody whose mail passed a corporate scanner, an antivirus proxy or a
//! preview pane — and there is no way to undo one (ADR 0044 §2 is absolute by
//! design), so the mistake would be permanent and would look like the feature
//! working. The resolve in
//! [`alo_store::campaign_unsubscribe`](alo_store::Store::resolve_campaign_unsubscribe_token)
//! is side-effect-free for the same reason.
//!
//! ## Fewer, rather than only none
//!
//! The queue item: *a recipient offered only all-or-nothing presses the spam
//! button instead, and that is the signal that ends a sending reputation.* So
//! the `POST` takes a [`Scope`]: this kind of mail, or all of it. One press
//! either way, and no second screen asking whether they are sure — a
//! confirmation maze on an unsubscribe is how a person ends up pressing "spam"
//! instead, which costs the sender far more than the subscription did.
//!
//! When the send did not name a kind of mail, the narrower choice is not
//! offered at all rather than offered and quietly ignored: `topic` is `null` on
//! the `GET`, and `scope=topic` on the `POST` is a `422` that says so.
//!
//! ## RFC 8058 one-click, and why it means *all of it*
//!
//! A mail client's own Unsubscribe button posts
//! `List-Unsubscribe=One-Click` as a form body, with no page and no chance for
//! the recipient to choose. This route accepts that exactly as the RFC spells
//! it, and treats it as the **wider** choice. The recipient made one
//! unconditional gesture; reading it as "only this kind" would leave them
//! receiving mail they believed they had stopped, and the next press is the
//! spam button. Somebody who wanted less rather than none still has the link
//! and the page.
//!
//! Anything else is a `422` naming the choice. Being permissive here — reading
//! any stray `POST` as "stop everything" — would hand a scanner that posts the
//! power to end a customer relationship that cannot be restored.
//!
//! ## What this route never reveals
//!
//! - **The address.** It is not in either answer. A link is forwarded, quoted
//!   in replies and read by scanners, and a page that echoed the recipient back
//!   would turn a forwarded mail into a disclosure. The topic *is* returned: it
//!   describes the mail rather than the person, so it tells the holder only
//!   what they have already read.
//! - **Whether a token was ever minted.** An unknown token, a malformed one and
//!   an empty one are the same `404`, so a spammer posting a million guesses
//!   learns which of their guesses were right — nothing — rather than which
//!   addresses this deployment holds.
//! - **Which workspace it is.** The tenant is what the token resolves to; it is
//!   never in an answer.

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, http::HeaderValue};
use serde_json::{Value, json};

use alo_store::{
    NewSuppression, NewTopicOptOut, SuppressionReason, UnsubscribeTokenTarget, normalise_topic,
};

use crate::error::Problem;
use crate::state::AppState;

/// The body RFC 8058 §3.1 requires of a one-click unsubscribe, verbatim.
const ONE_CLICK: &str = "List-Unsubscribe=One-Click";

/// The one answer an unknown, spent, malformed or empty token gets.
///
/// One sentence for every miss, and deliberately not four: telling them apart
/// is what turns this endpoint into an oracle for which addresses this
/// deployment holds.
fn unknown_link() -> Problem {
    Problem::with(
        StatusCode::NOT_FOUND,
        "This unsubscribe link is not one we recognise.",
    )
}

/// How much of this tenant's mail the recipient wants to stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// This kind of mail only — the *fewer* half. Requires the send to have
    /// named a kind.
    Topic,
    /// All of it: an absolute, tenant-wide suppression (ADR 0044 §2), which
    /// nothing can lift.
    All,
}

impl Scope {
    /// The stored token, as it arrives on the wire and goes back in the answer.
    fn as_str(self) -> &'static str {
        match self {
            Self::Topic => "topic",
            Self::All => "all",
        }
    }
}

/// The `422` a body earns, naming what the caller may say instead.
///
/// Verbatim and specific, per `docs/design/ux-principles.md` — though on this
/// surface the reader is a program: the page sends one of two words, and a mail
/// client sends the RFC's sentence.
fn unprocessable(detail: impl Into<String>) -> Problem {
    Problem::with(StatusCode::UNPROCESSABLE_ENTITY, detail)
}

/// Reads the scope out of a request body, in the two shapes this route accepts.
///
/// A free function so the rule is testable without a server, and so there is
/// exactly one of it: two places deciding what "stop everything" looks like is
/// one place too many for a decision that cannot be undone.
///
/// The form shape is checked before the JSON one because a mail client sends it
/// with `Content-Type: application/x-www-form-urlencoded` and no JSON at all,
/// and because a body that is literally RFC 8058's sentence has exactly one
/// meaning whatever a header claims.
pub(crate) fn scope_of(content_type: Option<&str>, body: &[u8]) -> Result<Scope, Problem> {
    let text = std::str::from_utf8(body).unwrap_or_default().trim();
    if text.eq_ignore_ascii_case(ONE_CLICK) {
        return Ok(Scope::All);
    }
    let form = content_type.is_some_and(|value| {
        value
            .trim_start()
            .starts_with("application/x-www-form-urlencoded")
    });
    if form {
        // A form body that is not the RFC's sentence is not a choice we can
        // guess at. Reading it as "stop everything" would hand whatever posted
        // it the power to end a relationship nothing can restore.
        return Err(unprocessable(
            "a one-click unsubscribe posts List-Unsubscribe=One-Click, exactly as RFC 8058 \
             spells it",
        ));
    }
    let parsed: Value = serde_json::from_slice(body).map_err(|_| Problem::not_json())?;
    match parsed.get("scope").and_then(Value::as_str) {
        Some("topic") => Ok(Scope::Topic),
        Some("all") => Ok(Scope::All),
        _ => Err(unprocessable(
            "scope says how much to stop: topic for this kind of mail, all for everything",
        )),
    }
}

/// Whose link this is, or the one `404` every miss shares.
async fn target(state: &AppState, token: &str) -> Result<UnsubscribeTokenTarget, Problem> {
    state
        .store
        .resolve_campaign_unsubscribe_token(token)
        .await
        .map_err(|_| Problem::server_error())?
        .ok_or_else(unknown_link)
}

/// What the page shows, for a target that has already been resolved.
///
/// `stopped` and `topicDeclined` are about *this* link's recipient in *this*
/// workspace, which is precisely what holding the link entitles somebody to
/// know. They exist so the page can say "you have already stopped this" rather
/// than offering a button that does nothing — the ambiguity that makes a person
/// press it twice and then press "spam".
async fn state_of(state: &AppState, target: &UnsubscribeTokenTarget) -> Result<Value, Problem> {
    let ts = state.store.for_tenant(target.tenant.clone());
    let stopped = ts
        .campaign_suppression_for(&target.address)
        .await
        .map_err(|_| Problem::server_error())?
        .is_some();
    let topic_declined = match target.topic.as_deref().and_then(normalise_topic) {
        None => false,
        Some(folded) => ts
            .campaign_topics_declined_by(&target.address)
            .await
            .map_err(|_| Problem::server_error())?
            .iter()
            .any(|declined| declined.topic == folded),
    };
    Ok(json!({
        // As the sender wrote it — a human reads it. `null` when the send did
        // not name a kind, and then the page offers one button rather than two.
        "topic": target.topic,
        "stopped": stopped,
        "topicDeclined": topic_declined,
    }))
}

/// `GET /jmap/campaign-unsubscribe/{token}` — what the landing page needs to
/// draw itself, and **nothing is written**.
///
/// Public: the recipient has no account, and the token is the whole credential.
/// See the module docs for why this must stay side-effect-free — every scanner
/// between us and the recipient fetches it.
///
/// # Errors
/// `404` when the token is not one we hold, in the same sentence a malformed or
/// empty one gets.
pub async fn show(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<Value>, Problem> {
    let target = target(&state, &token).await?;
    Ok(Json(state_of(&state, &target).await?))
}

/// `POST /jmap/campaign-unsubscribe/{token}` — the act.
///
/// Body is either RFC 8058's `List-Unsubscribe=One-Click` (a mail client's own
/// button — the wider choice, see the module docs) or `{"scope":"topic"|"all"}`
/// from our page.
///
/// Idempotent in both scopes: pressing the link twice, which everybody who is
/// not sure it worked does, answers the same thing and does not restamp when
/// they decided.
///
/// # Errors
/// `404` when the token is not one we hold; `422` when the body says neither of
/// the two things this route accepts, or asks to stop a kind of mail the send
/// never named.
pub async fn act(
    State(state): State<AppState>,
    Path(token): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let scope = scope_of(
        headers
            .get(CONTENT_TYPE)
            .and_then(|v: &HeaderValue| v.to_str().ok()),
        &body,
    )?;
    let target = target(&state, &token).await?;
    let ts = state.store.for_tenant(target.tenant.clone());

    match scope {
        Scope::All => {
            // Absolute and tenant-wide (ADR 0044 §2). `source_ref` is the
            // token's RECORD id, never the token: which send they left over
            // stays answerable without the working credential being copied into
            // a second table.
            ts.suppress_campaign_address(&NewSuppression {
                address: &target.address,
                reason: SuppressionReason::Unsubscribe,
                source_ref: Some(target.record.as_str()),
                occurred_at: None,
            })
            .await
            .map_err(|_| Problem::server_error())?;
        }
        Scope::Topic => {
            let topic = target.topic.as_deref().ok_or_else(|| {
                unprocessable(
                    "this message did not say what kind of mail it is, so the only choice is \
                     to stop all of it",
                )
            })?;
            ts.decline_campaign_topic(&NewTopicOptOut {
                address: &target.address,
                topic,
                source_ref: Some(target.record.as_str()),
                occurred_at: None,
            })
            .await
            .map_err(|_| Problem::server_error())?;
        }
    }

    let mut answer = state_of(&state, &target).await?;
    if let Some(object) = answer.as_object_mut() {
        object.insert("scope".to_owned(), json!(scope.as_str()));
    }
    Ok(Json(answer))
}

#[cfg(test)]
mod tests {
    use super::{ONE_CLICK, Scope, scope_of};

    fn scope(content_type: Option<&str>, body: &str) -> Option<Scope> {
        scope_of(content_type, body.as_bytes()).ok()
    }

    #[test]
    fn a_mail_clients_own_button_stops_everything() {
        // RFC 8058 §3.1, verbatim, with the Content-Type a client actually
        // sends. One unconditional gesture, read as the wider choice: see the
        // module docs for why the narrower reading earns a spam press.
        assert_eq!(
            scope(Some("application/x-www-form-urlencoded"), ONE_CLICK),
            Some(Scope::All)
        );
        // Some clients add a charset parameter, and some send it with no
        // Content-Type at all. The body is unambiguous either way.
        assert_eq!(
            scope(
                Some("application/x-www-form-urlencoded; charset=UTF-8"),
                ONE_CLICK
            ),
            Some(Scope::All)
        );
        assert_eq!(scope(None, ONE_CLICK), Some(Scope::All));
        assert_eq!(
            scope(None, "  list-unsubscribe=one-click  "),
            Some(Scope::All)
        );
    }

    #[test]
    fn our_own_page_says_which_of_the_two_buttons_was_pressed() {
        assert_eq!(
            scope(Some("application/json"), r#"{"scope":"topic"}"#),
            Some(Scope::Topic)
        );
        assert_eq!(
            scope(Some("application/json"), r#"{"scope":"all"}"#),
            Some(Scope::All)
        );
    }

    #[test]
    fn a_body_that_is_not_a_choice_is_refused_rather_than_guessed_at() {
        // The important direction. Reading a stray post as "stop everything"
        // would let whatever sent it end a relationship that nothing can
        // restore — there is no way to lift a suppression, by design.
        for body in [
            "",
            "   ",
            "{}",
            r#"{"scope":""}"#,
            r#"{"scope":"everything"}"#,
            r#"{"scope":true}"#,
            "List-Unsubscribe=Two-Click",
            "unsubscribe=yes",
        ] {
            assert_eq!(
                scope(Some("application/json"), body),
                None,
                "accepted {body:?}"
            );
        }
        // And a form body that is not the RFC's sentence is refused as a form
        // body, rather than falling through to a JSON parse error that would
        // read as our page being broken.
        assert_eq!(
            scope(Some("application/x-www-form-urlencoded"), "scope=all"),
            None
        );
    }
}
