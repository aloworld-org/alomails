//! What the media engine is told, and what it must never be told.
//!
//! LiveKit is a third party running as a sealed container. These pin the shape
//! of the one thing that crosses that boundary: a signed token.
//!
//! Written without `unwrap`, which this crate denies — a helper returns `Null`
//! on a malformed token, and the assertions below fail on it just as loudly.
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::Value;

use alo_jmap::meet_token::{mint, mint_room_admin, mint_room_record};

fn token_for(room: &str, secret: &str, now: i64) -> String {
    mint("key", secret, room, "user-anna", "anna", now).unwrap_or_default()
}

/// The claims segment, decoded.
fn claims_of(token: &str) -> Value {
    let Some(part) = token.split('.').nth(1) else {
        return Value::Null;
    };
    let Ok(bytes) = URL_SAFE_NO_PAD.decode(part) else {
        return Value::Null;
    };
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

/// The last segment is the signature.
fn signature_of(token: &str) -> String {
    token.split('.').next_back().unwrap_or_default().to_owned()
}

#[test]
fn a_token_grants_one_room_and_expires_soon() {
    let c = claims_of(&token_for("m-abc", "secret", 1_000_000));
    assert_eq!(c["video"]["room"], "m-abc");
    assert_eq!(c["video"]["roomJoin"], true);
    // Minutes, not hours: a token that outlives the join is one somebody can
    // pass on.
    let ttl = c["exp"].as_i64().unwrap_or_default() - 1_000_000;
    assert!((60..=600).contains(&ttl), "ttl was {ttl}s");
    // A minute of tolerance for clock drift, and no more.
    assert_eq!(c["nbf"].as_i64().unwrap_or_default(), 1_000_000 - 60);
}

/// A participant list in a third party's logs must not be a list of who works
/// at a customer, and a room name must not say what a meeting is about.
#[test]
fn the_engine_is_told_nothing_about_the_tenant() {
    let c = claims_of(&token_for("m-abc", "secret", 1_000_000));
    let raw = c.to_string();
    assert_ne!(raw, "null", "the token did not parse");
    for leaked in ["@", "alomails", "tenant", "Acme"] {
        assert!(
            !raw.contains(leaked),
            "the token carries {leaked:?} to the media engine"
        );
    }
}

/// A token for one meeting must not open another.
#[test]
fn a_token_is_not_transferable_between_rooms() {
    let a = signature_of(&token_for("m-one", "secret", 1_000));
    let b = signature_of(&token_for("m-two", "secret", 1_000));
    assert!(!a.is_empty());
    assert_ne!(a, b);
}

/// The whole arrangement rests on alo and the engine sharing exactly one
/// secret.
#[test]
fn the_signature_depends_on_the_secret() {
    let a = signature_of(&token_for("m-abc", "secret-one", 1_000));
    let b = signature_of(&token_for("m-abc", "secret-two", 1_000));
    assert!(!a.is_empty());
    assert_ne!(a, b);
}

#[test]
fn moderation_token_is_room_scoped_and_cannot_join() {
    let token = mint_room_admin("key", "secret", "m-abc", 1_000).unwrap_or_default();
    let c = claims_of(&token);
    assert_eq!(c["video"]["room"], "m-abc");
    assert_eq!(c["video"]["roomAdmin"], true);
    assert_eq!(c["video"]["roomJoin"], false);
    assert_eq!(c["video"]["canPublish"], false);
}

#[test]
fn recording_token_can_record_but_cannot_enter_a_room() {
    let token = mint_room_record("key", "secret", 1_000).unwrap_or_default();
    let c = claims_of(&token);
    assert_eq!(c["video"]["roomRecord"], true);
    assert_eq!(c["video"]["roomJoin"], false);
    assert_eq!(c["video"]["canPublish"], false);
    assert!(c["video"]["roomAdmin"].is_null());
}
