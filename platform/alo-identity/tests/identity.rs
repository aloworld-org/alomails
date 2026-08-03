//! Identity integration + isolation tests over live Postgres: password
//! auth (incl. the anti-enumeration result), token issue/resolve/revoke,
//! TOTP enroll/confirm/verify with drift, single-use recovery codes,
//! alias-aware resolution, groups, and — the load-bearing property — that
//! no identity operation crosses a tenant or account boundary.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_identity::totp::{TotpOutcome, current_code};
use common::{make_user, setup};

#[tokio::test]
async fn password_auth_roundtrip_and_anti_enumeration() {
    let (store, id) = setup().await;
    let u = make_user(&store, &id, "pw").await;

    // Correct password → the principal.
    let p = id
        .authenticate_password(&u.email, &u.password)
        .await
        .unwrap()
        .expect("should authenticate");
    assert_eq!(p.user, u.user);
    assert_eq!(p.tenant, u.tenant);

    // Wrong password and an unknown user are indistinguishable in result.
    assert!(
        id.authenticate_password(&u.email, "wrong-password")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        id.authenticate_password("nobody-here@ex.test", "whatever")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn access_token_issue_resolve_and_revoke() {
    let (store, id) = setup().await;
    let u = make_user(&store, &id, "tok").await;

    let token = id
        .issue_access_token(&u.tenant, &u.user, None, "openid email")
        .await
        .unwrap();
    let p = id
        .resolve_access_token(token.reveal())
        .await
        .unwrap()
        .expect("token resolves");
    assert_eq!(p.user, u.user);
    assert_eq!(p.scope, "openid email");

    // Revocation is real: the next resolve fails.
    id.revoke_access_token(token.reveal()).await.unwrap();
    assert!(
        id.resolve_access_token(token.reveal())
            .await
            .unwrap()
            .is_none()
    );

    // A garbage token never resolves (no panic, no leak).
    assert!(
        id.resolve_access_token("not-a-real-token")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn tokens_do_not_cross_tenant_or_account() {
    let (store, id) = setup().await;
    let a = make_user(&store, &id, "iso-a").await; // tenant A
    let b = make_user(&store, &id, "iso-b").await; // tenant B

    let ta = id
        .issue_access_token(&a.tenant, &a.user, None, "openid")
        .await
        .unwrap();

    // A's token resolves only to A — never to B's tenant/user.
    let p = id.resolve_access_token(ta.reveal()).await.unwrap().unwrap();
    assert_eq!(p.tenant, a.tenant);
    assert_ne!(p.tenant, b.tenant);
    assert_ne!(p.user, b.user);

    // B cannot authenticate as A with A's own password (username is A's).
    assert!(
        id.authenticate_password(&b.email, &a.password)
            .await
            .unwrap()
            .is_none()
    );

    // account_by_email resolves each address to its own account only.
    assert_eq!(
        store.account_by_email(&a.email).await.unwrap(),
        Some((a.tenant.clone(), a.user.clone()))
    );
    assert_eq!(
        store.account_by_email(&b.email).await.unwrap(),
        Some((b.tenant.clone(), b.user.clone()))
    );
}

#[tokio::test]
async fn alias_resolves_to_its_owner_only() {
    let (store, id) = setup().await;
    let u = make_user(&store, &id, "alias").await;
    let alias = format!("sales-{}@ex.test", u.tenant);
    store
        .for_tenant(u.tenant.clone())
        .add_alias(&u.user, &alias)
        .await
        .unwrap();

    // The alias routes to the owner; case-insensitively.
    assert_eq!(
        store.account_by_email(&alias).await.unwrap(),
        Some((u.tenant.clone(), u.user.clone()))
    );
    assert_eq!(
        store.account_by_email(&alias.to_uppercase()).await.unwrap(),
        Some((u.tenant.clone(), u.user.clone()))
    );
    // An unknown alias resolves to nothing.
    assert!(
        store
            .account_by_email("ghost@ex.test")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn totp_enroll_confirm_and_verify_with_drift() {
    let (store, id) = setup().await;
    let u = make_user(&store, &id, "totp").await;

    // Before enrollment, 2FA is not required.
    assert_eq!(
        id.check_second_factor(&u.tenant, &u.user, None)
            .await
            .unwrap(),
        TotpOutcome::NotEnrolled
    );

    let enrollment = id.enroll_totp(&u.tenant, &u.user, &u.email).await.unwrap();
    assert!(enrollment.provisioning_uri.starts_with("otpauth://totp/"));

    // An unconfirmed secret must NOT gate login yet.
    assert_eq!(
        id.check_second_factor(&u.tenant, &u.user, None)
            .await
            .unwrap(),
        TotpOutcome::NotEnrolled
    );

    // Confirm with the code an authenticator app would show now.
    let code = current_code(&enrollment.secret_base32).unwrap();
    let recovery = id
        .confirm_totp(&u.tenant, &u.user, &code)
        .await
        .unwrap()
        .expect("confirmation succeeds");
    assert_eq!(recovery.len(), 10);

    // Now 2FA is required: no code → Failed, right code → Verified.
    assert_eq!(
        id.check_second_factor(&u.tenant, &u.user, None)
            .await
            .unwrap(),
        TotpOutcome::Failed
    );
    let code = current_code(&enrollment.secret_base32).unwrap();
    assert_eq!(
        id.check_second_factor(&u.tenant, &u.user, Some(&code))
            .await
            .unwrap(),
        TotpOutcome::Verified
    );
    // A wrong code fails.
    assert_eq!(
        id.check_second_factor(&u.tenant, &u.user, Some("000000"))
            .await
            .unwrap(),
        TotpOutcome::Failed
    );
}

#[tokio::test]
async fn recovery_codes_are_single_use_and_account_scoped() {
    let (store, id) = setup().await;
    let victim = make_user(&store, &id, "rec-v").await;
    let other = make_user(&store, &id, "rec-o").await;

    let enrollment = id
        .enroll_totp(&victim.tenant, &victim.user, &victim.email)
        .await
        .unwrap();
    let code = current_code(&enrollment.secret_base32).unwrap();
    let recovery = id
        .confirm_totp(&victim.tenant, &victim.user, &code)
        .await
        .unwrap()
        .unwrap();
    let one = recovery[0].reveal().to_owned();

    // Another account cannot use the victim's recovery code (scoped door).
    // `other` has no TOTP enrolled, so its second factor is NotEnrolled;
    // enroll it too so we truly test a wrong code, not an unenrolled user.
    let oe = id
        .enroll_totp(&other.tenant, &other.user, &other.email)
        .await
        .unwrap();
    let oc = current_code(&oe.secret_base32).unwrap();
    id.confirm_totp(&other.tenant, &other.user, &oc)
        .await
        .unwrap();
    assert_eq!(
        id.check_second_factor(&other.tenant, &other.user, Some(&one))
            .await
            .unwrap(),
        TotpOutcome::Failed,
        "a recovery code from another account must never verify"
    );

    // The owner uses it once → Verified; reuse → Failed (single-use).
    assert_eq!(
        id.check_second_factor(&victim.tenant, &victim.user, Some(&one))
            .await
            .unwrap(),
        TotpOutcome::Verified
    );
    assert_eq!(
        id.check_second_factor(&victim.tenant, &victim.user, Some(&one))
            .await
            .unwrap(),
        TotpOutcome::Failed
    );
}

#[tokio::test]
async fn password_login_backs_off_after_repeated_failures() {
    let (store, id) = setup().await;
    let u = make_user(&store, &id, "backoff").await;

    // Burn past the free-attempt allowance with wrong passwords.
    for _ in 0..7 {
        assert!(
            id.password_login(&u.email, "wrong", None)
                .await
                .unwrap()
                .is_none()
        );
    }
    // Now even the CORRECT password is refused while backed off — the
    // endpoint is throttled, not just answering per-attempt.
    assert!(
        id.password_login(&u.email, &u.password, None)
            .await
            .unwrap()
            .is_none(),
        "a backed-off username must be throttled even with the right password"
    );
}

#[tokio::test]
async fn groups_are_tenant_scoped() {
    let (store, id) = setup().await;
    let a = make_user(&store, &id, "grp-a").await;
    let ts = store.for_tenant(a.tenant.clone());

    let g = ts.create_group("engineering").await.unwrap();
    ts.add_group_member(&g, &a.user).await.unwrap();
    let members = ts.group_members(&g).await.unwrap();
    assert_eq!(members, vec![a.user.clone()]);

    // A different tenant's view of that group id is empty (not another
    // tenant's members) — group ids are unguessable and tenant-scoped.
    let b = make_user(&store, &id, "grp-b").await;
    let members_from_b = store
        .for_tenant(b.tenant.clone())
        .group_members(&g)
        .await
        .unwrap();
    assert!(
        members_from_b.is_empty(),
        "a group must be invisible to another tenant"
    );
}

#[tokio::test]
async fn two_factor_is_enforced_on_every_token_issuing_and_legacy_path() {
    let (store, id) = setup().await;
    let u = make_user(&store, &id, "2fa-gate").await;

    // Before 2FA, legacy auth (IMAP/SMTP path) accepts the password.
    assert!(
        id.authenticate_legacy(&u.email, &u.password)
            .await
            .unwrap()
            .is_some(),
        "legacy auth works before 2FA is enabled"
    );

    // Enroll + confirm TOTP.
    let e = id.enroll_totp(&u.tenant, &u.user, &u.email).await.unwrap();
    let code = current_code(&e.secret_base32).unwrap();
    id.confirm_totp(&u.tenant, &u.user, &code).await.unwrap();

    // The token-issuing password_login refuses without the second factor…
    assert!(
        id.password_login(&u.email, &u.password, None)
            .await
            .unwrap()
            .is_none()
    );
    // …but succeeds with it.
    let code = current_code(&e.secret_base32).unwrap();
    assert!(
        id.password_login(&u.email, &u.password, Some(&code))
            .await
            .unwrap()
            .is_some()
    );

    // Legacy protocols (SMTP/IMAP/POP3) FAIL CLOSED for a 2FA account: a
    // bare password can't carry the second factor, so it is refused —
    // indistinguishably from a wrong password (no oracle). The user must use
    // the OIDC flow. This closes the "2FA bypassable over IMAP/SMTP" gap.
    assert!(
        id.authenticate_legacy(&u.email, &u.password)
            .await
            .unwrap()
            .is_none(),
        "legacy auth must refuse a 2FA-enabled account (fail closed)"
    );
}
