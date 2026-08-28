//! Tenancy proof for the business audit trail (B2.13; Law 1: isolation is
//! tested, not assumed).
//!
//! An audit entry is the one record type whose whole job is to say what someone
//! did — so a leak here is worse than a leak of the record it describes: it
//! names a person, a moment and an act. Two tenants writing entries about
//! records that happen to share an id is not a contrived case either, because
//! `entity_id` is an id from *another* table and nothing at the database level
//! ties it to a tenant. The tenant clause in the read is the whole guard, and
//! this is where it is proved.
//!
//! Also proved here: the log is append-only in the only sense the code can
//! promise — the entries a tenant has written are still there, unchanged and in
//! order, after later writes.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alo_store::{Store, TenantId, UserId};

use crate::common::test_store;

/// A tenant with one user, and the ids to act as them.
async fn a_tenant(store: &Store, tag: &str) -> (TenantId, UserId, String) {
    let tenant = store.create_tenant(&format!("audit-{tag}")).await.unwrap();
    let email = format!("{tag}-{tenant}@example.test");
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&email)
        .await
        .unwrap();
    (tenant, user, email)
}

#[tokio::test]
async fn a_records_history_is_only_ever_its_own_tenants() {
    let store = test_store().await;
    let (a, a_user, a_email) = a_tenant(&store, "a").await;
    let (b, b_user, b_email) = a_tenant(&store, "b").await;

    // The same entity id in both tenants — an id is unique within a tenant, and
    // nothing stops two tenants' ids from colliding in this column.
    let shared_id = "inv-collision";
    store
        .record_entity_audit(
            &a,
            Some(&a_user),
            "billing.invoice.issue",
            "billing.invoice",
            Some(shared_id),
            Some("/billing/invoices/inv-collision/issue"),
        )
        .await
        .unwrap();
    store
        .record_entity_audit(
            &b,
            Some(&b_user),
            "billing.invoice.void",
            "billing.invoice",
            Some(shared_id),
            Some("/billing/invoices/inv-collision/void"),
        )
        .await
        .unwrap();

    let seen_by_a = store
        .for_tenant(a.clone())
        .list_entity_audit("billing.invoice", shared_id, 100)
        .await
        .unwrap();
    assert_eq!(seen_by_a.len(), 1, "{seen_by_a:?}");
    assert_eq!(seen_by_a[0].action, "billing.invoice.issue");
    assert_eq!(seen_by_a[0].actor.as_deref(), Some(a_email.as_str()));

    let seen_by_b = store
        .for_tenant(b.clone())
        .list_entity_audit("billing.invoice", shared_id, 100)
        .await
        .unwrap();
    assert_eq!(seen_by_b.len(), 1, "{seen_by_b:?}");
    assert_eq!(seen_by_b[0].action, "billing.invoice.void");
    assert_eq!(seen_by_b[0].actor.as_deref(), Some(b_email.as_str()));

    // A third tenant, which did nothing, sees the same nothing for that id as
    // it would for an id nobody ever used.
    let (c, _, _) = a_tenant(&store, "c").await;
    let outsider = store.for_tenant(c);
    assert!(
        outsider
            .list_entity_audit("billing.invoice", shared_id, 100)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        outsider
            .list_entity_audit("billing.invoice", "never-existed", 100)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn entries_accumulate_newest_first_and_are_never_rewritten() {
    let store = test_store().await;
    let (tenant, user, _) = a_tenant(&store, "order").await;
    let actions = [
        "crm.deal.create",
        "crm.deal.update",
        "crm.deal.stage",
        "crm.deal.activity.create",
    ];
    for action in actions {
        store
            .record_entity_audit(
                &tenant,
                Some(&user),
                action,
                "crm.deal",
                Some("deal-1"),
                Some("/crm/deals/deal-1"),
            )
            .await
            .unwrap();
    }
    let ts = store.for_tenant(tenant.clone());
    let history: Vec<String> = ts
        .list_entity_audit("crm.deal", "deal-1", 100)
        .await
        .unwrap()
        .into_iter()
        .map(|entry| entry.action)
        .collect();
    let mut newest_first: Vec<&str> = actions.to_vec();
    newest_first.reverse();
    assert_eq!(history, newest_first);

    // A later act appends; the earlier four are untouched, in the same order.
    store
        .record_entity_audit(
            &tenant,
            Some(&user),
            "crm.deal.delete",
            "crm.deal",
            Some("deal-1"),
            Some("/crm/deals/deal-1"),
        )
        .await
        .unwrap();
    let after: Vec<String> = ts
        .list_entity_audit("crm.deal", "deal-1", 100)
        .await
        .unwrap()
        .into_iter()
        .map(|entry| entry.action)
        .collect();
    assert_eq!(after[0], "crm.deal.delete");
    assert_eq!(after[1..], history[..]);

    // `limit` is a page, and is clamped rather than trusted.
    assert_eq!(
        ts.list_entity_audit("crm.deal", "deal-1", 2)
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        ts.list_entity_audit("crm.deal", "deal-1", -5)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn an_administrative_entry_belongs_to_no_record() {
    let store = test_store().await;
    let (tenant, user, _) = a_tenant(&store, "admin").await;
    store
        .record_audit(
            &tenant,
            Some(&user),
            None,
            "dkim.rotate",
            Some("example.test"),
            None,
        )
        .await
        .unwrap();
    store
        .record_entity_audit(
            &tenant,
            Some(&user),
            "billing.customer.create",
            "billing.customer",
            Some("cust-1"),
            Some("/billing/customers"),
        )
        .await
        .unwrap();

    let ts = store.for_tenant(tenant);
    // The tenant-wide log is one log: both are in it, newest first.
    let all = ts.list_audit(100).await.unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].action, "billing.customer.create");
    assert_eq!(all[0].entity_type.as_deref(), Some("billing.customer"));
    assert_eq!(all[1].action, "dkim.rotate");
    assert_eq!(all[1].entity_type, None, "an admin act names no record");
    assert_eq!(all[1].target.as_deref(), Some("example.test"));

    // …and an administrative entry surfaces on no record's tab.
    assert!(
        ts.list_entity_audit("billing.customer", "example.test", 100)
            .await
            .unwrap()
            .is_empty()
    );
}
