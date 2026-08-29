//! Where a record came from (ADR 0058 §4, A4.5): set once at creation, the
//! first writer wins, and a tenant's provenance is never another tenant's to
//! read — however exactly the record's id is guessed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::StoreError;

/// Provenance is a fact about a record's creation: the first writer says it,
/// a later writer changes nothing, and the read answers the first word.
#[tokio::test]
async fn the_first_writer_wins_and_the_answer_stands() {
    let store = common::test_store().await;
    let t = store.create_tenant("origin-t").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let ua = ts.create_user("anna@origin.test").await.unwrap();
    let a = store.for_account(t.clone(), ua.clone());

    // Nothing said yet: no origin, not an error.
    assert!(a.record_origin("invoice", "inv-1").await.unwrap().is_none());

    // The module's own word, written where the record was created…
    assert!(
        a.set_record_origin(
            "invoice",
            "inv-1",
            "quote",
            "q-1",
            Some("  QUO-2026-00007  ")
        )
        .await
        .unwrap()
    );
    // …and the generic stamp arriving after it quietly loses.
    assert!(
        !a.set_record_origin("invoice", "inv-1", "thread", "ch-1", Some("finance"))
            .await
            .unwrap()
    );

    let kept = a.record_origin("invoice", "inv-1").await.unwrap().unwrap();
    assert_eq!(kept.kind, "quote");
    assert_eq!(kept.id, "q-1");
    // The label was trimmed on the way in — it is a citation, not a document.
    assert_eq!(kept.label.as_deref(), Some("QUO-2026-00007"));

    // A source with no name of its own still leaves the pointer.
    assert!(
        a.set_record_origin("task", "t-1", "thread", "dm-1", None)
            .await
            .unwrap()
    );
    let bare = a.record_origin("task", "t-1").await.unwrap().unwrap();
    assert_eq!(bare.kind, "thread");
    assert!(bare.label.is_none());
    // A blank label is no label, not an empty citation.
    assert!(
        a.set_record_origin("task", "t-2", "thread", "dm-1", Some("   "))
            .await
            .unwrap()
    );
    assert!(
        a.record_origin("task", "t-2")
            .await
            .unwrap()
            .unwrap()
            .label
            .is_none()
    );
}

/// The vocabulary is the event stream's, and an id has the event stream's
/// bounds — so the three record ledgers (events, actions, origins) can never
/// drift into needing a translator.
#[tokio::test]
async fn the_vocabulary_and_bounds_are_the_event_streams() {
    let store = common::test_store().await;
    let t = store.create_tenant("origin-v").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let ua = ts.create_user("vera@origin.test").await.unwrap();
    let a = store.for_account(t.clone(), ua.clone());

    for (record_type, kind) in [
        ("Invoice", "quote"),
        ("invoice", "Quote"),
        ("drop;--", "quote"),
    ] {
        assert!(matches!(
            a.set_record_origin(record_type, "inv-1", kind, "q-1", None)
                .await,
            Err(StoreError::Validation(_))
        ));
    }
    for (record_id, id) in [("", "q-1"), ("inv-1", ""), (&"x".repeat(129), "q-1")] {
        assert!(matches!(
            a.set_record_origin("invoice", record_id, "quote", id, None)
                .await,
            Err(StoreError::Validation(_))
        ));
    }
}

/// The wrong-tenant test, which is the one that matters: tenant B asking
/// about tenant A's record — its exact type and id — reads nothing, and B
/// writing its own provenance under the same words collides with nothing.
#[tokio::test]
async fn another_tenants_provenance_is_not_there_to_read_or_to_squat() {
    let store = common::test_store().await;
    let ta = store.create_tenant("origin-a").await.unwrap();
    let ua = store
        .for_tenant(ta.clone())
        .create_user("anna@origin-a.test")
        .await
        .unwrap();
    let a = store.for_account(ta.clone(), ua);
    let tb = store.create_tenant("origin-b").await.unwrap();
    let ub = store
        .for_tenant(tb.clone())
        .create_user("bram@origin-b.test")
        .await
        .unwrap();
    let b = store.for_account(tb.clone(), ub);

    assert!(
        a.set_record_origin("invoice", "inv-shared", "quote", "q-a", Some("QUO-A"))
            .await
            .unwrap()
    );

    // B guessing A's exact record address reads nothing at all.
    assert!(
        b.record_origin("invoice", "inv-shared")
            .await
            .unwrap()
            .is_none()
    );
    // And B's own record under the same words is B's row, not a conflict
    // with A's — the uniqueness of "set once" is per tenant, like the data.
    assert!(
        b.set_record_origin("invoice", "inv-shared", "thread", "ch-b", None)
            .await
            .unwrap()
    );
    assert_eq!(
        a.record_origin("invoice", "inv-shared")
            .await
            .unwrap()
            .unwrap()
            .kind,
        "quote"
    );
    assert_eq!(
        b.record_origin("invoice", "inv-shared")
            .await
            .unwrap()
            .unwrap()
            .kind,
        "thread"
    );
}
