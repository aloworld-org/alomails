//! Tenancy proof for alo Docs documents (Law 1: isolation is tested, not
//! assumed). Two users in one tenant and a user in a second tenant exercise the
//! `(tenant, owner)` predicate on every document operation: a user can only see,
//! load, save, and delete their own documents; another user's — same tenant or
//! not — is an indistinguishable "not found". Runs against a throwaway Postgres;
//! skips cleanly when none is available.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use alo_store::error::StoreError;
use alo_store::{BlobStore, Store};
use sqlx::postgres::PgPoolOptions;

/// The database this suite runs against.
///
/// Delegates to `alo_test_db`, which refuses the database the product
/// runs on: suites create and drop their own, they never write into `alo`.
fn database_url() -> String {
    alo_test_db::url()
}

#[tokio::test]
async fn documents_are_owner_and_tenant_scoped() {
    let Ok(pool) = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url())
        .await
    else {
        eprintln!("SKIP: no database at {}", database_url());
        return;
    };
    let store = Arc::new(Store::new(pool, BlobStore::in_memory(8 * 1024 * 1024)));
    store.migrate().await.unwrap();

    // Tenant A with two users; tenant B with one.
    let ta = store.create_tenant("docs-a").await.unwrap();
    let tsa = store.for_tenant(ta.clone());
    let alice = tsa.create_user("alice@docs-a.test").await.unwrap();
    let bob = tsa.create_user("bob@docs-a.test").await.unwrap();
    let tb = store.create_tenant("docs-b").await.unwrap();
    let carol = store
        .for_tenant(tb.clone())
        .create_user("carol@docs-b.test")
        .await
        .unwrap();

    let alice_store = store.for_account(ta.clone(), alice.clone());
    let bob_store = store.for_account(ta.clone(), bob.clone());
    let carol_store = store.for_account(tb.clone(), carol.clone());

    // Alice creates a document.
    let doc = alice_store.create_document("Alice's spec").await.unwrap();
    assert_eq!(doc.title, "Alice's spec");
    assert_eq!(doc.blocks, "[]");

    // Alice sees it; Bob (same tenant) and Carol (other tenant) do not.
    assert_eq!(alice_store.list_documents().await.unwrap().len(), 1);
    assert!(bob_store.list_documents().await.unwrap().is_empty());
    assert!(carol_store.list_documents().await.unwrap().is_empty());

    // Only Alice can load it by id.
    assert!(alice_store.get_document(&doc.id).await.unwrap().is_some());
    assert!(bob_store.get_document(&doc.id).await.unwrap().is_none());
    assert!(carol_store.get_document(&doc.id).await.unwrap().is_none());

    // Bob and Carol cannot save into Alice's document — NotFound, not a silent
    // cross-account write.
    let blocks = r#"[{"type":"heading","id":"h1","level":1,"text":"pwned"}]"#;
    assert!(matches!(
        bob_store.save_document(&doc.id, "hacked", blocks).await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        carol_store.save_document(&doc.id, "hacked", blocks).await,
        Err(StoreError::NotFound)
    ));
    // Alice's document is untouched.
    let after = alice_store.get_document(&doc.id).await.unwrap().unwrap();
    assert_eq!(after.title, "Alice's spec");
    assert_eq!(after.blocks, "[]");

    // Alice saves real blocks; the round-trip preserves them.
    alice_store
        .save_document(&doc.id, "Alice's spec v2", blocks)
        .await
        .unwrap();
    let saved = alice_store.get_document(&doc.id).await.unwrap().unwrap();
    assert_eq!(saved.title, "Alice's spec v2");
    assert!(saved.blocks.contains("heading"));

    // Bob and Carol cannot delete it; Alice can.
    assert!(matches!(
        bob_store.delete_document(&doc.id).await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        carol_store.delete_document(&doc.id).await,
        Err(StoreError::NotFound)
    ));
    alice_store.delete_document(&doc.id).await.unwrap();
    assert!(alice_store.get_document(&doc.id).await.unwrap().is_none());
    assert!(matches!(
        alice_store.delete_document(&doc.id).await,
        Err(StoreError::NotFound)
    ));

    store.delete_tenant(&ta).await.unwrap();
    store.delete_tenant(&tb).await.unwrap();
}
