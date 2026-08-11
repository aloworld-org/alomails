//! Workspace invitations: the token, what spending it installs, and what it
//! refuses (migration 0209; `platform/alo-store/src/user_invites.rs`).
//!
//! An invitation is a credential-shaped thing sitting in somebody's mailbox, so
//! the properties that matter are the ones about *refusing*:
//!
//! - spending it installs a credential **and** a recovery address, together or
//!   not at all — an account that can be signed into but never recovered is the
//!   state this feature exists to end;
//! - a spent token, an expired one and one that never existed are all the same
//!   answer, so the acceptance page cannot be used to learn which tokens exist;
//! - the token itself is never stored, only its hash;
//! - two people opening the same link resolve to one acceptance, not to a
//!   unique-index error reported as a server fault.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{Store, TenantId, UserId};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use common::{database_url, test_store};

/// A raw pool beside the store, for reading rows the store has no public read
/// for — the credential and the recovery address this feature installs. The
/// same arrangement `billing_bills.rs` uses, so `Store::pool` stays private.
async fn raw_pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url())
        .await
        .unwrap()
}

/// A token hash unique to this run. The literals this replaces made the
/// second `cargo test` fail on the primary key, because the table outlives the
/// run while the constants did not change.
fn token(tenant: &TenantId, name: &str) -> String {
    format!("{}-{name}", tenant.as_str())
}

/// A tenant, a user, and the address that user was created under.
///
/// The address carries the tenant id because `credentials.username` is unique
/// across the whole deployment and this table outlives the run: a fixed
/// address made every acceptance after the first `cargo test` collide with the
/// credential the previous one installed.
async fn tenant_with_user(store: &Store, tag: &str) -> (TenantId, UserId, String) {
    let tenant = store.create_tenant(&format!("inv-{tag}")).await.unwrap();
    let email = format!("{tag}-{}@invites.test", tenant.as_str().to_lowercase());
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&email)
        .await
        .unwrap();
    (tenant, user, email)
}

#[tokio::test]
async fn accepting_installs_a_credential_and_a_recovery_address() {
    let store = test_store().await;
    let (tenant, user, email) = tenant_with_user(&store, "accept").await;
    let ts = store.for_tenant(tenant.clone());
    let invites = store.invites();
    let pool = raw_pool().await;

    assert!(!ts.has_open_invite(&user).await.unwrap());
    ts.invite_user(&user, &email, &token(&tenant, "a"), &user)
        .await
        .unwrap();
    assert!(ts.has_open_invite(&user).await.unwrap());

    let target = invites.invite(&token(&tenant, "a")).await.unwrap().unwrap();
    assert_eq!(target.email, email);
    assert_eq!(target.user, user);

    let accepted = invites
        .accept(
            &token(&tenant, "a"),
            "argon2-of-their-choice",
            "ben@elsewhere.test",
        )
        .await
        .unwrap();
    assert!(accepted.is_some());

    // The credential exists under the invited address, and the recovery
    // address is theirs rather than the mailbox they would be locked out of.
    let cred: Option<String> = sqlx::query_scalar(
        "SELECT password_hash FROM credentials WHERE user_id = $1 AND username = $2",
    )
    .bind(user.as_str())
    .bind(&email)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(cred.as_deref(), Some("argon2-of-their-choice"));

    let recovery: Option<String> =
        sqlx::query_scalar("SELECT recovery_email FROM account_recovery WHERE address = $1")
            .bind(&email)
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert_eq!(
        recovery.as_deref(),
        Some("ben@elsewhere.test"),
        "an account that can be signed into and not recovered is the bug this fixes"
    );

    // And the invitation is spent.
    assert!(
        invites
            .invite(&token(&tenant, "a"))
            .await
            .unwrap()
            .is_none()
    );
    assert!(!ts.has_open_invite(&user).await.unwrap());
}

#[tokio::test]
async fn a_spent_token_is_refused_exactly_like_one_that_never_existed() {
    let store = test_store().await;
    let (tenant, user, email) = tenant_with_user(&store, "spent").await;
    let ts = store.for_tenant(tenant.clone());
    let invites = store.invites();
    let pool = raw_pool().await;

    ts.invite_user(&user, &email, &token(&tenant, "b"), &user)
        .await
        .unwrap();
    assert!(
        invites
            .accept(&token(&tenant, "b"), "first", "r@elsewhere.test")
            .await
            .unwrap()
            .is_some()
    );

    // Spending it twice must not install a second credential, and must answer
    // the same as a token nobody ever issued — no oracle for "this existed".
    assert!(
        invites
            .accept(&token(&tenant, "b"), "second", "other@elsewhere.test")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        invites
            .accept("never-issued", "x", "y@elsewhere.test")
            .await
            .unwrap()
            .is_none()
    );
    assert!(invites.invite("never-issued").await.unwrap().is_none());

    let creds: i64 = sqlx::query_scalar("SELECT count(*) FROM credentials WHERE user_id = $1")
        .bind(user.as_str())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(creds, 1, "the second acceptance installed nothing");
}

#[tokio::test]
async fn an_expired_invitation_is_refused() {
    let store = test_store().await;
    let (tenant, user, email) = tenant_with_user(&store, "expired").await;
    let ts = store.for_tenant(tenant.clone());
    let invites = store.invites();
    let pool = raw_pool().await;

    ts.invite_user(&user, &email, &token(&tenant, "c"), &user)
        .await
        .unwrap();
    // Age it past its window rather than waiting seven days for it.
    sqlx::query(
        "UPDATE user_invites SET expires_at = now() - interval '1 hour' WHERE token_hash = $1",
    )
    .bind(token(&tenant, "c"))
    .execute(&pool)
    .await
    .unwrap();

    assert!(
        invites
            .invite(&token(&tenant, "c"))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        invites
            .accept(&token(&tenant, "c"), "x", "y@elsewhere.test")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        !ts.has_open_invite(&user).await.unwrap(),
        "an expired invitation is not outstanding"
    );
}

#[tokio::test]
async fn re_inviting_leaves_the_earlier_link_usable_until_one_is_spent() {
    let store = test_store().await;
    let (tenant, user, email) = tenant_with_user(&store, "resend").await;
    let ts = store.for_tenant(tenant.clone());
    let invites = store.invites();
    let pool = raw_pool().await;

    // An admin who resends because the first mail was missed must not break
    // the first link — the person may open whichever they happen to find.
    ts.invite_user(&user, &email, &token(&tenant, "d1"), &user)
        .await
        .unwrap();
    ts.invite_user(&user, &email, &token(&tenant, "d2"), &user)
        .await
        .unwrap();
    assert!(
        invites
            .invite(&token(&tenant, "d1"))
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        invites
            .invite(&token(&tenant, "d2"))
            .await
            .unwrap()
            .is_some()
    );

    assert!(
        invites
            .accept(&token(&tenant, "d1"), "chosen", "r@elsewhere.test")
            .await
            .unwrap()
            .is_some()
    );
    // Spending one link spends them all: the invitation is to the account, and
    // the account can only be claimed once. Before this was fixed the second
    // link reached the credential insert and failed on the unique index — a
    // server error where the honest answer is "that link has been used".
    assert!(
        invites
            .accept(&token(&tenant, "d2"), "later", "r@elsewhere.test")
            .await
            .unwrap()
            .is_none()
    );
    let creds: i64 = sqlx::query_scalar("SELECT count(*) FROM credentials WHERE user_id = $1")
        .bind(user.as_str())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(creds, 1, "the second link installed nothing");
}
