//! Tenancy proof for alo CRM's deals and their stage history (Law 1:
//! isolation is tested, not assumed). Deals are tenant-wide — a co-tenant
//! works the same board — but an outsider tenant gets the clean
//! `NotFound`/empty on **every** path: read, list, create, update, move,
//! history and delete. It also proves the three links a deal carries can never
//! cross a tenant (customer, contact, owner), the CRUD arc the queue item
//! requires, that a move writes exactly one history row, that the closing
//! snapshot is written and cleared at the right moments, and the guards that
//! now count deals: a column or a board holding open work cannot be archived,
//! and a column the past has named cannot be deleted.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::crm_deals::{
    DEAL_EMAIL_MAX_CHARS, DEAL_PARTY_MAX_CHARS, DEAL_SOURCE_MAX_CHARS, DEAL_TITLE_MAX_CHARS,
    DEAL_VALUE_MAX_CENTS,
};
use alo_store::{
    AccountStore, ContactId, CrmDealId, CrmStageId, Deal, DealFilter, DealState, NewCustomer,
    NewDeal, NewPipeline, NewStage, PipelineSeed, StageMove, StageSeed, Store, StoreError,
    TenantId, UserId,
};
use time::{Date, Month};

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

/// A tenant with one user, returning the account door, the tenant id and the
/// user id.
async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId, UserId) {
    let tenant = store.create_tenant(&format!("crmd-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@crmd.test"))
        .await
        .unwrap();
    (
        store.for_account(tenant.clone(), user.clone()),
        tenant,
        user,
    )
}

fn stage_seed(name: &str, is_won: bool, is_lost: bool) -> StageSeed {
    StageSeed {
        name: name.to_owned(),
        is_won,
        is_lost,
    }
}

/// The five-column board a tenant is handed on first use.
fn sales_seed() -> PipelineSeed {
    PipelineSeed {
        name: "Sales".to_owned(),
        stages: vec![
            stage_seed("New", false, false),
            stage_seed("Qualified", false, false),
            stage_seed("Proposal", false, false),
            stage_seed("Won", true, false),
            stage_seed("Lost", false, true),
        ],
    }
}

/// The board a tenant opens the module onto, as (pipeline id, columns).
async fn seeded_board(store: &AccountStore) -> (alo_store::CrmPipelineId, Vec<CrmStageId>) {
    let boards = store.crm_pipelines_or_seed(&sales_seed()).await.unwrap();
    let board = boards[0].id.clone();
    let stages = store
        .crm_stages(&board, false)
        .await
        .unwrap()
        .into_iter()
        .map(|s| s.id)
        .collect();
    (board, stages)
}

fn deal(title: &str) -> NewDeal {
    NewDeal {
        title: title.to_owned(),
        ..Default::default()
    }
}

fn titles(deals: &[Deal]) -> Vec<&str> {
    deals.iter().map(|d| d.title.as_str()).collect()
}

#[tokio::test]
async fn crm_deals_round_trip_and_never_cross_tenant() {
    let store = common::test_store().await;
    let (a, t1, ua) = tenant_with_user(&store, "a").await;
    // A co-tenant user of the same tenant: deals are tenant-wide.
    let uc = store
        .for_tenant(t1.clone())
        .create_user("c@crmd.test")
        .await
        .unwrap();
    let c = store.for_account(t1.clone(), uc.clone());
    let (b, t2, ub) = tenant_with_user(&store, "b").await;

    let (board, stages) = seeded_board(&a).await;
    let (new, qualified) = (stages[0].clone(), stages[1].clone());
    let (b_board, b_stages) = seeded_board(&b).await;

    // ---- create: normalised on the way in, open, owned by the author ------
    let id = a
        .create_crm_deal(
            &board,
            &new,
            &NewDeal {
                title: "  Renewal — Acme GmbH  ".to_owned(),
                company_name: "  Acme GmbH  ".to_owned(),
                contact_name: "Ada".to_owned(),
                contact_email: "  ada@acme.example  ".to_owned(),
                value_cents: 1_250_000,
                currency: "eur".to_owned(),
                expected_close: Date::from_calendar_date(2026, Month::December, 1).ok(),
                source: "  referral  ".to_owned(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let got = a.crm_deal(&id).await.unwrap().unwrap();
    assert_eq!(got.title, "Renewal — Acme GmbH");
    assert_eq!(got.company_name, "Acme GmbH");
    assert_eq!(got.contact_email, "ada@acme.example");
    assert_eq!(
        got.currency, "EUR",
        "the code is uppercased once, in one place"
    );
    assert_eq!(got.value_cents, 1_250_000);
    assert_eq!(got.source, "referral");
    assert_eq!(got.stage_id, new);
    assert_eq!(got.pipeline_id, board);
    assert_eq!(
        got.owner_user_id,
        ua.as_str(),
        "the author owns it by default"
    );
    assert_eq!(got.created_by, ua.as_str());
    assert_eq!(got.state(), DealState::Open);
    assert!(!got.is_closed());
    assert!(got.closed_at.is_none() && got.lost_reason.is_none());
    assert!(got.customer_id.is_none(), "a lead has no customer yet");

    // The creation is row one of the history, with nowhere to come from.
    let history = a.crm_deal_history(&id).await.unwrap();
    assert_eq!(history.len(), 1);
    assert!(history[0].from_stage_id.is_none());
    assert_eq!(history[0].to_stage_id, new);
    assert_eq!(history[0].moved_by, ua.as_str());
    assert_eq!(history[0].deal_id, id);

    // ---- list: tenant-wide, in board order, filters compose --------------
    let second = c
        .create_crm_deal(&board, &qualified, &deal("Pilot — Beta BV"))
        .await
        .unwrap();
    let all = a.crm_deals(&DealFilter::default()).await.unwrap();
    assert_eq!(
        titles(&all),
        vec!["Renewal — Acme GmbH", "Pilot — Beta BV"],
        "column by column, left to right"
    );
    assert_eq!(
        titles(&c.crm_deals(&DealFilter::default()).await.unwrap()),
        titles(&all),
        "a co-tenant user reads the same deals"
    );
    let mine = a
        .crm_deals(&DealFilter {
            owner_user_id: Some(uc.as_str().to_owned()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(titles(&mine), vec!["Pilot — Beta BV"]);
    let in_column = a
        .crm_deals(&DealFilter {
            stage_id: Some(new.clone()),
            state: Some(DealState::Open),
            pipeline_id: Some(board.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(titles(&in_column), vec!["Renewal — Acme GmbH"]);
    assert!(
        a.crm_deals(&DealFilter {
            state: Some(DealState::Won),
            ..Default::default()
        })
        .await
        .unwrap()
        .is_empty(),
        "nothing is won yet"
    );

    // ---- another tenant: denied on every path -----------------------------
    assert!(b.crm_deal(&id).await.unwrap().is_none());
    assert_not_found(b.update_crm_deal(&id, &deal("Hijacked")).await);
    assert_not_found(
        b.move_crm_deal(&id, &StageMove::to(qualified.clone()))
            .await,
    );
    assert_not_found(b.crm_deal_history(&id).await);
    assert_not_found(b.delete_crm_deal(&id).await);
    assert_not_found(b.create_crm_deal(&board, &new, &deal("Theirs")).await);
    assert!(
        b.crm_deals(&DealFilter {
            pipeline_id: Some(board.clone()),
            ..Default::default()
        })
        .await
        .unwrap()
        .is_empty(),
        "another tenant's board matches nothing, exactly like one that never existed"
    );
    // A's deal may not be dropped into B's column either — the column is not
    // this tenant's at all.
    assert_not_found(
        a.move_crm_deal(&id, &StageMove::to(b_stages[0].clone()))
            .await,
    );
    // ... and nothing they tried changed A's deal.
    let after = a.crm_deal(&id).await.unwrap().unwrap();
    assert_eq!(after.title, "Renewal — Acme GmbH");
    assert_eq!(after.stage_id, new);
    assert_eq!(a.crm_deal_history(&id).await.unwrap().len(), 1);

    // An id that never existed answers exactly as another tenant's does.
    let ghost = CrmDealId::generate();
    assert!(a.crm_deal(&ghost).await.unwrap().is_none());
    assert_not_found(a.update_crm_deal(&ghost, &deal("Ghost")).await);
    assert_not_found(a.move_crm_deal(&ghost, &StageMove::to(new.clone())).await);
    assert_not_found(a.crm_deal_history(&ghost).await);
    assert_not_found(a.delete_crm_deal(&ghost).await);

    // ---- the three links a deal carries never cross a tenant --------------
    let b_customer = b
        .create_billing_customer(&NewCustomer {
            name: "Their customer".to_owned(),
            country: "DE".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_not_found(
        a.create_crm_deal(
            &board,
            &new,
            &NewDeal {
                customer_id: Some(b_customer.clone()),
                ..deal("Guessed customer")
            },
        )
        .await,
    );
    assert_not_found(
        a.update_crm_deal(
            &id,
            &NewDeal {
                customer_id: Some(b_customer),
                ..deal("Guessed customer")
            },
        )
        .await,
    );
    assert_not_found(
        a.create_crm_deal(
            &board,
            &new,
            &NewDeal {
                contact_id: Some(ContactId::generate()),
                ..deal("Guessed contact")
            },
        )
        .await,
    );
    assert_invalid(
        a.create_crm_deal(
            &board,
            &new,
            &NewDeal {
                owner_user_id: Some(ub.as_str().to_owned()),
                ..deal("Guessed owner")
            },
        )
        .await,
        "user of this tenant",
    );
    // A colleague of the same tenant is a legitimate owner.
    a.update_crm_deal(
        &id,
        &NewDeal {
            owner_user_id: Some(uc.as_str().to_owned()),
            value_cents: 2_000_000,
            ..deal("Renewal — Acme GmbH")
        },
    )
    .await
    .unwrap();
    let reassigned = a.crm_deal(&id).await.unwrap().unwrap();
    assert_eq!(reassigned.owner_user_id, uc.as_str());
    assert_eq!(reassigned.value_cents, 2_000_000);
    assert_eq!(
        reassigned.source, "",
        "an update is a full replace, not a merge"
    );
    assert_eq!(
        reassigned.stage_id, new,
        "an edit cannot move a deal — that is its own call"
    );

    // ---- the customer link, when there is a customer ----------------------
    let customer = a
        .create_billing_customer(&NewCustomer {
            name: "Acme GmbH".to_owned(),
            country: "DE".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    a.update_crm_deal(
        &id,
        &NewDeal {
            customer_id: Some(customer.clone()),
            ..deal("Renewal — Acme GmbH")
        },
    )
    .await
    .unwrap();
    assert_eq!(
        a.crm_deal(&id).await.unwrap().unwrap().customer_id,
        Some(customer.clone())
    );
    // Archiving a customer means "we no longer do business with them": a new
    // deal for one is a mistake worth reporting rather than obeying.
    a.set_billing_customer_archived(&customer, true)
        .await
        .unwrap();
    assert_invalid(
        a.create_crm_deal(
            &board,
            &new,
            &NewDeal {
                customer_id: Some(customer.clone()),
                ..deal("Archived customer")
            },
        )
        .await,
        "archived",
    );
    a.set_billing_customer_archived(&customer, false)
        .await
        .unwrap();

    // ---- validation guards both write paths -------------------------------
    let invalid: Vec<(NewDeal, &str)> = vec![
        (deal("   "), "title"),
        (deal(&"x".repeat(DEAL_TITLE_MAX_CHARS + 1)), "title"),
        (
            NewDeal {
                value_cents: -1,
                ..deal("Negative")
            },
            "deal value",
        ),
        (
            NewDeal {
                value_cents: DEAL_VALUE_MAX_CENTS + 1,
                ..deal("Too big")
            },
            "deal value",
        ),
        (
            NewDeal {
                currency: "EURO".to_owned(),
                ..deal("Bad currency")
            },
            "ISO 4217",
        ),
        (
            NewDeal {
                source: "x".repeat(DEAL_SOURCE_MAX_CHARS + 1),
                ..deal("Long source")
            },
            "source",
        ),
        (
            NewDeal {
                company_name: "x".repeat(DEAL_PARTY_MAX_CHARS + 1),
                ..deal("Long company")
            },
            "company name",
        ),
        (
            NewDeal {
                contact_name: "x".repeat(DEAL_PARTY_MAX_CHARS + 1),
                ..deal("Long contact")
            },
            "contact name",
        ),
        (
            NewDeal {
                contact_email: "x".repeat(DEAL_EMAIL_MAX_CHARS + 1),
                ..deal("Long email")
            },
            "contact email",
        ),
    ];
    for (bad, rule) in &invalid {
        assert_invalid(a.create_crm_deal(&board, &new, bad).await, rule);
        assert_invalid(a.update_crm_deal(&id, bad).await, rule);
    }
    // A rejected write left the record and the list untouched.
    assert_eq!(
        a.crm_deal(&id).await.unwrap().unwrap().title,
        "Renewal — Acme GmbH"
    );
    assert_eq!(a.crm_deals(&DealFilter::default()).await.unwrap().len(), 2);
    // The bounds are inclusive.
    let edge = a
        .create_crm_deal(
            &board,
            &new,
            &NewDeal {
                value_cents: DEAL_VALUE_MAX_CENTS,
                source: "x".repeat(DEAL_SOURCE_MAX_CHARS),
                ..deal(&"x".repeat(DEAL_TITLE_MAX_CHARS))
            },
        )
        .await
        .unwrap();
    a.delete_crm_deal(&edge).await.unwrap();

    // ---- delete: the record and its history go together -------------------
    a.delete_crm_deal(&second).await.unwrap();
    assert!(a.crm_deal(&second).await.unwrap().is_none());
    assert_not_found(a.crm_deal_history(&second).await);
    assert_eq!(
        titles(&a.crm_deals(&DealFilter::default()).await.unwrap()),
        vec!["Renewal — Acme GmbH"]
    );

    // ---- deleting the tenant purges deals and their history ---------------
    // Read the rows directly: the claim is that they were cascaded away, not
    // merely hidden behind the tenant predicate of the list call.
    store.delete_tenant(&t1).await.unwrap();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap();
    for table in ["crm_deals", "crm_deal_stage_events"] {
        let remaining: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {table} WHERE tenant_id = $1"
        ))
        .bind(t1.as_str())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(remaining, 0, "{table} is purged with the tenant");
    }
    // B's board is untouched by A's deletion.
    assert!(b.crm_pipeline(&b_board).await.unwrap().is_some());
    store.delete_tenant(&t2).await.unwrap();
}

#[tokio::test]
async fn a_move_writes_one_history_row_and_the_closing_snapshot() {
    let store = common::test_store().await;
    let (a, t1, ua) = tenant_with_user(&store, "move").await;
    let (board, stages) = seeded_board(&a).await;
    let (new, qualified, proposal, won, lost) = (
        stages[0].clone(),
        stages[1].clone(),
        stages[2].clone(),
        stages[3].clone(),
        stages[4].clone(),
    );

    let id = a
        .create_crm_deal(&board, &new, &deal("Renewal"))
        .await
        .unwrap();
    let first_position = a.crm_deal(&id).await.unwrap().unwrap().position;

    // ---- a real move: one row, from and to ------------------------------
    a.move_crm_deal(&id, &StageMove::to(qualified.clone()))
        .await
        .unwrap();
    let history = a.crm_deal_history(&id).await.unwrap();
    assert_eq!(history.len(), 2, "exactly one row per move");
    assert_eq!(history[1].from_stage_id, Some(new.clone()));
    assert_eq!(history[1].to_stage_id, qualified);
    assert_eq!(history[1].moved_by, ua.as_str());
    assert!(history[1].moved_at >= history[0].moved_at);
    let moved = a.crm_deal(&id).await.unwrap().unwrap();
    assert_eq!(moved.stage_id, qualified);
    assert_eq!(moved.state(), DealState::Open);

    // ---- a reposition within one column is not a move --------------------
    let neighbour = a
        .create_crm_deal(&board, &qualified, &deal("Pilot"))
        .await
        .unwrap();
    let between = (first_position + moved.position) / 2.0;
    a.move_crm_deal(
        &id,
        &StageMove {
            stage_id: qualified.clone(),
            position: Some(between),
            lost_reason: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        a.crm_deal_history(&id).await.unwrap().len(),
        2,
        "a card that stayed in its column wrote no history"
    );
    assert_eq!(a.crm_deal(&id).await.unwrap().unwrap().position, between);
    assert_invalid(
        a.move_crm_deal(
            &id,
            &StageMove {
                stage_id: qualified.clone(),
                position: Some(f64::NAN),
                lost_reason: None,
            },
        )
        .await,
        "finite",
    );
    // The refused move left the card where it was.
    assert_eq!(a.crm_deal(&id).await.unwrap().unwrap().position, between);
    a.delete_crm_deal(&neighbour).await.unwrap();

    // ---- winning: the snapshot is written, not derived -------------------
    a.move_crm_deal(&id, &StageMove::to(won.clone()))
        .await
        .unwrap();
    let win = a.crm_deal(&id).await.unwrap().unwrap();
    assert_eq!(win.state(), DealState::Won);
    assert!(win.is_closed());
    let won_at = win.closed_at.expect("a closed deal carries the moment");
    assert!(win.lost_reason.is_none());
    assert_eq!(
        titles(
            &a.crm_deals(&DealFilter {
                state: Some(DealState::Won),
                ..Default::default()
            })
            .await
            .unwrap()
        ),
        vec!["Renewal"]
    );

    // ---- losing: a reason is the feature, not the friction ---------------
    assert_invalid(
        a.move_crm_deal(&id, &StageMove::to(lost.clone())).await,
        "reason",
    );
    // The refused move left the deal won.
    assert_eq!(
        a.crm_deal(&id).await.unwrap().unwrap().state(),
        DealState::Won
    );
    // And a reason where it does not belong is refused just as firmly.
    assert_invalid(
        a.move_crm_deal(
            &id,
            &StageMove {
                stage_id: proposal.clone(),
                position: None,
                lost_reason: Some("Price".to_owned()),
            },
        )
        .await,
        "losing stage",
    );
    a.move_crm_deal(
        &id,
        &StageMove {
            stage_id: lost.clone(),
            position: None,
            lost_reason: Some("  Price  ".to_owned()),
        },
    )
    .await
    .unwrap();
    let loss = a.crm_deal(&id).await.unwrap().unwrap();
    assert_eq!(loss.state(), DealState::Lost);
    assert_eq!(loss.lost_reason.as_deref(), Some("Price"));
    assert!(
        loss.closed_at.expect("still closed") >= won_at,
        "a new outcome is stamped at the moment it was reached"
    );

    // ---- reopening: allowed, and it clears the snapshot -------------------
    a.move_crm_deal(&id, &StageMove::to(proposal.clone()))
        .await
        .unwrap();
    let reopened = a.crm_deal(&id).await.unwrap().unwrap();
    assert_eq!(reopened.state(), DealState::Open);
    assert!(reopened.closed_at.is_none() && reopened.lost_reason.is_none());
    let history = a.crm_deal_history(&id).await.unwrap();
    assert_eq!(history.len(), 5, "every move it ever made is still there");
    assert_eq!(history[4].from_stage_id, Some(lost.clone()));
    assert_eq!(history[4].to_stage_id, proposal);

    // ---- a board is not a place to lose a deal into another funnel -------
    let other = a
        .create_crm_pipeline(&NewPipeline {
            name: "Renewals".to_owned(),
            description: String::new(),
        })
        .await
        .unwrap();
    let elsewhere = a
        .create_crm_stage(
            &other,
            &NewStage {
                name: "Theirs".to_owned(),
                is_won: false,
                is_lost: false,
            },
        )
        .await
        .unwrap();
    assert_invalid(
        a.move_crm_deal(&id, &StageMove::to(elsewhere)).await,
        "another pipeline",
    );
    assert_not_found(
        a.move_crm_deal(&id, &StageMove::to(CrmStageId::generate()))
            .await,
    );

    // ---- an archived column takes no new cards ---------------------------
    a.set_crm_stage_archived(&new, true).await.unwrap();
    assert_invalid(
        a.move_crm_deal(&id, &StageMove::to(new.clone())).await,
        "archived",
    );
    assert_invalid(
        a.create_crm_deal(&board, &new, &deal("Into an archived column"))
            .await,
        "archived",
    );
    assert_eq!(
        a.crm_deal(&id).await.unwrap().unwrap().stage_id,
        proposal,
        "the refused move left the card where it was"
    );

    store.delete_tenant(&t1).await.unwrap();
}

#[tokio::test]
async fn a_column_holding_work_cannot_be_archived_and_one_with_a_past_cannot_be_deleted() {
    let store = common::test_store().await;
    let (a, t1, _) = tenant_with_user(&store, "guard").await;
    let (board, stages) = seeded_board(&a).await;
    let (new, qualified, won) = (stages[0].clone(), stages[1].clone(), stages[3].clone());

    let id = a
        .create_crm_deal(&board, &qualified, &deal("Renewal"))
        .await
        .unwrap();

    // ---- open work blocks the archive, on the column and on the board ----
    assert_conflict(a.set_crm_stage_archived(&qualified, true).await);
    assert!(
        !a.crm_stage(&qualified)
            .await
            .unwrap()
            .unwrap()
            .is_archived(),
        "the refused archive left the column on the board"
    );
    assert_conflict(a.set_crm_pipeline_archived(&board, true).await);
    assert!(!a.crm_pipeline(&board).await.unwrap().unwrap().is_archived());

    // A column that never held anything archives as it always did.
    a.set_crm_stage_archived(&new, true).await.unwrap();
    a.set_crm_stage_archived(&new, false).await.unwrap();

    // ---- closing the work releases both guards ---------------------------
    a.move_crm_deal(&id, &StageMove::to(won.clone()))
        .await
        .unwrap();
    a.set_crm_stage_archived(&qualified, true).await.unwrap();
    assert!(
        a.crm_stage(&qualified)
            .await
            .unwrap()
            .unwrap()
            .is_archived()
    );
    // The board holds only closed deals now, so it may be retired.
    a.set_crm_pipeline_archived(&board, true).await.unwrap();
    a.set_crm_pipeline_archived(&board, false).await.unwrap();

    // ---- a column the past has named is archived, never deleted ----------
    // `Won` holds the deal; `Qualified` holds only its history rows. Both are
    // refused, and the message points at the archive.
    assert_conflict(a.delete_crm_stage(&won).await);
    assert_conflict(a.delete_crm_stage(&qualified).await);
    assert!(
        a.crm_stage(&qualified).await.unwrap().is_some(),
        "the refused delete left the column where the history can find it"
    );
    // Deleting the deal releases the columns it stood in — its history goes
    // with it, so nothing points at them any more.
    a.delete_crm_deal(&id).await.unwrap();
    a.delete_crm_stage(&qualified).await.unwrap();
    assert!(a.crm_stage(&qualified).await.unwrap().is_none());

    store.delete_tenant(&t1).await.unwrap();
}

#[tokio::test]
async fn a_card_cannot_slip_into_a_column_as_it_is_being_archived() {
    let store = common::test_store().await;
    let (a, t1, _) = tenant_with_user(&store, "race").await;
    let (board, stages) = seeded_board(&a).await;
    let (new, qualified) = (stages[0].clone(), stages[1].clone());

    let id = a
        .create_crm_deal(&board, &new, &deal("Renewal"))
        .await
        .unwrap();

    // One colleague drags the card into "Qualified" while another archives
    // that very column. Both hold the board row — the mover shares it, the
    // archiver takes it exclusively — so they cannot interleave.
    let drag = StageMove::to(qualified.clone());
    let (moved, archived) = tokio::join!(
        a.move_crm_deal(&id, &drag),
        a.set_crm_stage_archived(&qualified, true)
    );
    assert!(
        !(moved.is_ok() && archived.is_ok()),
        "one of the two must lose: {moved:?} / {archived:?}"
    );
    let card = a.crm_deal(&id).await.unwrap().unwrap();
    let column = a.crm_stage(&qualified).await.unwrap().unwrap();
    assert!(
        !(card.stage_id == qualified && column.is_archived() && !card.is_closed()),
        "open work never ends up standing in an archived column"
    );
    // Whichever won, the deal's history still says exactly what happened.
    let history = a.crm_deal_history(&id).await.unwrap();
    assert_eq!(history.len(), usize::from(moved.is_ok()) + 1);

    store.delete_tenant(&t1).await.unwrap();
}
