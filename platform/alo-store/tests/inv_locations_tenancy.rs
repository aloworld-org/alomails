//! Tenancy proof and lifecycle for the places stock can be (alo Inventory,
//! B5.04a — Law 1: isolation is tested, not assumed).
//!
//! Locations are tenant-wide — a co-tenant counts the same warehouse — but an
//! outsider gets the clean `NotFound`/empty on **every** path: read, list,
//! rename, archive and delete. The second test carries the first-use seed: that
//! it runs once, that deleting what it wrote does not bring it back, and that
//! two simultaneous first reads produce one set of locations rather than two.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::inv_locations::{LocationKind, LocationSeed, NewLocation, VIRTUAL_KINDS};
use alo_store::{AccountStore, InvLocationId, Store, StoreError, TenantId};

/// Asserts a result is the clean not-found denial — never data, never an
/// internal (`Db`) error.
fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

/// Asserts a result is a refusal the caller could have predicted from the
/// state, and returns its sentence.
fn conflict<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Conflict(message)) => message,
        other => panic!("expected Conflict, got: {other:?}"),
    }
}

/// The seed as the HTTP edge will build it — names in the reader's language,
/// never a hardcoded English word in the store.
fn dutch_seed() -> LocationSeed {
    LocationSeed {
        stock: "Hoofdmagazijn".to_owned(),
        supplier: "Leveranciers".to_owned(),
        customer: "Klanten".to_owned(),
        adjustment: "Correcties".to_owned(),
        production: "Productie".to_owned(),
    }
}

/// A tenant with one user, returning the account door plus the tenant id.
async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("loc-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@locations.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user), tenant)
}

#[tokio::test]
async fn locations_round_trip_and_never_cross_tenant() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "a").await;
    let uc = store
        .for_tenant(t1.clone())
        .create_user("c@locations.test")
        .await
        .unwrap();
    let c = store.for_account(t1.clone(), uc);
    let (b, _t2) = tenant_with_user(&store, "b").await;

    // ---- create: a real place, normalised on the way in ------------------
    let id = a
        .create_inv_location(&NewLocation {
            code: " wh-2 ".to_owned(),
            name: " Tweede magazijn ".to_owned(),
            kind: LocationKind::Stock,
        })
        .await
        .unwrap();
    let got = a.inv_location(&id).await.unwrap().unwrap();
    assert_eq!(
        got.code, "WH-2",
        "a code is an identifier, so it uppercases"
    );
    assert_eq!(got.name, "Tweede magazijn");
    assert_eq!(got.kind, LocationKind::Stock);
    assert!(!got.is_archived());

    // ---- the co-tenant counts the same warehouse -------------------------
    assert_eq!(c.inv_location(&id).await.unwrap().unwrap().code, "WH-2");
    assert_eq!(c.inv_locations(false).await.unwrap().len(), 1);

    // ---- the outsider sees nothing, on every path ------------------------
    assert!(
        b.inv_location(&id).await.unwrap().is_none(),
        "another tenant's warehouse reads as absent, not as data"
    );
    assert!(b.inv_locations(true).await.unwrap().is_empty());
    let rename = NewLocation {
        code: "STOLEN".to_owned(),
        name: "Stolen".to_owned(),
        kind: LocationKind::Stock,
    };
    assert_not_found(b.update_inv_location(&id, &rename).await);
    assert_not_found(b.set_inv_location_archived(&id, true).await);
    assert_not_found(b.delete_inv_location(&id).await);
    let after = a.inv_location(&id).await.unwrap().unwrap();
    assert_eq!(after.code, "WH-2", "none of those attempts touched the row");
    assert!(!after.is_archived());

    // ---- validation refuses what the caller can fix ----------------------
    for bad in [
        NewLocation {
            code: "WH 3".to_owned(),
            name: "Derde".to_owned(),
            kind: LocationKind::Stock,
        },
        NewLocation {
            code: "WH3".to_owned(),
            name: "  ".to_owned(),
            kind: LocationKind::Stock,
        },
    ] {
        assert!(
            matches!(
                a.create_inv_location(&bad).await,
                Err(StoreError::Validation(_))
            ),
            "expected a Validation refusal"
        );
    }
    // The four virtual counterparties are the system's, not a caller's: one of
    // each exists per tenant, so a receipt can never choose between two.
    for kind in VIRTUAL_KINDS {
        assert!(
            matches!(
                a.create_inv_location(&NewLocation {
                    code: "X1".to_owned(),
                    name: "Mine".to_owned(),
                    kind,
                })
                .await,
                Err(StoreError::Validation(_))
            ),
            "{kind:?} must not be creatable"
        );
    }

    // ---- codes are unique within the tenant, and only within it ----------
    let taken = a
        .create_inv_location(&NewLocation {
            code: "wh-2".to_owned(),
            name: "Duplicate".to_owned(),
            kind: LocationKind::Stock,
        })
        .await;
    assert!(conflict(taken).contains("code"));
    // …and tenant B may use the very same code: a global index would leak the
    // existence of A's warehouse through a constraint violation.
    b.create_inv_location(&NewLocation {
        code: "WH-2".to_owned(),
        name: "B's own".to_owned(),
        kind: LocationKind::Stock,
    })
    .await
    .unwrap();

    // ---- rename: code and name, never the kind ---------------------------
    a.update_inv_location(
        &id,
        &NewLocation {
            code: "SHOP".to_owned(),
            name: "Winkel".to_owned(),
            kind: LocationKind::Stock,
        },
    )
    .await
    .unwrap();
    let renamed = a.inv_location(&id).await.unwrap().unwrap();
    assert_eq!(renamed.code, "SHOP");
    assert_eq!(renamed.name, "Winkel");
    let rekinded = a
        .update_inv_location(
            &id,
            &NewLocation {
                code: "SHOP".to_owned(),
                name: "Winkel".to_owned(),
                kind: LocationKind::Transit,
            },
        )
        .await;
    assert!(
        matches!(rekinded, Err(StoreError::Validation(_))),
        "re-kinding rewrites the meaning of every movement already recorded"
    );

    // ---- archive: out of the pickers, still nameable ---------------------
    a.set_inv_location_archived(&id, true).await.unwrap();
    assert!(
        !a.inv_locations(false)
            .await
            .unwrap()
            .iter()
            .any(|l| l.id == id),
        "archived locations leave the picker"
    );
    let listed = a.inv_locations(true).await.unwrap();
    let stamp = listed
        .iter()
        .find(|l| l.id == id)
        .and_then(|l| l.archived_at);
    assert!(stamp.is_some());
    a.set_inv_location_archived(&id, true).await.unwrap();
    assert_eq!(
        a.inv_locations(true)
            .await
            .unwrap()
            .iter()
            .find(|l| l.id == id)
            .and_then(|l| l.archived_at),
        stamp,
        "re-archiving must not restamp"
    );
    a.set_inv_location_archived(&id, false).await.unwrap();

    // ---- delete: the typo made a minute ago ------------------------------
    a.delete_inv_location(&id).await.unwrap();
    assert!(a.inv_location(&id).await.unwrap().is_none());

    // ---- an unknown id is NotFound, never a Db error ---------------------
    let ghost = InvLocationId::generate();
    assert!(a.inv_location(&ghost).await.unwrap().is_none());
    assert_not_found(a.update_inv_location(&ghost, &rename).await);
    assert_not_found(a.set_inv_location_archived(&ghost, true).await);
    assert_not_found(a.delete_inv_location(&ghost).await);

    // ---- deleting the tenant purges the rows -----------------------------
    store.delete_tenant(&t1).await.unwrap();
    assert!(a.inv_locations(true).await.unwrap().is_empty());
}

#[tokio::test]
async fn the_starting_set_is_seeded_once_and_never_handed_back() {
    let store = common::test_store().await;
    let (a, _t1) = tenant_with_user(&store, "seed").await;
    let (b, _t2) = tenant_with_user(&store, "seed-b").await;

    assert!(!a.inv_seed_ran("starting_locations").await.unwrap());
    assert!(
        a.inv_locations(true).await.unwrap().is_empty(),
        "nothing is written by the migration"
    );

    // ---- first use: one real place and the four counterparties -----------
    let first = a.inv_locations_or_seed(&dutch_seed(), false).await.unwrap();
    let mut kinds: Vec<LocationKind> = first.iter().map(|l| l.kind).collect();
    kinds.sort_by_key(|k| k.as_str());
    assert_eq!(
        kinds,
        vec![
            LocationKind::Adjustment,
            LocationKind::Customer,
            LocationKind::Production,
            LocationKind::Stock,
            LocationKind::Supplier,
        ],
        "transit is not seeded — a tenant with one warehouse does not need one"
    );
    assert!(
        first.iter().any(|l| l.name == "Hoofdmagazijn"),
        "the names are the caller's, in the reader's language"
    );
    assert!(
        first.iter().all(|l| !l.name.is_empty()),
        "a tenant is never handed a nameless place"
    );
    assert!(a.inv_seed_ran("starting_locations").await.unwrap());

    // The lookup every document movement makes: by kind, never by code, so a
    // tenant may rename everything without breaking a rule.
    for kind in VIRTUAL_KINDS {
        assert!(
            a.inv_location_of_kind(kind).await.unwrap().is_some(),
            "{kind:?} must resolve"
        );
    }
    assert!(
        a.inv_location_of_kind(LocationKind::Transit)
            .await
            .unwrap()
            .is_none()
    );

    // ---- a second read seeds nothing -------------------------------------
    let again = a.inv_locations_or_seed(&dutch_seed(), false).await.unwrap();
    assert_eq!(again.len(), first.len());

    // ---- and deleting what we gave them does not bring it back -----------
    let spare = first
        .iter()
        .find(|l| l.kind == LocationKind::Stock)
        .unwrap();
    a.delete_inv_location(&spare.id).await.unwrap();
    let after = a.inv_locations_or_seed(&dutch_seed(), true).await.unwrap();
    assert_eq!(
        after.len(),
        first.len() - 1,
        "the ledger's question is whether the seed RAN, not whether the rows survive"
    );

    // ---- a system location is neither archivable nor deletable -----------
    let supplier = a
        .inv_location_of_kind(LocationKind::Supplier)
        .await
        .unwrap()
        .unwrap();
    assert!(conflict(a.set_inv_location_archived(&supplier.id, true).await).contains("system"));
    assert!(conflict(a.delete_inv_location(&supplier.id).await).contains("system"));
    // …but it IS renameable: our word for it was a starting point, not a claim.
    a.update_inv_location(
        &supplier.id,
        &NewLocation {
            code: "LEV".to_owned(),
            name: "Onze leveranciers".to_owned(),
            kind: LocationKind::Supplier,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        a.inv_location_of_kind(LocationKind::Supplier)
            .await
            .unwrap()
            .unwrap()
            .name,
        "Onze leveranciers"
    );

    // ---- the seed is one tenant's, never another's -----------------------
    assert!(!b.inv_seed_ran("starting_locations").await.unwrap());
    assert!(b.inv_locations(true).await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_simultaneous_first_reads_produce_one_set_of_locations() {
    let store = common::test_store().await;
    let (a, _t1) = tenant_with_user(&store, "race").await;

    let mut handles = Vec::new();
    for _ in 0..6 {
        let door = a.clone();
        handles.push(tokio::spawn(async move {
            door.inv_locations_or_seed(&dutch_seed(), true).await
        }));
    }
    for handle in handles {
        let seeded = handle.await.unwrap().unwrap();
        assert_eq!(seeded.len(), 5, "every caller reads the one set that won");
    }
    assert_eq!(a.inv_locations(true).await.unwrap().len(), 5);
}
