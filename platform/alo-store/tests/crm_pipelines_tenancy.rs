//! Tenancy proof for alo CRM's boards and their columns (Law 1: isolation is
//! tested, not assumed). Pipelines and stages are tenant-wide — a co-tenant
//! user works the same board — but an outsider tenant gets the clean
//! `NotFound`/empty on **every** path: read, list, create, update, move,
//! archive and delete. Also covers the CRUD arc the queue item requires, the
//! board rules that make a win rate meaningful (one won column, one lost
//! column, never both on one), the first-use seed and its race, and that a
//! tenant deletion purges both tables.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::crm_pipelines::{PIPELINE_DESCRIPTION_MAX_CHARS, PIPELINE_NAME_MAX_CHARS};
use alo_store::crm_stages::{STAGE_NAME_MAX_CHARS, STAGES_PER_PIPELINE_MAX};
use alo_store::{
    AccountStore, CrmPipelineId, CrmStageId, NewPipeline, NewStage, PipelineSeed, Stage, StageSeed,
    Store, StoreError, TenantId,
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

/// Asserts a result is a validation refusal naming the rule.
fn assert_invalid<T: std::fmt::Debug>(result: Result<T, StoreError>, rule: &str) {
    match result {
        Err(StoreError::Validation(msg)) => {
            assert!(msg.contains(rule), "expected {rule:?} in {msg:?}");
        }
        other => panic!("expected Validation({rule:?}), got: {other:?}"),
    }
}

/// Asserts a result is a conflict — a well-formed request the current state
/// disagrees with.
fn assert_conflict<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::Conflict(_)) => {}
        other => panic!("expected Conflict, got: {other:?}"),
    }
}

/// A tenant with one user, returning the account door plus the tenant id.
async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("crm-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@crm.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user), tenant)
}

/// The board a tenant is handed on first use — five columns, in the language
/// the caller (the route edge, in real life) hands in.
fn sales_seed() -> PipelineSeed {
    PipelineSeed {
        name: "Vente".to_owned(),
        stages: vec![
            stage_seed("Nouveau", false, false),
            stage_seed("Qualifié", false, false),
            stage_seed("Proposition", false, false),
            stage_seed("Gagné", true, false),
            stage_seed("Perdu", false, true),
        ],
    }
}

fn stage_seed(name: &str, is_won: bool, is_lost: bool) -> StageSeed {
    StageSeed {
        name: name.to_owned(),
        is_won,
        is_lost,
    }
}

fn column(name: &str) -> NewStage {
    NewStage {
        name: name.to_owned(),
        is_won: false,
        is_lost: false,
    }
}

fn names(stages: &[Stage]) -> Vec<&str> {
    stages.iter().map(|s| s.name.as_str()).collect()
}

#[tokio::test]
async fn crm_pipelines_round_trip_and_never_cross_tenant() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "pa").await;
    // A co-tenant user of the same tenant: boards are tenant-wide.
    let uc = store
        .for_tenant(t1.clone())
        .create_user("pc@crm.test")
        .await
        .unwrap();
    let c = store.for_account(t1.clone(), uc);
    let (b, t2) = tenant_with_user(&store, "pb").await;

    // ---- create: normalised on the way in --------------------------------
    let id = a
        .create_crm_pipeline(&NewPipeline {
            name: "  Renewals  ".to_owned(),
            description: "  Contracts up this year  ".to_owned(),
        })
        .await
        .unwrap();
    let got = a.crm_pipeline(&id).await.unwrap().unwrap();
    assert_eq!(got.name, "Renewals");
    assert_eq!(got.description, "Contracts up this year");
    assert_eq!(got.created_by, a.user().as_str());
    assert!(!got.is_archived());
    // A board a user builds by hand starts with no columns.
    assert!(a.crm_stages(&id, true).await.unwrap().is_empty());

    // ---- list: tenant-wide, active only by default -----------------------
    assert_eq!(a.crm_pipelines(false).await.unwrap().len(), 1);
    assert_eq!(
        c.crm_pipelines(false).await.unwrap().len(),
        1,
        "a co-tenant user works the same board"
    );
    assert!(
        b.crm_pipelines(true).await.unwrap().is_empty(),
        "another tenant sees nothing, archived included"
    );

    // ---- read/update/archive from another tenant: clean denial -----------
    assert!(b.crm_pipeline(&id).await.unwrap().is_none());
    assert_not_found(
        b.update_crm_pipeline(
            &id,
            &NewPipeline {
                name: "Hijacked".to_owned(),
                description: String::new(),
            },
        )
        .await,
    );
    assert_not_found(b.set_crm_pipeline_archived(&id, true).await);
    assert_not_found(b.create_crm_stage(&id, &column("Theirs")).await);
    assert_not_found(b.crm_stages(&id, false).await);
    // ... and nothing they tried changed A's board.
    let after = a.crm_pipeline(&id).await.unwrap().unwrap();
    assert_eq!(after.name, "Renewals");
    assert!(!after.is_archived());
    assert!(a.crm_stages(&id, true).await.unwrap().is_empty());

    // An id that never existed answers exactly as another tenant's does — no
    // existence oracle.
    let ghost = CrmPipelineId::generate();
    assert!(a.crm_pipeline(&ghost).await.unwrap().is_none());
    assert_not_found(
        a.update_crm_pipeline(
            &ghost,
            &NewPipeline {
                name: "Ghost".to_owned(),
                description: String::new(),
            },
        )
        .await,
    );
    assert_not_found(a.set_crm_pipeline_archived(&ghost, true).await);
    assert_not_found(a.crm_stages(&ghost, false).await);

    // ---- one active board owns one name, per tenant ----------------------
    assert_conflict(
        c.create_crm_pipeline(&NewPipeline {
            name: "renewals".to_owned(),
            description: String::new(),
        })
        .await,
    );
    // The rule is per tenant: B's board may carry the same name.
    let b_renewals = b
        .create_crm_pipeline(&NewPipeline {
            name: "Renewals".to_owned(),
            description: String::new(),
        })
        .await
        .unwrap();
    assert_eq!(b.crm_pipelines(false).await.unwrap().len(), 1);

    // ---- update: full replace, by a co-tenant user -----------------------
    let _new_business = a
        .create_crm_pipeline(&NewPipeline {
            name: "New Business".to_owned(),
            description: String::new(),
        })
        .await
        .unwrap();
    c.update_crm_pipeline(
        &id,
        &NewPipeline {
            name: "Renewals 2027".to_owned(),
            description: String::new(),
        },
    )
    .await
    .unwrap();
    let edited = a.crm_pipeline(&id).await.unwrap().unwrap();
    assert_eq!(edited.name, "Renewals 2027");
    assert_eq!(edited.description, "", "an empty description is legitimate");
    assert!(edited.updated_at >= edited.created_at);
    // Renaming onto another active board's name is refused.
    assert_conflict(
        a.update_crm_pipeline(
            &id,
            &NewPipeline {
                name: "New Business".to_owned(),
                description: String::new(),
            },
        )
        .await,
    );

    // ---- validation guards the write paths -------------------------------
    let invalid = [
        NewPipeline {
            name: "  ".to_owned(),
            description: String::new(),
        },
        NewPipeline {
            name: "x".repeat(PIPELINE_NAME_MAX_CHARS + 1),
            description: String::new(),
        },
        NewPipeline {
            name: "Fine".to_owned(),
            description: "x".repeat(PIPELINE_DESCRIPTION_MAX_CHARS + 1),
        },
    ];
    for bad in &invalid {
        assert!(matches!(
            a.create_crm_pipeline(bad).await,
            Err(StoreError::Validation(_))
        ));
        assert!(matches!(
            a.update_crm_pipeline(&id, bad).await,
            Err(StoreError::Validation(_))
        ));
    }
    // A rejected write left the record and the list untouched.
    assert_eq!(
        a.crm_pipeline(&id).await.unwrap().unwrap().name,
        "Renewals 2027"
    );
    assert_eq!(a.crm_pipelines(true).await.unwrap().len(), 2);

    // ---- archive: hidden from the default list, never deleted ------------
    a.set_crm_pipeline_archived(&id, true).await.unwrap();
    let active: Vec<String> = a
        .crm_pipelines(false)
        .await
        .unwrap()
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(active, vec!["New Business".to_owned()]);
    let archived = a.crm_pipeline(&id).await.unwrap().unwrap();
    assert!(
        archived.is_archived(),
        "still readable, so a closed deal can name where it closed"
    );
    let archived_at = archived.archived_at;
    a.set_crm_pipeline_archived(&id, true).await.unwrap();
    assert_eq!(
        a.crm_pipeline(&id).await.unwrap().unwrap().archived_at,
        archived_at,
        "re-archiving keeps the original time"
    );
    // An archived board frees its name for a new one...
    let fresh = a
        .create_crm_pipeline(&NewPipeline {
            name: "Renewals 2027".to_owned(),
            description: String::new(),
        })
        .await
        .unwrap();
    // ... and restoring the old one is then refused rather than silently
    // producing two boards with one name.
    assert_conflict(a.set_crm_pipeline_archived(&id, false).await);
    assert!(
        a.crm_pipeline(&id).await.unwrap().unwrap().is_archived(),
        "the refused restore left it archived"
    );
    a.update_crm_pipeline(
        &fresh,
        &NewPipeline {
            name: "Renewals (new)".to_owned(),
            description: String::new(),
        },
    )
    .await
    .unwrap();
    a.set_crm_pipeline_archived(&id, false).await.unwrap();
    assert!(!a.crm_pipeline(&id).await.unwrap().unwrap().is_archived());

    // ---- deleting the tenant purges its boards ---------------------------
    // Read the rows directly: the claim is that they were cascaded away, not
    // merely hidden behind the tenant predicate of the list call.
    store.delete_tenant(&t1).await.unwrap();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap();
    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*) FROM crm_pipelines WHERE tenant_id = $1")
            .bind(t1.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(remaining, 0, "the tenant's boards are purged with it");
    // B's board is untouched by A's deletion.
    assert!(b.crm_pipeline(&b_renewals).await.unwrap().is_some());
    store.delete_tenant(&t2).await.unwrap();
}

#[tokio::test]
async fn crm_stages_round_trip_and_never_cross_tenant() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "sa").await;
    let uc = store
        .for_tenant(t1.clone())
        .create_user("sc@crm.test")
        .await
        .unwrap();
    let c = store.for_account(t1.clone(), uc);
    let (b, t2) = tenant_with_user(&store, "sb").await;

    let board = a
        .create_crm_pipeline(&NewPipeline {
            name: "Sales".to_owned(),
            description: String::new(),
        })
        .await
        .unwrap();

    // ---- append: columns land left to right ------------------------------
    let new = a
        .create_crm_stage(&board, &column("  New  "))
        .await
        .unwrap();
    let qualified = c
        .create_crm_stage(&board, &column("Qualified"))
        .await
        .unwrap();
    let won = a
        .create_crm_stage(
            &board,
            &NewStage {
                name: "Won".to_owned(),
                is_won: true,
                is_lost: false,
            },
        )
        .await
        .unwrap();
    let listed = a.crm_stages(&board, false).await.unwrap();
    assert_eq!(names(&listed), vec!["New", "Qualified", "Won"]);
    assert!(
        listed[0].position < listed[1].position && listed[1].position < listed[2].position,
        "appended columns keep their order"
    );
    assert_eq!(listed[0].pipeline_id, board);
    assert!(!listed[0].is_closed());
    assert!(listed[2].is_closed());
    assert_eq!(
        names(&c.crm_stages(&board, false).await.unwrap()),
        vec!["New", "Qualified", "Won"],
        "a co-tenant user reads the same board"
    );

    // ---- the board rules that make a win rate mean something -------------
    assert_invalid(
        a.create_crm_stage(
            &board,
            &NewStage {
                name: "Also won".to_owned(),
                is_won: true,
                is_lost: false,
            },
        )
        .await,
        "one won stage",
    );
    assert_invalid(
        a.create_crm_stage(
            &board,
            &NewStage {
                name: "Both".to_owned(),
                is_won: true,
                is_lost: true,
            },
        )
        .await,
        "both",
    );
    assert_invalid(
        a.update_crm_stage(
            &new,
            &NewStage {
                name: "New".to_owned(),
                is_won: true,
                is_lost: false,
            },
        )
        .await,
        "one won stage",
    );
    // The second board may have its own winning column: the rule is per
    // pipeline, not per tenant.
    let other_board = a
        .create_crm_pipeline(&NewPipeline {
            name: "Renewals".to_owned(),
            description: String::new(),
        })
        .await
        .unwrap();
    let other_won = a
        .create_crm_stage(
            &other_board,
            &NewStage {
                name: "Won".to_owned(),
                is_won: true,
                is_lost: false,
            },
        )
        .await
        .unwrap();
    assert!(a.crm_stage(&other_won).await.unwrap().unwrap().is_won);

    // ---- validation on the name ------------------------------------------
    let too_long = "x".repeat(STAGE_NAME_MAX_CHARS + 1);
    for bad in ["  ", too_long.as_str()] {
        assert_invalid(a.create_crm_stage(&board, &column(bad)).await, "stage name");
        assert_invalid(a.update_crm_stage(&new, &column(bad)).await, "stage name");
    }

    // ---- another tenant: denied on every path ----------------------------
    assert_not_found(b.crm_stages(&board, true).await);
    assert_not_found(b.create_crm_stage(&board, &column("Theirs")).await);
    assert!(b.crm_stage(&new).await.unwrap().is_none());
    assert_not_found(b.update_crm_stage(&new, &column("Hijacked")).await);
    assert_not_found(b.move_crm_stage(&new, 99.0).await);
    assert_not_found(b.set_crm_stage_archived(&new, true).await);
    assert_not_found(b.delete_crm_stage(&new).await);
    // ... and nothing they tried changed A's board.
    assert_eq!(
        names(&a.crm_stages(&board, true).await.unwrap()),
        vec!["New", "Qualified", "Won"]
    );

    // A stage id that never existed answers the same way.
    let ghost = CrmStageId::generate();
    assert!(a.crm_stage(&ghost).await.unwrap().is_none());
    assert_not_found(a.update_crm_stage(&ghost, &column("Ghost")).await);
    assert_not_found(a.move_crm_stage(&ghost, 1.0).await);
    assert_not_found(a.set_crm_stage_archived(&ghost, true).await);
    assert_not_found(a.delete_crm_stage(&ghost).await);

    // ---- move: the one operation a drag performs -------------------------
    // Fractional, so "Won" goes between "New" and "Qualified" without
    // rewriting either.
    let between = (listed[0].position + listed[1].position) / 2.0;
    a.move_crm_stage(&won, between).await.unwrap();
    assert_eq!(
        names(&a.crm_stages(&board, false).await.unwrap()),
        vec!["New", "Won", "Qualified"]
    );
    assert_invalid(a.move_crm_stage(&won, f64::NAN).await, "finite");
    assert_invalid(a.move_crm_stage(&won, f64::INFINITY).await, "finite");
    assert_eq!(
        a.crm_stage(&won).await.unwrap().unwrap().position,
        between,
        "a refused move left the column where it was"
    );
    // A move cannot rename, and an edit cannot move.
    a.update_crm_stage(
        &won,
        &NewStage {
            name: "Signed".to_owned(),
            is_won: true,
            is_lost: false,
        },
    )
    .await
    .unwrap();
    let signed = a.crm_stage(&won).await.unwrap().unwrap();
    assert_eq!(signed.name, "Signed");
    assert_eq!(signed.position, between);
    assert!(signed.is_won, "the flag survives a rename");

    // ---- archive: off the board, still in the data -----------------------
    a.set_crm_stage_archived(&qualified, true).await.unwrap();
    assert_eq!(
        names(&a.crm_stages(&board, false).await.unwrap()),
        vec!["New", "Signed"]
    );
    let hidden = a.crm_stage(&qualified).await.unwrap().unwrap();
    assert!(hidden.is_archived());
    let archived_at = hidden.archived_at;
    a.set_crm_stage_archived(&qualified, true).await.unwrap();
    assert_eq!(
        a.crm_stage(&qualified).await.unwrap().unwrap().archived_at,
        archived_at,
        "re-archiving keeps the original time"
    );
    // Included, it keeps its place rather than being pushed to the end.
    assert_eq!(
        names(&a.crm_stages(&board, true).await.unwrap()),
        vec!["New", "Signed", "Qualified"]
    );
    a.set_crm_stage_archived(&qualified, false).await.unwrap();
    assert!(
        !a.crm_stage(&qualified)
            .await
            .unwrap()
            .unwrap()
            .is_archived()
    );

    // ---- delete: the escape hatch, never the last column -----------------
    a.delete_crm_stage(&qualified).await.unwrap();
    assert!(a.crm_stage(&qualified).await.unwrap().is_none());
    assert_eq!(
        names(&a.crm_stages(&board, true).await.unwrap()),
        vec!["New", "Signed"]
    );
    a.delete_crm_stage(&won).await.unwrap();
    assert_conflict(a.delete_crm_stage(&new).await);
    assert_eq!(
        names(&a.crm_stages(&board, true).await.unwrap()),
        vec!["New"],
        "the refused delete left the board with its one column"
    );

    // ---- archiving the board takes its columns with it -------------------
    a.set_crm_pipeline_archived(&other_board, true)
        .await
        .unwrap();
    assert_eq!(
        a.crm_stages(&other_board, false).await.unwrap().len(),
        1,
        "the columns of an archived board are still readable through it"
    );

    store.delete_tenant(&t1).await.unwrap();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap();
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM crm_stages WHERE tenant_id = $1")
        .bind(t1.as_str())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, 0, "the tenant's columns are purged with it");
    store.delete_tenant(&t2).await.unwrap();
}

#[tokio::test]
async fn the_first_read_seeds_one_board_even_when_two_arrive_at_once() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "seed").await;
    let uc = store
        .for_tenant(t1.clone())
        .create_user("seed-c@crm.test")
        .await
        .unwrap();
    let c = store.for_account(t1.clone(), uc);
    let (b, t2) = tenant_with_user(&store, "seed-b").await;

    // A tenant that never opens CRM has no rows at all.
    assert!(a.crm_pipelines(true).await.unwrap().is_empty());

    // Two colleagues open the module in the same instant. Exactly one board
    // exists afterwards, with exactly one set of columns.
    let seed = sales_seed();
    let (first, second) = tokio::join!(
        a.crm_pipelines_or_seed(&seed),
        c.crm_pipelines_or_seed(&seed)
    );
    let first = first.unwrap();
    let second = second.unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert_eq!(first[0].id, second[0].id, "one board, not two");
    assert_eq!(a.crm_pipelines(true).await.unwrap().len(), 1);

    // The seeded board arrived whole: five columns, in the caller's language
    // and the caller's order, with one winning and one losing column.
    let board = first[0].id.clone();
    assert_eq!(first[0].name, "Vente");
    let stages = a.crm_stages(&board, true).await.unwrap();
    assert_eq!(
        names(&stages),
        vec!["Nouveau", "Qualifié", "Proposition", "Gagné", "Perdu"]
    );
    assert!(stages[3].is_won && !stages[3].is_lost);
    assert!(stages[4].is_lost && !stages[4].is_won);
    assert!(stages.iter().all(|s| !s.is_archived()));

    // Seeding is a first-use rule, not an every-read one: a tenant that
    // renamed or archived its board is never handed a new one.
    a.update_crm_pipeline(
        &board,
        &NewPipeline {
            name: "Notre pipeline".to_owned(),
            description: String::new(),
        },
    )
    .await
    .unwrap();
    let again = a.crm_pipelines_or_seed(&sales_seed()).await.unwrap();
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].name, "Notre pipeline");
    a.set_crm_pipeline_archived(&board, true).await.unwrap();
    assert!(
        a.crm_pipelines_or_seed(&sales_seed())
            .await
            .unwrap()
            .is_empty(),
        "an archived board still counts as having opened the module"
    );
    assert_eq!(a.crm_pipelines(true).await.unwrap().len(), 1);

    // The seed is another tenant's business only. B's first read gives B its
    // own board, and A's rows are not among them.
    let b_boards = b.crm_pipelines_or_seed(&sales_seed()).await.unwrap();
    assert_eq!(b_boards.len(), 1);
    assert_ne!(b_boards[0].id, board);
    assert!(b.crm_pipeline(&board).await.unwrap().is_none());
    assert_not_found(b.crm_stages(&board, true).await);

    // A malformed seed is refused rather than half-written — and refusing it
    // leaves the tenant with no rows, not with a board and no columns.
    let (d, t3) = tenant_with_user(&store, "seed-d").await;
    assert_invalid(
        d.crm_pipelines_or_seed(&PipelineSeed {
            name: "Sales".to_owned(),
            stages: vec![
                stage_seed("Won", true, false),
                stage_seed("Also", true, false),
            ],
        })
        .await,
        "one won stage",
    );
    assert_invalid(
        d.crm_pipelines_or_seed(&PipelineSeed {
            name: "Sales".to_owned(),
            stages: Vec::new(),
        })
        .await,
        "at least one stage",
    );
    assert!(d.crm_pipelines(true).await.unwrap().is_empty());

    for tenant in [t1, t2, t3] {
        store.delete_tenant(&tenant).await.unwrap();
    }
}

#[tokio::test]
async fn a_board_may_not_grow_past_the_stage_cap() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "cap").await;

    // Seed a board that is exactly full — one transaction rather than two
    // hundred round trips, and a check that the cap is inclusive.
    let full = PipelineSeed {
        name: "Full".to_owned(),
        stages: (0..STAGES_PER_PIPELINE_MAX)
            .map(|i| stage_seed(&format!("Stage {i}"), false, false))
            .collect(),
    };
    let boards = a.crm_pipelines_or_seed(&full).await.unwrap();
    let board = boards[0].id.clone();
    assert_eq!(
        a.crm_stages(&board, true).await.unwrap().len(),
        STAGES_PER_PIPELINE_MAX
    );

    assert_invalid(
        a.create_crm_stage(&board, &column("One too many")).await,
        "at most",
    );
    // Archived columns still count: the cap is on the rows, and an archived
    // column is a row a deal can still point at.
    let stages = a.crm_stages(&board, false).await.unwrap();
    a.set_crm_stage_archived(&stages[0].id, true).await.unwrap();
    assert_invalid(
        a.create_crm_stage(&board, &column("Still too many")).await,
        "at most",
    );
    // Deleting one makes room.
    a.delete_crm_stage(&stages[0].id).await.unwrap();
    a.create_crm_stage(&board, &column("Room now"))
        .await
        .unwrap();
    assert_eq!(
        a.crm_stages(&board, true).await.unwrap().len(),
        STAGES_PER_PIPELINE_MAX
    );

    store.delete_tenant(&t1).await.unwrap();
}
