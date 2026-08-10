//! What the media engine is told, and what it must never be told.
//!
//! LiveKit is a third party running as a sealed container. These assert the
//! shape of the one thing that crosses that boundary: a signed token.
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::Value;

use alo_jmap::meet_token::mint;

/// The claims segment, decoded.
fn claims_of(token: &str) -> Value {
    let part = token.split('.').nth(1).unwrap();
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(part).unwrap()).unwrap()
}

#[test]
fn a_token_grants_one_room_and_expires_soon() {
    let token = mint("key", "secret", "m-abc", "user-anna", "anna", 1_000_000).unwrap();
    let c = claims_of(&token);
    assert_eq!(c["video"]["room"], "m-abc");
    assert_eq!(c["video"]["roomJoin"], true);
    // Minutes, not hours: a token that outlives the join is one somebody can
    // pass on.
    let ttl = c["exp"].as_i64().unwrap() - 1_000_000;
    assert!((60..=600).contains(&ttl), "ttl was {ttl}s");
    // A minute of tolerance for clock drift, and no more.
    assert_eq!(c["nbf"].as_i64().unwrap(), 1_000_000 - 60);
}

/// A participant list in a third party's logs must not be a list of who works
/// at a customer, and a room name must not say what a meeting is about.
#[test]
fn the_engine_is_told_nothing_about_the_tenant() {
    let token = mint("key", "secret", "m-abc", "user-anna", "anna", 1_000_000).unwrap();
    let raw = serde_json::to_string(&claims_of(&token)).unwrap();
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
    let a = mint("key", "secret", "m-one", "user-anna", "anna", 1_000).unwrap();
    let b = mint("key", "secret", "m-two", "user-anna", "anna", 1_000).unwrap();
    assert_ne!(a.split('.').next_back(), b.split('.').next_back());
}

/// The whole arrangement rests on alo and the engine sharing exactly one
/// secret.
#[test]
fn the_signature_depends_on_the_secret() {
    let a = mint("key", "secret-one", "m-abc", "user-anna", "anna", 1_000).unwrap();
    let b = mint("key", "secret-two", "m-abc", "user-anna", "anna", 1_000).unwrap();
    assert_ne!(a.split('.').next_back(), b.split('.').next_back());
}
