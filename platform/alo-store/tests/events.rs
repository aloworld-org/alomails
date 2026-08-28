//! The tenant's event stream (ADR 0058 §5, A4.6): every intent execution is
//! one append-only row, and the two reads answer different questions through
//! different doors.
//!
//! The properties that are the point of the table existing: a record's
//! history shows what was **done to** it and never who looked at it; the
//! caller's own history shows everything, reads included; and neither read
//! ever crosses a tenant or a person, however exactly an id is guessed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{AgentProduct, NewDomainEvent, StoreError};

#[tokio::test]
async fn a_records_history_is_its_writes_and_never_its_readers() {
    let store = common::test_store().await;
    let t = store.create_tenant("events-record").await.unwrap();
    let ts = store.for_tenant(t.clone());
    let anna = ts.create_user("anna@events.test").await.unwrap();
    let a = store.for_account(t.clone(), anna.clone());

    let agent = a
        .create_agent("billing", "Billing", None, AgentProduct::Billing)
        .await
        .unwrap();

    // The agent sent the quote (a write), and earlier looked it up (a read).
    a.emit_event(&NewDomainEvent {
        kind: "quote_lookup",
        effect: "read",
        record_type: Some("quote"),
        record_id: Some("q-77"),
        agent: Some(&agent),
    })
    .await
    .unwrap();
    a.emit_event(&NewDomainEvent {
        kind: "send_quote",
        effect: "write",
        record_type: Some("quote"),
        record_id: Some("q-77"),
        agent: Some(&agent),
    })
    .await
    .unwrap();
    // A list read about no single record joins the stream with no reference.
    a.emit_event(&NewDomainEvent {
        kind: "open_quotes",
        effect: "read",
        record_type: None,
        record_id: None,
        agent: Some(&agent),
    })
    .await
    .unwrap();

    // The record's history: the write, addressed the way the audit trail
    // addresses a record — the bare word the emitter stored is matched by the
    // address's own last segment.
    let history = ts
        .list_record_events("billing.quote", "q-77", 50)
        .await
        .unwrap();
    assert_eq!(history.len(), 1, "the lookup is not part of the story");
    assert_eq!(history[0].kind, "send_quote");
    assert_eq!(history[0].effect, "write");
    assert_eq!(history[0].actor.as_deref(), Some("anna@events.test"));
    assert_eq!(history[0].agent.as_deref(), Some("billing"));

    // The caller's own history: everything, newest first.
    let mine = a.my_events(50).await.unwrap();
    assert_eq!(mine.len(), 3);
    assert_eq!(mine[0].kind, "open_quotes");
    assert!(
        mine.iter()
            .any(|e| e.kind == "quote_lookup" && e.effect == "read")
    );
}

#[tokio::test]
async fn an_event_never_crosses_a_tenant_or_a_person() {
    let store = common::test_store().await;
    let t1 = store.create_tenant("events-t1").await.unwrap();
    let t2 = store.create_tenant("events-t2").await.unwrap();
    let anna = store
        .for_tenant(t1.clone())
        .create_user("anna@ev1.test")
        .await
        .unwrap();
    let ben = store
        .for_tenant(t1.clone())
        .create_user("ben@ev1.test")
        .await
        .unwrap();
    let eve = store
        .for_tenant(t2.clone())
        .create_user("eve@ev2.test")
        .await
        .unwrap();
    let a = store.for_account(t1.clone(), anna);
    let b = store.for_account(t1.clone(), ben);
    let e = store.for_account(t2.clone(), eve);

    a.emit_event(&NewDomainEvent {
        kind: "issue_invoice",
        effect: "write",
        record_type: Some("invoice"),
        record_id: Some("inv-1"),
        agent: None,
    })
    .await
    .unwrap();

    // The other tenant, asking with the exact record id, gets the same answer
    // as for an id that was never issued.
    let stolen = store
        .for_tenant(t2.clone())
        .list_record_events("billing.invoice", "inv-1", 50)
        .await
        .unwrap();
    assert!(stolen.is_empty(), "another tenant read our record's events");
    assert!(e.my_events(50).await.unwrap().is_empty());

    // A colleague of the same tenant sees the record's history (that is what
    // an audit tab is for) but never the actor's personal stream.
    assert_eq!(
        store
            .for_tenant(t1.clone())
            .list_record_events("billing.invoice", "inv-1", 50)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        b.my_events(50).await.unwrap().is_empty(),
        "a colleague read another person's stream"
    );
}

#[tokio::test]
async fn the_stream_refuses_what_is_not_an_event() {
    let store = common::test_store().await;
    let t = store.create_tenant("events-refuse").await.unwrap();
    let anna = store
        .for_tenant(t.clone())
        .create_user("anna@evr.test")
        .await
        .unwrap();
    let a = store.for_account(t.clone(), anna);

    let refused = |result: Result<_, StoreError>| {
        assert!(matches!(result, Err(StoreError::Validation(_))));
    };
    refused(
        a.emit_event(&NewDomainEvent {
            kind: "send_quote",
            effect: "maybe",
            record_type: None,
            record_id: None,
            agent: None,
        })
        .await,
    );
    refused(
        a.emit_event(&NewDomainEvent {
            kind: "Send Quote;--",
            effect: "write",
            record_type: None,
            record_id: None,
            agent: None,
        })
        .await,
    );
    // A record reference is a type and an id together, never half of one.
    refused(
        a.emit_event(&NewDomainEvent {
            kind: "send_quote",
            effect: "write",
            record_type: Some("quote"),
            record_id: None,
            agent: None,
        })
        .await,
    );
    let long = "x".repeat(129);
    refused(
        a.emit_event(&NewDomainEvent {
            kind: "send_quote",
            effect: "write",
            record_type: Some("quote"),
            record_id: Some(&long),
            agent: None,
        })
        .await,
    );
    assert!(
        a.my_events(50).await.unwrap().is_empty(),
        "a refusal left a row"
    );
}
