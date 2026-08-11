//! Storage boundary for restricted alo Sites collaborators.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{StoreError, TenantRole};

use common::test_store;

fn subdomain(tag: &str, tenant: &alo_store::TenantId) -> String {
    let salt: String = tenant
        .as_str()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|value| value.to_ascii_lowercase())
        .take(20)
        .collect();
    format!("{tag}{salt}")
}

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    assert!(matches!(result, Err(StoreError::NotFound)), "{result:?}");
}

#[tokio::test]
async fn grants_are_per_site_revocable_and_keep_the_restricted_role_in_step() {
    let store = test_store().await;
    let tenant = store.create_tenant("site-editors").await.unwrap();
    let ts = store.for_tenant(tenant.clone());
    let owner = ts.create_user("owner@site-editors.test").await.unwrap();
    let editor = ts.create_user("editor@site-editors.test").await.unwrap();
    let owner_door = store.for_account(tenant.clone(), owner.clone());
    let editor_door = store.for_account(tenant.clone(), editor.clone());
    let first = owner_door
        .create_site("First", &subdomain("first", &tenant))
        .await
        .unwrap();
    let second = owner_door
        .create_site("Second", &subdomain("second", &tenant))
        .await
        .unwrap();

    ts.grant_site_editor(&editor, &first, &owner).await.unwrap();
    ts.grant_site_editor(&editor, &second, &owner)
        .await
        .unwrap();
    ts.grant_site_editor(&editor, &first, &owner).await.unwrap();

    assert!(
        editor_door
            .access_facts()
            .await
            .unwrap()
            .has(TenantRole::SiteEditor)
    );
    assert!(editor_door.can_edit_site(&first).await.unwrap());
    assert!(editor_door.can_edit_site(&second).await.unwrap());
    assert_eq!(editor_door.editable_sites().await.unwrap().len(), 2);
    assert_eq!(ts.site_editor_grants(&editor).await.unwrap().len(), 2);

    ts.revoke_site_editor(&editor, &first).await.unwrap();
    assert!(!editor_door.can_edit_site(&first).await.unwrap());
    assert!(
        editor_door
            .access_facts()
            .await
            .unwrap()
            .has(TenantRole::SiteEditor)
    );

    ts.revoke_site_editor(&editor, &second).await.unwrap();
    assert!(
        !editor_door
            .access_facts()
            .await
            .unwrap()
            .has(TenantRole::SiteEditor)
    );
    assert!(editor_door.editable_sites().await.unwrap().is_empty());

    let last = owner_door
        .create_site("Last", &subdomain("last", &tenant))
        .await
        .unwrap();
    ts.grant_site_editor(&editor, &last, &owner).await.unwrap();
    owner_door.delete_site(&last).await.unwrap();
    assert!(
        !editor_door
            .access_facts()
            .await
            .unwrap()
            .has(TenantRole::SiteEditor),
        "deleting the final granted site must not strand a restricted account"
    );
}

#[tokio::test]
async fn a_grant_cannot_cross_either_the_user_or_site_tenant_boundary() {
    let store = test_store().await;
    let ours = store.create_tenant("site-editors-ours").await.unwrap();
    let theirs = store.create_tenant("site-editors-theirs").await.unwrap();
    let our_ts = store.for_tenant(ours.clone());
    let their_ts = store.for_tenant(theirs.clone());
    let owner = our_ts.create_user("owner@ours.test").await.unwrap();
    let editor = our_ts.create_user("editor@ours.test").await.unwrap();
    let outsider = their_ts.create_user("editor@theirs.test").await.unwrap();
    let our_site = store
        .for_account(ours.clone(), owner.clone())
        .create_site("Ours", &subdomain("ours", &ours))
        .await
        .unwrap();
    let their_site = store
        .for_account(theirs.clone(), outsider.clone())
        .create_site("Theirs", &subdomain("theirs", &theirs))
        .await
        .unwrap();

    assert_not_found(our_ts.grant_site_editor(&outsider, &our_site, &owner).await);
    assert_not_found(our_ts.grant_site_editor(&editor, &their_site, &owner).await);
    assert_not_found(
        our_ts
            .grant_site_editor(&editor, &our_site, &outsider)
            .await,
    );
    assert_not_found(our_ts.revoke_site_editor(&editor, &their_site).await);
    assert!(our_ts.site_editor_grants(&editor).await.unwrap().is_empty());
    assert!(
        !store
            .for_account(ours, editor)
            .can_edit_site(&their_site)
            .await
            .unwrap()
    );
}
