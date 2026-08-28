//! Tenancy proof for alo Insights dashboards and tiles (Law 1: isolation is
//! tested, not assumed). Boards are tenant-wide — a co-tenant reads the same
//! board — but an outsider tenant gets the clean `NotFound`/empty on **every**
//! path: read, list, read-by-system-key, rename, delete, pin, edit, move and
//! unpin. Also covers the CRUD arc the queue item requires, the write gate
//! (a spec the typed model rejects never reaches the column), the read
//! tolerance (a spec from the future comes back marked unreadable instead of
//! breaking the board), the seed marker and its race, the caps, the cascade,
//! and that a tenant deletion purges both tables.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::insight_dashboards::{DASHBOARD_NAME_MAX_CHARS, DASHBOARDS_PER_TENANT_MAX};
use alo_store::insight_tiles::{TILE_SPAN_MAX, TILES_PER_DASHBOARD_MAX};
use alo_store::{
    AccountStore, BUSINESS_OVERVIEW_KEY, InsightDashboardId, InsightTileId, NewDashboard, NewTile,
    Store, StoreError, TenantId, TileSpec, Viz,
};
use serde_json::{Value, json};

/// Asserts a result is the clean not-found denial — never data, never an
/// internal (`Db`) error.
fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

/// Asserts a result is a validation refusal naming the rule.
fn assert_invalid<T: std::fmt::Debug>(result: Result<T, StoreError>, rule: &str) {
    match result {
        Err(StoreError::Validation(msg)) => {
            assert!(msg.contains(rule), "expected {rule:?} in {msg:?}");
        }
        other => panic!("expected Validation({rule:?}), got: {other:?}"),
    }
}

/// A tenant with one user, returning the account door plus the tenant id.
async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("bi-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@insights.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user), tenant)
}

/// Revenue by month for the last year — a bar over a time breakdown.
fn revenue_spec() -> Value {
    json!({
        "schema_version": 1,
        "dataset": "billing.documents",
        "measure": { "id": "net", "agg": "sum" },
        "dimension": { "id": "issue_date", "grain": "month" },
        "period": { "kind": "last_n", "n": 12, "grain": "month" },
        "filters": [ { "id": "status", "op": "in", "values": ["issued", "paid"] } ],
        "viz": "bar"
    })
}

/// Everything still owed, as one figure.
fn outstanding_spec() -> Value {
    json!({
        "schema_version": 1,
        "dataset": "billing.receivables",
        "measure": { "id": "outstanding", "agg": "sum" },
        "period": { "kind": "all" },
        "viz": "number"
    })
}

fn tile(title: &str, spec: Value) -> NewTile {
    NewTile {
        title: title.to_owned(),
        spec,
        span: 1,
    }
}

#[tokio::test]
async fn insight_dashboards_round_trip_and_never_cross_tenant() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "da").await;
    // A co-tenant user: boards are tenant-wide in BI-1.
    let uc = store
        .for_tenant(t1.clone())
        .create_user("dc@insights.test")
        .await
        .unwrap();
    let c = store.for_account(t1.clone(), uc);
    let (b, t2) = tenant_with_user(&store, "db").await;

    // ---- create: normalised on the way in --------------------------------
    let id = a
        .create_insight_dashboard(&NewDashboard {
            name: "  Cash  ".to_owned(),
        })
        .await
        .unwrap();
    let got = a.insight_dashboard(&id).await.unwrap().unwrap();
    assert_eq!(got.name, "Cash");
    assert!(!got.is_seeded(), "a user-made board carries no system key");
    assert_eq!(got.system_key, None);

    // A co-tenant sees the same board.
    assert_eq!(
        c.insight_dashboard(&id).await.unwrap().unwrap().name,
        "Cash"
    );
    assert_eq!(c.insight_dashboards().await.unwrap().len(), 1);

    // ---- validation ------------------------------------------------------
    assert_invalid(
        a.create_insight_dashboard(&NewDashboard {
            name: "   ".to_owned(),
        })
        .await,
        "name",
    );
    assert_invalid(
        a.create_insight_dashboard(&NewDashboard {
            name: "x".repeat(DASHBOARD_NAME_MAX_CHARS + 1),
        })
        .await,
        "at most",
    );

    // ---- rename ----------------------------------------------------------
    a.rename_insight_dashboard(
        &id,
        &NewDashboard {
            name: "Cash & collections".to_owned(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        a.insight_dashboard(&id).await.unwrap().unwrap().name,
        "Cash & collections"
    );

    // ---- another tenant: the clean denial on every path -------------------
    assert!(b.insight_dashboards().await.unwrap().is_empty());
    assert!(b.insight_dashboard(&id).await.unwrap().is_none());
    assert_not_found(
        b.rename_insight_dashboard(
            &id,
            &NewDashboard {
                name: "Mine now".to_owned(),
            },
        )
        .await,
    );
    assert_not_found(b.delete_insight_dashboard(&id).await);
    assert_not_found(
        b.create_insight_tile(&id, &tile("Theirs", outstanding_spec()))
            .await,
    );
    assert!(b.insight_tiles(&id).await.unwrap().is_empty());
    // …and nothing they did touched it.
    assert_eq!(
        a.insight_dashboard(&id).await.unwrap().unwrap().name,
        "Cash & collections"
    );

    // An id that never existed is the same answer as another tenant's id —
    // no existence oracle across tenants.
    let ghost = InsightDashboardId::generate();
    assert!(a.insight_dashboard(&ghost).await.unwrap().is_none());
    assert_not_found(a.delete_insight_dashboard(&ghost).await);

    // ---- delete ----------------------------------------------------------
    a.delete_insight_dashboard(&id).await.unwrap();
    assert!(a.insight_dashboard(&id).await.unwrap().is_none());
    assert!(a.insight_dashboards().await.unwrap().is_empty());

    for tenant in [t1, t2] {
        store.delete_tenant(&tenant).await.unwrap();
    }
}

#[tokio::test]
async fn insight_tiles_round_trip_and_never_cross_tenant() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "ta").await;
    let (b, t2) = tenant_with_user(&store, "tb").await;

    let board = a
        .create_insight_dashboard(&NewDashboard {
            name: "Overview".to_owned(),
        })
        .await
        .unwrap();
    let theirs = b
        .create_insight_dashboard(&NewDashboard {
            name: "Theirs".to_owned(),
        })
        .await
        .unwrap();

    // ---- pin: appended at the end of the layout --------------------------
    let first = a
        .create_insight_tile(&board, &tile("Outstanding", outstanding_spec()))
        .await
        .unwrap();
    let second = a
        .create_insight_tile(
            &board,
            &NewTile {
                title: "  Revenue  ".to_owned(),
                spec: revenue_spec(),
                span: TILE_SPAN_MAX,
            },
        )
        .await
        .unwrap();

    let tiles = a.insight_tiles(&board).await.unwrap();
    assert_eq!(tiles.len(), 2);
    assert_eq!(tiles[0].id, first);
    assert_eq!(tiles[1].id, second);
    assert_eq!(tiles[1].title, "Revenue", "the title is trimmed on write");
    assert!(tiles[0].position < tiles[1].position);
    assert_eq!(tiles[1].span, TILE_SPAN_MAX);
    // The chart form is derived from the spec, never taken separately.
    assert_eq!(tiles[0].viz, Some(Viz::Number));
    assert_eq!(tiles[1].viz, Some(Viz::Bar));
    // And the stored spec round-trips through the typed model.
    let stored = tiles[1].spec.readable().expect("a spec we just wrote");
    assert_eq!(stored.dataset, alo_store::Dataset::BillingDocuments);
    assert_eq!(stored.measure.id, alo_store::Measure::Net);

    // ---- the write gate --------------------------------------------------
    let mut impossible = revenue_spec();
    impossible["measure"] = json!({ "id": "win_rate", "agg": "ratio" });
    assert_invalid(
        a.create_insight_tile(&board, &tile("Nonsense", impossible))
            .await,
        "does not offer",
    );
    assert_invalid(
        a.create_insight_tile(&board, &tile("Empty", json!({})))
            .await,
        "spec",
    );
    assert_invalid(
        a.create_insight_tile(&board, &tile("  ", outstanding_spec()))
            .await,
        "title",
    );
    assert_invalid(
        a.create_insight_tile(
            &board,
            &NewTile {
                span: TILE_SPAN_MAX + 1,
                ..tile("Too wide", outstanding_spec())
            },
        )
        .await,
        "span",
    );
    assert_eq!(a.insight_tiles(&board).await.unwrap().len(), 2, "no writes");

    // ---- edit and move ---------------------------------------------------
    a.update_insight_tile(
        &first,
        &NewTile {
            title: "Owed to us".to_owned(),
            spec: revenue_spec(),
            span: 2,
        },
    )
    .await
    .unwrap();
    let edited = a.insight_tile(&first).await.unwrap().unwrap();
    assert_eq!(edited.title, "Owed to us");
    assert_eq!(edited.span, 2);
    assert_eq!(
        edited.viz,
        Some(Viz::Bar),
        "the derived form follows the spec"
    );

    a.move_insight_tile(&first, 99.0).await.unwrap();
    let order: Vec<InsightTileId> = a
        .insight_tiles(&board)
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.id)
        .collect();
    assert_eq!(order, vec![second.clone(), first.clone()]);
    assert_invalid(a.move_insight_tile(&first, f64::NAN).await, "finite");

    // ---- another tenant: the clean denial on every path -------------------
    assert!(b.insight_tile(&first).await.unwrap().is_none());
    assert!(b.insight_tiles(&board).await.unwrap().is_empty());
    assert_not_found(
        b.update_insight_tile(&first, &tile("Mine now", outstanding_spec()))
            .await,
    );
    assert_not_found(b.move_insight_tile(&first, 1.0).await);
    assert_not_found(b.delete_insight_tile(&first).await);
    // Neither direction: our tenant cannot pin onto their board either.
    assert_not_found(
        a.create_insight_tile(&theirs, &tile("Uninvited", outstanding_spec()))
            .await,
    );
    assert!(b.insight_tiles(&theirs).await.unwrap().is_empty());
    assert_eq!(a.insight_tiles(&board).await.unwrap().len(), 2, "untouched");

    let ghost = InsightTileId::generate();
    assert!(a.insight_tile(&ghost).await.unwrap().is_none());
    assert_not_found(a.delete_insight_tile(&ghost).await);

    // ---- unpin, and the cascade -----------------------------------------
    a.delete_insight_tile(&first).await.unwrap();
    assert_eq!(a.insight_tiles(&board).await.unwrap().len(), 1);
    a.delete_insight_dashboard(&board).await.unwrap();
    assert!(
        a.insight_tile(&second).await.unwrap().is_none(),
        "a deleted board takes its tiles with it"
    );

    for tenant in [t1, t2] {
        store.delete_tenant(&tenant).await.unwrap();
    }
}

#[tokio::test]
async fn a_seeded_board_is_written_once_and_is_ordinary_afterwards() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "seed").await;

    assert!(
        a.insight_dashboard_by_key(BUSINESS_OVERVIEW_KEY)
            .await
            .unwrap()
            .is_none(),
        "nothing is seeded until somebody opens Insights"
    );

    let id = a
        .create_seeded_insight_dashboard(
            &NewDashboard {
                // Already translated by the route edge, like a pipeline seed.
                name: "Aperçu de l'activité".to_owned(),
            },
            BUSINESS_OVERVIEW_KEY,
        )
        .await
        .unwrap();
    let seeded = a
        .insight_dashboard_by_key(BUSINESS_OVERVIEW_KEY)
        .await
        .unwrap();
    let seeded = seeded.expect("the seeded board");
    assert_eq!(seeded.id, id);
    assert!(seeded.is_seeded());

    // The race: a concurrent first visit loses on the partial unique index and
    // reads back what the winner wrote — which is what makes the seed
    // idempotent without a lock.
    match a
        .create_seeded_insight_dashboard(
            &NewDashboard {
                name: "Business overview".to_owned(),
            },
            BUSINESS_OVERVIEW_KEY,
        )
        .await
    {
        Err(StoreError::Conflict(_)) => {}
        other => panic!("expected Conflict on a second seed, got: {other:?}"),
    }
    assert_eq!(a.insight_dashboards().await.unwrap().len(), 1);

    // From here it is an ordinary board: renamable, with tiles.
    a.rename_insight_dashboard(
        &id,
        &NewDashboard {
            name: "Mon aperçu".to_owned(),
        },
    )
    .await
    .unwrap();
    let renamed = a.insight_dashboard(&id).await.unwrap().unwrap();
    assert_eq!(renamed.name, "Mon aperçu");
    assert_eq!(
        renamed.system_key.as_deref(),
        Some(BUSINESS_OVERVIEW_KEY),
        "renaming never clears the marker, so the seed still cannot run twice"
    );
    a.create_insight_tile(&id, &tile("Outstanding", outstanding_spec()))
        .await
        .unwrap();

    // A user-made board alongside it carries no key, so it never competes for
    // the seed's slot.
    a.create_insight_dashboard(&NewDashboard {
        name: "Mine".to_owned(),
    })
    .await
    .unwrap();
    a.create_insight_dashboard(&NewDashboard {
        name: "Mine too".to_owned(),
    })
    .await
    .unwrap();
    assert_eq!(a.insight_dashboards().await.unwrap().len(), 3);

    // Our own key vocabulary is checked as strictly as anything a user types.
    assert_invalid(
        a.create_seeded_insight_dashboard(
            &NewDashboard {
                name: "Bad key".to_owned(),
            },
            "Business Overview",
        )
        .await,
        "system key",
    );
    assert_invalid(a.insight_dashboard_by_key("").await, "system key");

    // A thrown-away overview does not come back: the seed asks whether the
    // tenant has the key, and after a delete it has not.
    a.delete_insight_dashboard(&id).await.unwrap();
    assert!(
        a.insight_dashboard_by_key(BUSINESS_OVERVIEW_KEY)
            .await
            .unwrap()
            .is_none()
    );

    store.delete_tenant(&t1).await.unwrap();
}

#[tokio::test]
async fn a_tile_from_the_future_is_marked_unreadable_and_the_board_still_renders() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "future").await;
    let board = a
        .create_insight_dashboard(&NewDashboard {
            name: "Mixed".to_owned(),
        })
        .await
        .unwrap();
    let good = a
        .create_insight_tile(&board, &tile("Outstanding", outstanding_spec()))
        .await
        .unwrap();

    // Plant what a newer build might have written. The write gate refuses it,
    // which is exactly why the row is planted directly: this is the read
    // tolerance being proven, not a hole in the gate.
    assert_invalid(
        a.create_insight_tile(
            &board,
            &tile(
                "Later",
                json!({ "schema_version": 2, "dataset": "billing.documents" }),
            ),
        )
        .await,
        "schema_version",
    );
    let pool = sqlx::postgres::PgPool::connect(&common::database_url())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO insight_tiles \
         (tenant_id, id, dashboard_id, title, spec, viz, position, span) \
         VALUES ($1, $2, $3, 'Later', $4::jsonb, 'sankey', 2, 1)",
    )
    .bind(t1.as_str())
    .bind("future-tile")
    .bind(board.as_str())
    .bind(r#"{"schema_version":2,"dataset":"billing.documents"}"#)
    .execute(&pool)
    .await
    .unwrap();

    let tiles = a.insight_tiles(&board).await.unwrap();
    assert_eq!(tiles.len(), 2, "the board still lists every tile");
    assert_eq!(tiles[0].id, good);
    assert!(tiles[0].spec.readable().is_some());
    match &tiles[1].spec {
        TileSpec::Unreadable { raw, reason } => {
            assert_eq!(raw["schema_version"], json!(2), "handed back untouched");
            assert!(reason.contains("schema_version"), "{reason}");
        }
        TileSpec::Readable(spec) => panic!("expected unreadable, got {spec:?}"),
    }
    assert_eq!(
        tiles[1].viz, None,
        "an unknown chart form is not guessed at"
    );

    // Replacing the spec re-runs the gate and heals the tile.
    a.update_insight_tile(&tiles[1].id, &tile("Now", revenue_spec()))
        .await
        .unwrap();
    let healed = a.insight_tile(&tiles[1].id).await.unwrap().unwrap();
    assert!(healed.spec.readable().is_some());
    assert_eq!(healed.viz, Some(Viz::Bar));

    pool.close().await;
    store.delete_tenant(&t1).await.unwrap();
}

#[tokio::test]
async fn the_caps_hold_and_a_deleted_tenant_takes_its_boards_with_it() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "caps").await;

    let board = a
        .create_insight_dashboard(&NewDashboard {
            name: "Full".to_owned(),
        })
        .await
        .unwrap();
    for i in 0..TILES_PER_DASHBOARD_MAX {
        a.create_insight_tile(&board, &tile(&format!("Tile {i}"), outstanding_spec()))
            .await
            .unwrap();
    }
    assert_eq!(
        a.insight_tiles(&board).await.unwrap().len() as i64,
        TILES_PER_DASHBOARD_MAX,
        "the cap is inclusive"
    );
    assert_invalid(
        a.create_insight_tile(&board, &tile("One too many", outstanding_spec()))
            .await,
        "at most",
    );
    // Unpinning one makes room.
    let tiles = a.insight_tiles(&board).await.unwrap();
    a.delete_insight_tile(&tiles[0].id).await.unwrap();
    a.create_insight_tile(&board, &tile("Room now", outstanding_spec()))
        .await
        .unwrap();

    for i in 1..DASHBOARDS_PER_TENANT_MAX {
        a.create_insight_dashboard(&NewDashboard {
            name: format!("Board {i}"),
        })
        .await
        .unwrap();
    }
    assert_eq!(
        a.insight_dashboards().await.unwrap().len() as i64,
        DASHBOARDS_PER_TENANT_MAX
    );
    assert_invalid(
        a.create_insight_dashboard(&NewDashboard {
            name: "One too many".to_owned(),
        })
        .await,
        "at most",
    );

    // ---- purge: the tenant's rows leave with the tenant -------------------
    store.delete_tenant(&t1).await.unwrap();
    let pool = sqlx::postgres::PgPool::connect(&common::database_url())
        .await
        .unwrap();
    for table in ["insight_dashboards", "insight_tiles"] {
        let left: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {table} WHERE tenant_id = $1"
        ))
        .bind(t1.as_str())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(left, 0, "{table} still holds rows of a deleted tenant");
    }
    pool.close().await;
}
