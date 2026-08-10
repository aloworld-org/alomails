//! Access tokens for the media engine.
//!
//! LiveKit authenticates with a JWT signed by a shared secret (its documented
//! public contract — `docs/alo-product-description.md`: engines are contacted
//! exclusively through their public API, and for LiveKit that is SDK, JWT and
//! webhooks). Minting one is the whole seam between alo's identity and the
//! engine's, and everything that makes the arrangement safe is decided here:
//!
//! - **The engine is told an opaque room and nothing else.** No tenant, no
//!   title, no email. Its `identity` is the workspace user id, which is
//!   meaningless outside alo, and its display name is the local part of an
//!   address rather than the address — a participant list in a third party's
//!   logs should not be a list of who works here.
//! - **A token is short-lived and single-room.** It grants join on exactly one
//!   room, expires in minutes, and is minted per join. Nothing about it is
//!   reusable for a room the person was not admitted to, because the admission
//!   check happens in the store before this is ever called.
//! - **Nothing is signed for a meeting the caller could not open.** This module
//!   does not check anything; it is called only after `join_meeting` has
//!   already answered, and that ordering is the security property.
//!
//! The secret never leaves the server. The browser receives a token, never a
//! key, and cannot mint another.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::Sha256;

/// How long a join token is good for. Minutes, not hours: it is exchanged for
/// a media session immediately, and a token that outlives the join is a token
/// somebody can pass on.
const TOKEN_TTL_SECONDS: i64 = 300;

#[derive(Serialize)]
struct Header {
    alg: &'static str,
    typ: &'static str,
}

/// LiveKit's room-grant claim.
#[derive(Serialize)]
struct VideoGrant<'a> {
    #[serde(rename = "roomJoin")]
    room_join: bool,
    room: &'a str,
    #[serde(rename = "canPublish")]
    can_publish: bool,
    #[serde(rename = "canSubscribe")]
    can_subscribe: bool,
    #[serde(rename = "canPublishData")]
    can_publish_data: bool,
}

#[derive(Serialize)]
struct Claims<'a> {
    /// The API key identifies which shared secret signed this.
    iss: &'a str,
    /// Who the engine will call this participant. A workspace user id, which
    /// means nothing outside alo.
    sub: &'a str,
    /// LiveKit reads the participant identity from here.
    identity: &'a str,
    /// What other participants see. A local part, never a full address.
    name: &'a str,
    nbf: i64,
    exp: i64,
    video: VideoGrant<'a>,
}

/// Mint a join token for one room.
///
/// `identity` is the workspace user id and `display` the name others will see
/// — pass the local part of an address, not the address.
///
/// # Errors
/// Returns `None` when the secret is unusable or the claims cannot be encoded,
/// which the caller should report as "meetings are not configured" rather than
/// as a failure of this request.
#[must_use]
pub fn mint(
    api_key: &str,
    api_secret: &str,
    room: &str,
    identity: &str,
    display: &str,
    now: i64,
) -> Option<String> {
    let header = Header {
        alg: "HS256",
        typ: "JWT",
    };
    let claims = Claims {
        iss: api_key,
        sub: identity,
        identity,
        name: display,
        // A minute of tolerance for clock drift between us and the engine;
        // without it a correctly-minted token is refused on a healthy system.
        nbf: now - 60,
        exp: now + TOKEN_TTL_SECONDS,
        video: VideoGrant {
            room_join: true,
            room,
            can_publish: true,
            can_subscribe: true,
            // Data messages carry things like raised hands and reactions,
            // which belong to the meeting rather than to us.
            can_publish_data: true,
        },
    };
    let header_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).ok()?);
    let claims_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).ok()?);
    let signing_input = format!("{header_b64}.{claims_b64}");
    let mut mac = Hmac::<Sha256>::new_from_slice(api_secret.as_bytes()).ok()?;
    mac.update(signing_input.as_bytes());
    let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    Some(format!("{signing_input}.{signature}"))
}
