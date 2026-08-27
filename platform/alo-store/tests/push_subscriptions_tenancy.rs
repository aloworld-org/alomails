//! Push-subscription isolation (mail M5.3): every operation on the tenant
//! door gets a clean denial across both the tenant boundary and the user
//! boundary inside one tenant. Runs against the real Postgres from compose.
//! The crypto half (VAPID, RFC 8291) is proven in `alo-jmap`'s suite; here
//! the key material is opaque text, which is exactly what the store holds.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{PUSH_SUBSCRIPTIONS_MAX, StoreError, TenantId, UserId};

/// Two tenants with one user each, plus a second user in the first tenant.
struct World {
    store: alo_store::Store,
    tenant_a: TenantId,
    user_a: UserId,
    user_a2: UserId,
    tenant_b: TenantId,
    user_b: UserId,
}

async fn world(tag: &str) -> World {
    let store = common::test_store().await;
    let tenant_a = store.create_tenant(&format!("push-{tag}-a")).await.unwrap();
    let tenant_b = store.create_tenant(&format!("push-{tag}-b")).await.unwrap();
    let ts_a = store.for_tenant(tenant_a.clone());
    let ts_b = store.for_tenant(tenant_b.clone());
    let user_a = ts_a
        .create_user(&format!("a-{tenant_a}@ex.test"))
        .await
        .unwrap();
    let user_a2 = ts_a
        .create_user(&format!("a2-{tenant_a}@ex.test"))
        .await
        .unwrap();
    let user_b = ts_b
        .create_user(&format!("b-{tenant_b}@ex.test"))
        .await
        .unwrap();
    World {
        store,
        tenant_a,
        user_a,
        user_a2,
        tenant_b,
        user_b,
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
    // A foreign tenant's door cannot subscribe for this user.
    assert_not_found(
        w.store
            .for_tenant(w.tenant_b.clone())
            .create_push_subscription(&w.user_a, "https://push.example/x", "pk", "auth")
            .await,
    );
    // The right door works.
    w.store
        .for_tenant(w.tenant_a.clone())
        .create_push_subscription(&w.user_a, "https://push.example/x", "pk", "auth")
        .await
        .unwrap();
}

#[tokio::test]
async fn resubscribing_the_same_endpoint_replaces_not_duplicates() {
    let w = world("upsert").await;
    let ts_a = w.store.for_tenant(w.tenant_a.clone());
    let first = ts_a
        .create_push_subscription(&w.user_a, "https://push.example/dev1", "pk-1", "auth-1")
        .await
        .unwrap();
    // The same device again, with rotated keys: same row, same id, new keys.
    let second = ts_a
        .create_push_subscription(&w.user_a, "https://push.example/dev1", "pk-2", "auth-2")
        .await
        .unwrap();
    assert_eq!(first, second);
    let deliveries = ts_a.push_deliveries(&w.user_a).await.unwrap();
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].p256dh, "pk-2");
    assert_eq!(deliveries[0].auth, "auth-2");
    // The SAME endpoint under a different user of the same tenant is a
    // different device row — the unique key includes the user.
    ts_a.create_push_subscription(&w.user_a2, "https://push.example/dev1", "pk-3", "auth-3")
        .await
        .unwrap();
    assert_eq!(ts_a.push_deliveries(&w.user_a).await.unwrap().len(), 1);
    assert_eq!(ts_a.push_deliveries(&w.user_a2).await.unwrap().len(), 1);
}

#[tokio::test]
async fn list_and_deliveries_are_tenant_and_user_scoped() {
    let w = world("list").await;
    let ts_a = w.store.for_tenant(w.tenant_a.clone());
    ts_a.create_push_subscription(&w.user_a, "https://push.example/desk", "pk", "auth")
        .await
        .unwrap();

    // Another tenant's door cannot list this user at all.
    assert_not_found(
        w.store
            .for_tenant(w.tenant_b.clone())
            .list_push_subscriptions(&w.user_a)
            .await,
    );
    assert_not_found(
        w.store
            .for_tenant(w.tenant_b.clone())
            .push_deliveries(&w.user_a)
            .await,
    );
    // A different user in the same tenant sees their own (empty) list,
    // never a neighbour's rows.
    assert!(
        ts_a.list_push_subscriptions(&w.user_a2)
            .await
            .unwrap()
            .is_empty()
    );

    let rows = ts_a.list_push_subscriptions(&w.user_a).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].endpoint, "https://push.example/desk");
}

#[tokio::test]
async fn delete_is_tenant_and_user_scoped_and_immediate() {
    let w = world("delete").await;
    let ts_a = w.store.for_tenant(w.tenant_a.clone());
    let id = ts_a
        .create_push_subscription(&w.user_a, "https://push.example/phone", "pk", "auth")
        .await
        .unwrap();

    // A foreign tenant's door gets the same clean denial as an absent row…
    assert_not_found(
        w.store
            .for_tenant(w.tenant_b.clone())
            .delete_push_subscription(&w.user_b, &id)
            .await,
    );
    // …and so does another user of the same tenant.
    assert_not_found(ts_a.delete_push_subscription(&w.user_a2, &id).await);
    // Neither denial deleted anything.
    assert_eq!(ts_a.push_deliveries(&w.user_a).await.unwrap().len(), 1);

    // The owner's delete removes the row: deliveries stop with it.
    ts_a.delete_push_subscription(&w.user_a, &id).await.unwrap();
    assert!(ts_a.push_deliveries(&w.user_a).await.unwrap().is_empty());
    // Deleting again: the same clean NotFound.
    assert_not_found(ts_a.delete_push_subscription(&w.user_a, &id).await);
}

#[tokio::test]
async fn a_dead_endpoint_drop_is_by_unguessable_id_only() {
    let w = world("dead").await;
    let ts_a = w.store.for_tenant(w.tenant_a.clone());
    let id = ts_a
        .create_push_subscription(&w.user_a, "https://push.example/gone", "pk", "auth")
        .await
        .unwrap();
    // The dispatcher's cleanup drops the row it just failed to deliver to;
    // dropping it twice is silent — the row is gone either way.
    w.store.drop_dead_push_subscription(&id).await.unwrap();
    w.store.drop_dead_push_subscription(&id).await.unwrap();
    assert!(ts_a.push_deliveries(&w.user_a).await.unwrap().is_empty());
}

#[tokio::test]
async fn fields_are_validated_and_the_cap_holds() {
    let w = world("cap").await;
    let ts_a = w.store.for_tenant(w.tenant_a.clone());

    // Empty or overlong fields are validation errors the caller can fix.
    assert!(matches!(
        ts_a.create_push_subscription(&w.user_a, "   ", "pk", "auth")
            .await,
        Err(StoreError::Validation(_))
    ));
    let long_endpoint = format!("https://push.example/{}", "e".repeat(2000));
    assert!(matches!(
        ts_a.create_push_subscription(&w.user_a, &long_endpoint, "pk", "auth")
            .await,
        Err(StoreError::Validation(_))
    ));
    assert!(matches!(
        ts_a.create_push_subscription(&w.user_a, "https://push.example/x", "", "auth")
            .await,
        Err(StoreError::Validation(_))
    ));
    assert!(matches!(
        ts_a.create_push_subscription(&w.user_a, "https://push.example/x", "pk", &"a".repeat(513))
            .await,
        Err(StoreError::Validation(_))
    ));

    // The cap bounds how many external POSTs one state change fans out to.
    for i in 0..PUSH_SUBSCRIPTIONS_MAX {
        ts_a.create_push_subscription(&w.user_a, &format!("https://push.example/d{i}"), "pk", "a")
            .await
            .unwrap();
    }
    assert!(matches!(
        ts_a.create_push_subscription(&w.user_a, "https://push.example/one-more", "pk", "a")
            .await,
        Err(StoreError::Conflict(_))
    ));
    // Refreshing an EXISTING device at the cap still works — it makes no
    // new row.
    ts_a.create_push_subscription(&w.user_a, "https://push.example/d0", "pk-new", "a-new")
        .await
        .unwrap();
    // The cap is per user, not per tenant: a neighbour still has room.
    ts_a.create_push_subscription(&w.user_a2, "https://push.example/theirs", "pk", "a")
        .await
        .unwrap();
}
