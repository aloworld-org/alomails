//! Stable MAPI identifiers (ADR 0051, stage 8).
//!
//! Each property here fails *silently* in production if it stops holding: a
//! repeated counter makes a message permanently invisible in Outlook, a counter
//! that moves breaks a client's replica, and a counter that resolves across
//! accounts is a tenant leak.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::mapi_ids::{FIRST_ALLOCATABLE, MapiKind};

/// Asking twice must answer the same thing — a client keeps the id for years.
#[tokio::test]
async fn a_counter_is_allocated_once_and_never_moves() {
    let store = common::test_store().await;
    let (acc, _user, _inbox) = common::fresh_account(&store, "mapi-stable").await;

    let first = acc
        .mapi_counter_for(MapiKind::Message, "m-1")
        .await
        .unwrap();
    let again = acc
        .mapi_counter_for(MapiKind::Message, "m-1")
        .await
        .unwrap();

    assert_eq!(first, again, "the same object was given two different ids");
    assert!(
        first >= FIRST_ALLOCATABLE,
        "allocated {first}, inside the reserved special-folder band"
    );
}

/// Distinct objects must never share a counter — including across the two
/// kinds, which deliberately draw from one space.
#[tokio::test]
async fn distinct_objects_never_share_a_counter() {
    let store = common::test_store().await;
    let (acc, _user, _inbox) = common::fresh_account(&store, "mapi-unique").await;

    let mut seen = std::collections::HashSet::new();
    for n in 0..40 {
        let counter = acc
            .mapi_counter_for(MapiKind::Message, &format!("m-{n}"))
            .await
            .unwrap();
        assert!(seen.insert(counter), "counter {counter} was issued twice");
    }
    for n in 0..10 {
        let counter = acc
            .mapi_counter_for(MapiKind::Folder, &format!("f-{n}"))
            .await
            .unwrap();
        assert!(
            seen.insert(counter),
            "a folder took a counter a message already had"
        );
    }
}

/// The reverse lookup is what a synchronising client's id set needs.
#[tokio::test]
async fn a_counter_resolves_back_to_its_object() {
    let store = common::test_store().await;
    let (acc, _user, _inbox) = common::fresh_account(&store, "mapi-reverse").await;

    let counter = acc
        .mapi_counter_for(MapiKind::Message, "m-42")
        .await
        .unwrap();

    assert_eq!(
        acc.mapi_store_id_for(MapiKind::Message, counter)
            .await
            .unwrap()
            .as_deref(),
        Some("m-42")
    );

    // A counter this account never issued is unknown.
    assert_eq!(
        acc.mapi_store_id_for(MapiKind::Message, 999_999)
            .await
            .unwrap(),
        None
    );

    // The kind is part of the identity: the same counter is not a folder.
    assert_eq!(
        acc.mapi_store_id_for(MapiKind::Folder, counter)
            .await
            .unwrap(),
        None,
        "a message resolved as a folder"
    );
}

/// The batch form must agree with the single form, and must allocate for
/// anything it has not seen — a folder listing calls it for every row.
#[tokio::test]
async fn the_batch_form_agrees_with_the_single_form() {
    let store = common::test_store().await;
    let (acc, _user, _inbox) = common::fresh_account(&store, "mapi-batch").await;

    let known = acc
        .mapi_counter_for(MapiKind::Message, "m-a")
        .await
        .unwrap();

    let ids: Vec<String> = ["m-a", "m-b", "m-c"]
        .iter()
        .map(|id| (*id).to_owned())
        .collect();
    let batch = acc
        .mapi_counters_for(MapiKind::Message, &ids)
        .await
        .unwrap();

    assert_eq!(batch.len(), 3);
    assert_eq!(
        batch.get("m-a"),
        Some(&known),
        "an existing id was reissued"
    );

    // And the reverse batch round-trips every one of them.
    let counters: Vec<i64> = batch.values().copied().collect();
    let back = acc
        .mapi_store_ids_for(MapiKind::Message, &counters)
        .await
        .unwrap();
    assert_eq!(back.len(), 3);
    for (store_id, counter) in &batch {
        assert_eq!(back.get(counter), Some(store_id));
    }

    assert!(
        acc.mapi_counters_for(MapiKind::Message, &[])
            .await
            .unwrap()
            .is_empty()
    );
}

/// The mandatory wrong-tenant test. `AccountStore` makes a cross-account call
/// unrepresentable, so this proves the *data* agrees with the type: B's handle,
/// asked about A's counter, learns nothing.
#[tokio::test]
async fn a_counter_never_resolves_across_accounts() {
    let store = common::test_store().await;
    let (acc_a, _user_a, _ia) = common::fresh_account(&store, "mapi-tenant-a").await;
    let (acc_b, _user_b, _ib) = common::fresh_account(&store, "mapi-tenant-b").await;

    let secret = acc_a
        .mapi_counter_for(MapiKind::Message, "a-private-message")
        .await
        .unwrap();

    assert_eq!(
        acc_b
            .mapi_store_id_for(MapiKind::Message, secret)
            .await
            .unwrap(),
        None,
        "tenant B resolved tenant A's message id"
    );
    assert!(
        acc_b
            .mapi_store_ids_for(MapiKind::Message, &[secret])
            .await
            .unwrap()
            .is_empty(),
        "the batch lookup leaked across tenants"
    );

    // Each account numbers independently, so B may hold the very same counter
    // for its own message without that meaning anything about A's.
    let b_counter = acc_b
        .mapi_counter_for(MapiKind::Message, "b-own-message")
        .await
        .unwrap();
    assert_eq!(
        acc_b
            .mapi_store_id_for(MapiKind::Message, b_counter)
            .await
            .unwrap()
            .as_deref(),
        Some("b-own-message")
    );
    // ...and A is still unaffected by anything B did.
    assert_eq!(
        acc_a
            .mapi_store_id_for(MapiKind::Message, secret)
            .await
            .unwrap()
            .as_deref(),
        Some("a-private-message")
    );
}

/// Forgetting an object must not return its counter to the pool: a client may
/// still hold it, and reissuing it would silently point at different mail.
#[tokio::test]
async fn a_forgotten_counter_is_never_reissued() {
    let store = common::test_store().await;
    let (acc, _user, _inbox) = common::fresh_account(&store, "mapi-forget").await;

    let gone = acc
        .mapi_counter_for(MapiKind::Message, "m-x")
        .await
        .unwrap();
    acc.mapi_forget(MapiKind::Message, "m-x").await.unwrap();

    assert_eq!(
        acc.mapi_store_id_for(MapiKind::Message, gone)
            .await
            .unwrap(),
        None,
        "a forgotten object still resolved"
    );

    let next = acc
        .mapi_counter_for(MapiKind::Message, "m-y")
        .await
        .unwrap();
    assert_ne!(next, gone, "a released counter was handed to a new message");
}
