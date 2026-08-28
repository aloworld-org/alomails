//! App-password isolation (mail M1.1): every operation on the tenant
//! door gets a clean denial across both the tenant boundary and the
//! user boundary inside one tenant, and the pre-tenant username lookup
//! resolves only the rows the unique username index names. Runs against
//! the real Postgres from compose. The crypto half (generation, argon2,
//! the dummy-hash timing seam) is proven in `alo-identity`'s suite; here
//! the hash is an opaque string, which is exactly what the store holds.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{APP_PASSWORDS_MAX, StoreError, TenantId, UserId};

/// Two tenants with one user each (plus a second user in the first
/// tenant), each user with a login username so the pre-tenant lookup has
/// a row to resolve.
struct World {
    store: alo_store::Store,
    tenant_a: TenantId,
    user_a: UserId,
    username_a: String,
    user_a2: UserId,
    tenant_b: TenantId,
    user_b: UserId,
    username_b: String,
}

async fn world(tag: &str) -> World {
    let store = common::test_store().await;
    let tenant_a = store.create_tenant(&format!("ap-{tag}-a")).await.unwrap();
    let tenant_b = store.create_tenant(&format!("ap-{tag}-b")).await.unwrap();
    let ts_a = store.for_tenant(tenant_a.clone());
    let ts_b = store.for_tenant(tenant_b.clone());
    let username_a = format!("a-{tenant_a}@ex.test");
    let username_b = format!("b-{tenant_b}@ex.test");
    let user_a = ts_a.create_user(&username_a).await.unwrap();
    let user_a2 = ts_a
        .create_user(&format!("a2-{tenant_a}@ex.test"))
        .await
        .unwrap();
    let user_b = ts_b.create_user(&username_b).await.unwrap();
    ts_a.set_password_hash(&user_a, &username_a, "phc-a")
        .await
        .unwrap();
    ts_b.set_password_hash(&user_b, &username_b, "phc-b")
        .await
        .unwrap();
    World {
        store,
        tenant_a,
        user_a,
        username_a,
        user_a2,
        tenant_b,
        user_b,
        username_b,
    }
}

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, got data: {value:?}"),
    }
}

#[tokio::test]
async fn create_is_tenant_and_user_scoped() {
    let w = world("create").await;
    // A foreign tenant's door cannot create for this user.
    assert_not_found(
        w.store
            .for_tenant(w.tenant_b.clone())
            .create_app_password(&w.user_a, "laptop", "phc-hash")
            .await,
    );
    // The right door works.
    w.store
        .for_tenant(w.tenant_a.clone())
        .create_app_password(&w.user_a, "laptop", "phc-hash")
        .await
        .unwrap();
}

#[tokio::test]
async fn list_is_tenant_and_user_scoped() {
    let w = world("list").await;
    let ts_a = w.store.for_tenant(w.tenant_a.clone());
    ts_a.create_app_password(&w.user_a, "desk", "phc-1")
        .await
        .unwrap();

    // Another tenant's door cannot list this user at all.
    assert_not_found(
        w.store
            .for_tenant(w.tenant_b.clone())
            .list_app_passwords(&w.user_a)
            .await,
    );
    // A different user in the same tenant sees their own (empty) list,
    // never a neighbour's rows.
    assert!(
        ts_a.list_app_passwords(&w.user_a2)
            .await
            .unwrap()
            .is_empty()
    );

    let rows = ts_a.list_app_passwords(&w.user_a).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "desk");
    assert!(rows[0].last_used_at.is_none());
}

#[tokio::test]
async fn revoke_is_tenant_and_user_scoped_and_immediate() {
    let w = world("revoke").await;
    let ts_a = w.store.for_tenant(w.tenant_a.clone());
    let id = ts_a
        .create_app_password(&w.user_a, "phone", "phc-1")
        .await
        .unwrap();

    // A foreign tenant's door gets the same clean denial as an absent row…
    assert_not_found(
        w.store
            .for_tenant(w.tenant_b.clone())
            .revoke_app_password(&w.user_b, &id)
            .await,
    );
    // …and so does another user of the same tenant.
    assert_not_found(ts_a.revoke_app_password(&w.user_a2, &id).await);
    // Neither denial deleted anything.
    assert_eq!(ts_a.list_app_passwords(&w.user_a).await.unwrap().len(), 1);

    // The owner's revoke deletes the row (and with it the hash).
    ts_a.revoke_app_password(&w.user_a, &id).await.unwrap();
    assert!(ts_a.list_app_passwords(&w.user_a).await.unwrap().is_empty());
    assert!(
        w.store
            .app_password_credentials_by_username(&w.username_a)
            .await
            .unwrap()
            .is_empty()
    );
    // Revoking again: the same clean NotFound.
    assert_not_found(ts_a.revoke_app_password(&w.user_a, &id).await);
}

#[tokio::test]
async fn username_lookup_resolves_one_user_only() {
    let w = world("lookup").await;
    w.store
        .for_tenant(w.tenant_a.clone())
        .create_app_password(&w.user_a, "desk", "phc-a-1")
        .await
        .unwrap();
    w.store
        .for_tenant(w.tenant_b.clone())
        .create_app_password(&w.user_b, "desk", "phc-b-1")
        .await
        .unwrap();

    // Each username resolves to exactly its own user's rows.
    let rows_a = w
        .store
        .app_password_credentials_by_username(&w.username_a)
        .await
        .unwrap();
    assert_eq!(rows_a.len(), 1);
    assert_eq!(rows_a[0].tenant, w.tenant_a);
    assert_eq!(rows_a[0].user, w.user_a);
    assert_eq!(rows_a[0].password_hash, "phc-a-1");

    let rows_b = w
        .store
        .app_password_credentials_by_username(&w.username_b)
        .await
        .unwrap();
    assert_eq!(rows_b.len(), 1);
    assert_eq!(rows_b[0].tenant, w.tenant_b);

    // An unknown username resolves to nothing — not an error.
    assert!(
        w.store
            .app_password_credentials_by_username("nobody@ex.test")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn touch_stamps_last_used() {
    let w = world("touch").await;
    let ts_a = w.store.for_tenant(w.tenant_a.clone());
    let id = ts_a
        .create_app_password(&w.user_a, "tablet", "phc-1")
        .await
        .unwrap();
    w.store.touch_app_password(&id).await.unwrap();
    let rows = ts_a.list_app_passwords(&w.user_a).await.unwrap();
    assert!(rows[0].last_used_at.is_some());
}

#[tokio::test]
async fn name_is_validated_and_the_cap_holds() {
    let w = world("cap").await;
    let ts_a = w.store.for_tenant(w.tenant_a.clone());

    // An empty (or whitespace) name is a validation error the caller can fix.
    assert!(matches!(
        ts_a.create_app_password(&w.user_a, "   ", "phc").await,
        Err(StoreError::Validation(_))
    ));
    let long = "n".repeat(101);
    assert!(matches!(
        ts_a.create_app_password(&w.user_a, &long, "phc").await,
        Err(StoreError::Validation(_))
    ));

    // The per-user cap bounds how much argon2 work one login can cost.
    for i in 0..APP_PASSWORDS_MAX {
        ts_a.create_app_password(&w.user_a, &format!("device {i}"), "phc")
            .await
            .unwrap();
    }
    assert!(matches!(
        ts_a.create_app_password(&w.user_a, "one too many", "phc")
            .await,
        Err(StoreError::Conflict(_))
    ));
    // The cap is per user, not per tenant: a neighbour still has room.
    ts_a.create_app_password(&w.user_a2, "their first", "phc")
        .await
        .unwrap();
}
