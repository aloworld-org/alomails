//! Tenancy proof for alo Billing customers (Law 1: isolation is tested, not
//! assumed). Customers are tenant-wide — a co-tenant user manages the same
//! list — but an outsider tenant gets the clean `NotFound`/empty on **every**
//! path: read, list, update, archive, and the address-book link. Also covers
//! the CRUD arc the queue item requires (create, read, update, list, archive)
//! and that a tenant deletion purges the rows.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountStore, BillingCustomerId, Contact, ContactId, NewCustomer, Store, StoreError, TenantId,
};

/// Asserts a result is the clean not-found denial — never data, never an
/// internal (`Db`) error.
fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

/// A fully-specified customer, so the round-trip assertions cover every column.
fn acme() -> NewCustomer {
    NewCustomer {
        name: "Acme GmbH".to_owned(),
        address_line1: "Hauptstraße 1".to_owned(),
        address_line2: "Haus B".to_owned(),
        postal_code: "10115".to_owned(),
        city: "Berlin".to_owned(),
        country: "de".to_owned(),
        vat_id: Some("DE 811.907-980".to_owned()),
        email: Some("billing@acme.test".to_owned()),
        payment_terms_days: 14,
        currency: "eur".to_owned(),
        contact_id: None,
    }
}

/// A minimal address-book contact (the id is replaced on insert).
fn contact(name: &str) -> Contact {
    Contact {
        id: ContactId::generate(),
        display_name: name.to_owned(),
        first_name: None,
        last_name: None,
        emails: Vec::new(),
        phones: Vec::new(),
        organization: None,
        job_title: None,
        notes: None,
    }
}

/// A tenant with one user, returning the account door plus the tenant id.
async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("bill-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@billing.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user), tenant)
}

#[tokio::test]
async fn billing_customers_round_trip_and_never_cross_tenant() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "a").await;
    // A co-tenant user of the same tenant: billing is a tenant-wide list.
    let uc = store
        .for_tenant(t1.clone())
        .create_user("c@billing.test")
        .await
        .unwrap();
    let c = store.for_account(t1.clone(), uc);
    let (b, t2) = tenant_with_user(&store, "b").await;

    // ---- create: every field normalised on the way in --------------------
    let id = a.create_billing_customer(&acme()).await.unwrap();
    let got = a.billing_customer(&id).await.unwrap().unwrap();
    assert_eq!(got.name, "Acme GmbH");
    assert_eq!(got.address_line1, "Hauptstraße 1");
    assert_eq!(got.address_line2, "Haus B");
    assert_eq!(got.postal_code, "10115");
    assert_eq!(got.city, "Berlin");
    assert_eq!(got.country, "DE", "country uppercased");
    assert_eq!(got.currency, "EUR", "currency uppercased");
    assert_eq!(
        got.vat_id.as_deref(),
        Some("DE811907980"),
        "VAT id stored canonical: prefixed, uppercase, no separators"
    );
    assert_eq!(got.email.as_deref(), Some("billing@acme.test"));
    assert_eq!(got.payment_terms_days, 14);
    assert_eq!(got.created_by, a.user().as_str());
    assert!(!got.is_archived());

    // ---- list: tenant-wide, active only by default -----------------------
    assert_eq!(a.billing_customers(false).await.unwrap().len(), 1);
    assert_eq!(
        c.billing_customers(false).await.unwrap().len(),
        1,
        "a co-tenant user sees the same customer list"
    );
    assert!(
        b.billing_customers(true).await.unwrap().is_empty(),
        "another tenant sees nothing, archived included"
    );

    // ---- read/update/archive from another tenant: clean denial -----------
    assert!(b.billing_customer(&id).await.unwrap().is_none());
    assert_not_found(
        b.update_billing_customer(
            &id,
            &NewCustomer {
                name: "Hijacked".to_owned(),
                ..acme()
            },
        )
        .await,
    );
    assert_not_found(b.set_billing_customer_archived(&id, true).await);
    // ... and nothing they tried changed A's row.
    let after = a.billing_customer(&id).await.unwrap().unwrap();
    assert_eq!(after.name, "Acme GmbH");
    assert!(!after.is_archived());

    // An id that never existed is the same answer as another tenant's id —
    // no existence oracle.
    let ghost = BillingCustomerId::generate();
    assert!(a.billing_customer(&ghost).await.unwrap().is_none());
    assert_not_found(a.update_billing_customer(&ghost, &acme()).await);
    assert_not_found(a.set_billing_customer_archived(&ghost, true).await);

    // ---- update: full replace, by a co-tenant user -----------------------
    c.update_billing_customer(
        &id,
        &NewCustomer {
            name: "Acme Europe GmbH".to_owned(),
            city: "Hamburg".to_owned(),
            vat_id: None,
            email: None,
            payment_terms_days: 30,
            ..acme()
        },
    )
    .await
    .unwrap();
    let edited = a.billing_customer(&id).await.unwrap().unwrap();
    assert_eq!(edited.name, "Acme Europe GmbH");
    assert_eq!(edited.city, "Hamburg");
    assert_eq!(edited.vat_id, None, "a B2C customer may drop its VAT id");
    assert_eq!(edited.email, None);
    assert_eq!(edited.payment_terms_days, 30);
    assert!(edited.updated_at >= edited.created_at);

    // ---- validation guards the write paths -------------------------------
    let invalid = [
        NewCustomer {
            name: "  ".to_owned(),
            ..acme()
        },
        NewCustomer {
            country: "DEU".to_owned(),
            ..acme()
        },
        NewCustomer {
            currency: "EU".to_owned(),
            ..acme()
        },
        NewCustomer {
            email: Some("not-an-address".to_owned()),
            ..acme()
        },
        NewCustomer {
            vat_id: Some("DE!!907980".to_owned()),
            ..acme()
        },
        // Right shape for Germany, wrong check digit — a typo, refused on
        // both the create and the update path.
        NewCustomer {
            vat_id: Some("DE811907981".to_owned()),
            ..acme()
        },
        // Nine digits are the German shape, but not with a Dutch prefix.
        NewCustomer {
            vat_id: Some("NL811907980".to_owned()),
            ..acme()
        },
        NewCustomer {
            payment_terms_days: -1,
            ..acme()
        },
        NewCustomer {
            payment_terms_days: 400,
            ..acme()
        },
    ];
    for bad in &invalid {
        match a.create_billing_customer(bad).await {
            Err(StoreError::Validation(_)) => {}
            other => panic!("expected Validation for {bad:?}, got {other:?}"),
        }
        match a.update_billing_customer(&id, bad).await {
            Err(StoreError::Validation(_)) => {}
            other => panic!("expected Validation for {bad:?}, got {other:?}"),
        }
    }
    // A rejected write left the record and the list untouched.
    assert_eq!(
        a.billing_customer(&id).await.unwrap().unwrap().name,
        "Acme Europe GmbH"
    );
    assert_eq!(a.billing_customers(true).await.unwrap().len(), 1);

    // ---- archive: hidden from the default list, never deleted ------------
    a.set_billing_customer_archived(&id, true).await.unwrap();
    assert!(a.billing_customers(false).await.unwrap().is_empty());
    let archived = a.billing_customer(&id).await.unwrap().unwrap();
    assert!(
        archived.is_archived(),
        "still readable, so invoices can name it"
    );
    let archived_at = archived.archived_at;
    // Idempotent: re-archiving keeps the original time.
    a.set_billing_customer_archived(&id, true).await.unwrap();
    assert_eq!(
        a.billing_customer(&id).await.unwrap().unwrap().archived_at,
        archived_at
    );
    // Archived rows sort after active ones in the include-archived list.
    let active = a
        .create_billing_customer(&NewCustomer {
            name: "Zzz Later GmbH".to_owned(),
            ..acme()
        })
        .await
        .unwrap();
    let listed = a.billing_customers(true).await.unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].id, active, "active before archived");
    assert_eq!(listed[1].id, id);
    // Restore.
    a.set_billing_customer_archived(&id, false).await.unwrap();
    assert!(
        !a.billing_customer(&id)
            .await
            .unwrap()
            .unwrap()
            .is_archived()
    );

    // ---- deleting the tenant purges its customers ------------------------
    // Read the rows directly: the claim is that they were cascaded away, not
    // merely hidden behind the tenant predicate of the list call.
    store.delete_tenant(&t1).await.unwrap();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap();
    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*) FROM billing_customers WHERE tenant_id = $1")
            .bind(t1.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(remaining, 0, "the tenant's customers are purged with it");
    store.delete_tenant(&t2).await.unwrap();
}

#[tokio::test]
async fn a_customer_can_only_link_a_contact_of_its_own_tenant() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "link-a").await;
    let (b, t2) = tenant_with_user(&store, "link-b").await;

    // B owns a contact; A must not be able to link it.
    let theirs = b.create_contact(&contact("Bob Other")).await.unwrap();
    let mine = a.create_contact(&contact("Alice Ours")).await.unwrap();

    let linked = NewCustomer {
        contact_id: Some(mine.clone()),
        ..acme()
    };
    let id = a.create_billing_customer(&linked).await.unwrap();
    assert_eq!(
        a.billing_customer(&id).await.unwrap().unwrap().contact_id,
        Some(mine)
    );

    // Another tenant's contact id, and an id that never existed, both get the
    // same clean denial — never a cross-tenant link.
    for foreign in [theirs, ContactId::generate()] {
        let attempt = NewCustomer {
            contact_id: Some(foreign),
            ..acme()
        };
        assert_not_found(a.create_billing_customer(&attempt).await);
        assert_not_found(a.update_billing_customer(&id, &attempt).await);
    }
    // The link that was there is intact.
    assert!(
        a.billing_customer(&id)
            .await
            .unwrap()
            .unwrap()
            .contact_id
            .is_some()
    );

    store.delete_tenant(&t1).await.unwrap();
    store.delete_tenant(&t2).await.unwrap();
}
