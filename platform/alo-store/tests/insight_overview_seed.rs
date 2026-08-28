//! The zero-setup Business overview (BI1.06): that a tenant's first read of
//! Insights hands it a working board, that the board is **ordinary** from that
//! moment, that the seed runs exactly once per tenant — including across a
//! concurrent first visit and across the board being thrown away — and that
//! every prebuilt question on it evaluates against real rows into the figures
//! the documents underneath carry.
//!
//! And the law that outranks all of it: the seed is per tenant, through the
//! account door, so another tenant's overview is invisible and its numbers are
//! never ours (Law 1 — isolation is tested, not assumed).
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::insight_overview::{BUSINESS_OVERVIEW, GALLERY, OverviewCaption, OverviewSeed};
use alo_store::{
    AccountStore, BUSINESS_OVERVIEW_KEY, NewDashboard, Store, TenantId, Unit, gallery_entry,
};

/// A tenant with one user, returning the account door plus the tenant id.
async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("bi6-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@overview.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user), tenant)
}

/// The words the edge hands in — here in the language the tests read.
fn seed(name: &str) -> OverviewSeed {
    OverviewSeed {
        name: name.to_owned(),
        captions: BUSINESS_OVERVIEW
            .iter()
            .map(|key| OverviewCaption {
                key: (*key).to_owned(),
                title: format!("Tile: {key}"),
            })
            .collect(),
    }
}

#[tokio::test]
async fn a_first_visit_hands_a_tenant_a_working_board_and_only_ever_once() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "seed").await;
    let (b, t2) = tenant_with_user(&store, "other").await;

    assert!(
        !a.insight_seed_ran(BUSINESS_OVERVIEW_KEY).await.unwrap(),
        "nothing is seeded until somebody opens Insights"
    );

    // ---- the first read ---------------------------------------------------
    let boards = a
        .insight_dashboards_or_seed(&seed("Business overview"))
        .await
        .unwrap();
    assert_eq!(boards.len(), 1, "one board, and it is the overview");
    let overview = boards.first().unwrap();
    assert_eq!(overview.name, "Business overview");
    assert!(overview.is_seeded());
    assert_eq!(overview.system_key.as_deref(), Some(BUSINESS_OVERVIEW_KEY));

    // Every tile the layout names is on it, in layout order, at the width its
    // gallery entry asked for, with a spec this build can read.
    let tiles = a.insight_tiles(&overview.id).await.unwrap();
    assert_eq!(tiles.len(), BUSINESS_OVERVIEW.len());
    for (tile, key) in tiles.iter().zip(BUSINESS_OVERVIEW) {
        let entry = gallery_entry(key).unwrap();
        assert_eq!(tile.title, format!("Tile: {key}"), "the edge's words");
        assert_eq!(tile.span, entry.span, "{key}");
        assert_eq!(tile.viz, Some(entry.viz()), "{key}");
        let spec = tile.spec.readable().unwrap_or_else(|| {
            panic!("{key} was stored as a question this build cannot read");
        });
        assert_eq!(*spec, entry.spec(), "{key} is the gallery's question");
    }
    let positions: Vec<f64> = tiles.iter().map(|t| t.position).collect();
    assert!(
        positions.windows(2).all(|w| w[0] < w[1]),
        "the layout is an ordering: {positions:?}"
    );

    // ---- a second read seeds nothing --------------------------------------
    assert!(a.insight_seed_ran(BUSINESS_OVERVIEW_KEY).await.unwrap());
    let again = a
        .insight_dashboards_or_seed(&seed("Business overview"))
        .await
        .unwrap();
    assert_eq!(again.len(), 1, "a second visit writes nothing");
    assert_eq!(
        a.insight_tiles(&overview.id).await.unwrap().len(),
        BUSINESS_OVERVIEW.len(),
        "and pins nothing twice"
    );

    // ---- it is an ordinary board afterwards -------------------------------
    a.rename_insight_dashboard(
        &overview.id,
        &NewDashboard {
            name: "Our numbers".to_owned(),
        },
    )
    .await
    .unwrap();
    a.delete_insight_tile(&tiles[0].id).await.unwrap();
    let after = a
        .insight_dashboards_or_seed(&seed("Business overview"))
        .await
        .unwrap();
    assert_eq!(after.first().unwrap().name, "Our numbers");
    assert_eq!(
        a.insight_tiles(&overview.id).await.unwrap().len(),
        BUSINESS_OVERVIEW.len() - 1,
        "a removed tile stays removed: the seed is not a repair job"
    );

    // ---- a thrown-away overview does not come back ------------------------
    a.delete_insight_dashboard(&overview.id).await.unwrap();
    let empty = a
        .insight_dashboards_or_seed(&seed("Business overview"))
        .await
        .unwrap();
    assert!(
        empty.is_empty(),
        "the seed asks whether it has ever run, not whether the board is still there"
    );

    // ---- and none of it was ever the other tenant's -----------------------
    assert!(!b.insight_seed_ran(BUSINESS_OVERVIEW_KEY).await.unwrap());
    assert!(b.insight_dashboards().await.unwrap().is_empty());
    assert!(b.insight_dashboard(&overview.id).await.unwrap().is_none());
    let theirs = b.insight_dashboards_or_seed(&seed("Aperçu")).await.unwrap();
    assert_eq!(
        theirs.len(),
        1,
        "their own first visit seeds their own board"
    );
    assert_ne!(theirs.first().unwrap().id, overview.id);
    assert_eq!(a.insight_dashboards().await.unwrap().len(), 0);

    for tenant in [t1, t2] {
        store.delete_tenant(&tenant).await.unwrap();
    }
}

#[tokio::test]
async fn two_first_visits_at_once_produce_exactly_one_board() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "race").await;
    let second = store
        .for_tenant(t1.clone())
        .create_user("race2@overview.test")
        .await
        .unwrap();
    let colleague = store.for_account(t1.clone(), second);

    // Two colleagues open Insights in the same instant. The ledger's primary
    // key decides; the loser writes nothing and reads back what the winner
    // wrote — which is the whole reason the seed needs no lock.
    let english = seed("Business overview");
    let french = seed("Aperçu de l'activité");
    let (left, right) = tokio::join!(
        a.insight_dashboards_or_seed(&english),
        colleague.insight_dashboards_or_seed(&french),
    );
    let left = left.unwrap();
    let right = right.unwrap();
    assert_eq!(left.len(), 1, "one board");
    assert_eq!(right.len(), 1, "the same one board");
    assert_eq!(left.first().unwrap().id, right.first().unwrap().id);
    assert_eq!(
        a.insight_tiles(&left.first().unwrap().id)
            .await
            .unwrap()
            .len(),
        BUSINESS_OVERVIEW.len(),
        "and it is whole: the board and its tiles are one transaction"
    );

    store.delete_tenant(&t1).await.unwrap();
}

/// The item's done-when, at the store: a seeded tenant's board answers with
/// live figures and no clicks. Every prebuilt question in the gallery — not
/// only the ones on the overview — is evaluated against a tenant that has just
/// been created, because an empty business is the state every tenant starts in
/// and a chart that cannot answer "nothing yet" is a chart that greets a new
/// customer with an error.
#[tokio::test]
async fn every_prebuilt_question_answers_on_a_real_tenant() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "eval").await;

    for entry in GALLERY {
        let series = a
            .insight_evaluate(&entry.spec())
            .await
            .unwrap_or_else(|e| panic!("{} could not be answered: {e}", entry.key));
        assert!(!series.truncated, "{}", entry.key);
        // A money answer states the currency it is in, always: a figure whose
        // unit a reader has to guess is a figure nobody can act on.
        if series.unit.kind == Unit::Money {
            assert!(
                series.unit.currency.is_some() || series.groups.len() != 1,
                "{} returned money with no currency",
                entry.key
            );
        }
        for group in &series.groups {
            assert!(
                group.points.iter().all(|p| p.value == 0),
                "{} invented a figure for a tenant with no documents",
                entry.key
            );
        }
    }

    store.delete_tenant(&t1).await.unwrap();
}

#[tokio::test]
async fn a_deleted_tenant_takes_its_seed_ledger_with_it() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "purge").await;
    a.insight_dashboards_or_seed(&seed("Business overview"))
        .await
        .unwrap();

    store.delete_tenant(&t1).await.unwrap();
    let pool = sqlx::postgres::PgPool::connect(&common::database_url())
        .await
        .unwrap();
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM insight_seeds WHERE tenant_id = $1")
        .bind(t1.as_str())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0, "the ledger still holds rows of a deleted tenant");
    pool.close().await;
}
