//! Categories (colored message labels): the catalog CRUD, the fact that
//! deleting a category strips its `$category_<id>` keyword from tagged
//! messages, and that categories are per-account (one user never sees or
//! touches another's).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common::{fresh_account, tenant_fixture, test_store};
use alo_store::{StoreError, category_keyword};

#[tokio::test]
async fn create_list_and_unique_name() {
    let store = test_store().await;
    let (acc, _u, _inbox) = fresh_account(&store, "cat-crud").await;

    let work = acc.create_category("Work", Some("#3f7cac")).await.unwrap();
    let personal = acc.create_category("Personal", None).await.unwrap();

    let list = acc.categories().await.unwrap();
    assert_eq!(list.len(), 2);
    // Ordered by sort_order: Work was created first.
    assert_eq!(list[0].id, work);
    assert_eq!(list[0].name, "Work");
    assert_eq!(list[0].color.as_deref(), Some("#3f7cac"));
    assert_eq!(list[1].id, personal);
    assert_eq!(list[1].color, None);

    // A duplicate name for the same user is a conflict, not a silent success.
    let dup = acc.create_category("Work", None).await;
    assert!(matches!(dup, Err(StoreError::Conflict(_))), "got {dup:?}");
}

#[tokio::test]
async fn update_name_and_color() {
    let store = test_store().await;
    let (acc, _u, _inbox) = fresh_account(&store, "cat-update").await;

    let id = acc.create_category("Draft", None).await.unwrap();
    acc.update_category(&id, "Final", Some("#5b8a72"))
        .await
        .unwrap();

    let list = acc.categories().await.unwrap();
    assert_eq!(list[0].name, "Final");
    assert_eq!(list[0].color.as_deref(), Some("#5b8a72"));

    // Clearing the color.
    acc.update_category(&id, "Final", None).await.unwrap();
    assert_eq!(acc.categories().await.unwrap()[0].color, None);
}

#[tokio::test]
async fn delete_strips_the_keyword_from_tagged_messages() {
    let store = test_store().await;
    let fx = tenant_fixture(&store, "cat-strip").await;

    let id = fx
        .acc
        .create_category("Receipts", Some("#c07a3e"))
        .await
        .unwrap();
    let kw = category_keyword(&id);

    // Tag the message, as the client would via Email/set keywords/<kw>.
    fx.acc.set_keyword(&fx.message, &kw, true).await.unwrap();
    assert!(fx.acc.keywords(&fx.message).await.unwrap().contains(&kw));

    // Deleting the category removes both the catalog row and every tag.
    fx.acc.delete_category(&id).await.unwrap();
    assert!(acc_has_no_category(&fx.acc, &id).await);
    assert!(
        !fx.acc.keywords(&fx.message).await.unwrap().contains(&kw),
        "the dangling $category keyword must be gone",
    );
}

#[tokio::test]
async fn categories_are_per_account() {
    let store = test_store().await;
    let (alice, _ua, _ia) = fresh_account(&store, "cat-alice").await;
    let (bob, _ub, _ib) = fresh_account(&store, "cat-bob").await;

    let id = alice.create_category("Secret", None).await.unwrap();

    // Bob's catalog is empty and untouched by Alice's create.
    assert!(bob.categories().await.unwrap().is_empty());

    // Bob cannot mutate or destroy Alice's category — it is NotFound to him.
    assert!(matches!(
        bob.update_category(&id, "Hijacked", None).await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        bob.delete_category(&id).await,
        Err(StoreError::NotFound)
    ));

    // Alice's category is intact after Bob's attempts.
    assert_eq!(alice.categories().await.unwrap().len(), 1);
}

/// True if `id` is absent from the account's catalog.
async fn acc_has_no_category(acc: &alo_store::AccountStore, id: &alo_store::CategoryId) -> bool {
    acc.categories().await.unwrap().iter().all(|c| &c.id != id)
}
