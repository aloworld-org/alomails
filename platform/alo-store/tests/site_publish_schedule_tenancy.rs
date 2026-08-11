//! Scheduled publishing (ADR 0036, S2.05a): the model behind "go live on
//! Monday at 09:00".
//!
//! Three properties are load-bearing and are proved here against a real
//! Postgres. **One future per website** — scheduling twice moves one
//! intention instead of creating two. **At-most-once under concurrent
//! sweepers** — two claims running at the same instant never hand the same
//! schedule out twice, and a worker that dies is retried a bounded number of
//! times before the row fails visibly. And **tenant scope** — another
//! tenant's schedule cannot be read, moved, cancelled, claimed on behalf of,
//! completed, or failed, and is indistinguishable from one that never existed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::LazyLock;

use alo_store::{
    SiteId, SitePublishId, SitePublishScheduleId, SitePublishScheduleStatus, SiteStatus, StoreError,
};
use serde_json::json;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use time::{Duration, OffsetDateTime};
use tokio::sync::{Mutex, MutexGuard};

/// The sweeper claims across tenants by design, so two tests sweeping at once
/// would steal each other's due rows. Every test that sweeps takes this first;
/// the concurrency this suite actually exercises is inside a single test.
static SWEEP: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

async fn sweeping() -> MutexGuard<'static, ()> {
    SWEEP.lock().await
}

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

fn assert_conflict<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Conflict(detail)) => detail,
        other => panic!("expected Conflict, got {other:?}"),
    }
}

fn assert_validation<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Validation(detail)) => detail,
        other => panic!("expected Validation, got {other:?}"),
    }
}

fn subdomain(tag: &str) -> String {
    format!(
        "{tag}-{}",
        SiteId::generate()
            .as_str()
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .take(12)
            .collect::<String>()
            .to_ascii_lowercase()
    )
}

/// A site with a home page, ready to be published.
async fn ready_site(account: &alo_store::AccountStore, tag: &str) -> SiteId {
    let site = account.create_site("Acme", &subdomain(tag)).await.unwrap();
    let home = account
        .create_site_page(&site, "Home", "", true)
        .await
        .unwrap();
    account
        .set_page_sections(
            &site,
            &home,
            json!({"schema_version": 1, "sections": [{"type": "hero", "heading": "Later"}]}),
        )
        .await
        .unwrap();
    site
}

/// The whole arc: schedule, move the moment, let it come due, have a worker
/// claim it, publish through the scheduling user's door, and record the
/// version it produced — with the site live and the schedule readable
/// afterwards.
#[tokio::test]
async fn a_scheduled_publish_moves_claims_and_records_the_version_it_made() {
    let _sweeping = sweeping().await;
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let tenant = store.create_tenant("site-schedule").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("owner@site-schedule.test")
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user.clone());
    let site = ready_site(&account, "schedule").await;

    // ---- nothing scheduled yet ---------------------------------------------
    assert!(
        account
            .site_publish_schedule(&site)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        account
            .site_publish_schedules(&site, 50)
            .await
            .unwrap()
            .is_empty()
    );

    // ---- schedule it -------------------------------------------------------
    let monday = OffsetDateTime::now_utc() + Duration::hours(48);
    let scheduled = account.schedule_site_publish(&site, monday).await.unwrap();
    assert_eq!(scheduled.status, SitePublishScheduleStatus::Scheduled);
    assert_eq!(scheduled.requested_by, user);
    assert_eq!(scheduled.attempts, 0);
    assert!(scheduled.publish.is_none());
    assert!(scheduled.last_error.is_none());
    assert!((scheduled.publish_at - monday).abs() < Duration::seconds(1));
    // Scheduling does not publish: the site is still a draft, with no version.
    assert_eq!(
        account.site(&site).await.unwrap().unwrap().status,
        SiteStatus::Draft
    );
    assert!(account.current_site_publish(&site).await.unwrap().is_none());

    // ---- moving it keeps ONE intention, with the same identity -------------
    let tuesday = monday + Duration::hours(24);
    let moved = account.schedule_site_publish(&site, tuesday).await.unwrap();
    assert_eq!(moved.id, scheduled.id, "rescheduling moves the same row");
    assert!((moved.publish_at - tuesday).abs() < Duration::seconds(1));
    assert_eq!(
        account
            .site_publish_schedules(&site, 50)
            .await
            .unwrap()
            .len(),
        1
    );
    let pending = account
        .site_publish_schedule(&site)
        .await
        .unwrap()
        .expect("a pending schedule");
    assert_eq!(pending.id, scheduled.id);

    // ---- a future schedule is not due --------------------------------------
    assert!(
        store
            .claim_due_site_publishes(10)
            .await
            .unwrap()
            .iter()
            .all(|row| row.schedule != scheduled.id)
    );

    // ---- its moment arrives -------------------------------------------------
    set_publish_at(&pool, &scheduled.id, "now() - INTERVAL '1 minute'").await;
    let due = store.claim_due_site_publishes(10).await.unwrap();
    let claimed = due
        .iter()
        .find(|row| row.schedule == scheduled.id)
        .expect("the due schedule was claimed");
    assert_eq!(claimed.tenant, tenant);
    assert_eq!(claimed.requested_by, user, "publishes as who scheduled it");
    assert_eq!(claimed.site, site);
    assert_eq!(claimed.attempts, 1);
    // A second sweep in the same window finds nothing: the row is claimed.
    assert!(
        store
            .claim_due_site_publishes(10)
            .await
            .unwrap()
            .iter()
            .all(|row| row.schedule != scheduled.id)
    );
    let running = account
        .site_publish_schedule(&site)
        .await
        .unwrap()
        .expect("still pending while it runs");
    assert_eq!(running.status, SitePublishScheduleStatus::Publishing);
    assert!(running.claimed_at.is_some());

    // ---- the worker publishes through the scheduling user's door -----------
    let worker_door = store.for_account(claimed.tenant.clone(), claimed.requested_by.clone());
    let publish = worker_door.publish_site(&claimed.site).await.unwrap();
    store
        .finish_site_publish_schedule(&claimed.tenant, &claimed.schedule, &publish)
        .await
        .unwrap();

    let done = account.site_publish_schedules(&site, 50).await.unwrap();
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].status, SitePublishScheduleStatus::Published);
    assert_eq!(done[0].publish.as_ref(), Some(&publish));
    assert!(done[0].finished_at.is_some());
    // Terminal rows stop being "pending", so the site can be scheduled again.
    assert!(
        account
            .site_publish_schedule(&site)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        account.site(&site).await.unwrap().unwrap().status,
        SiteStatus::Live
    );
    assert_eq!(
        account
            .current_site_publish(&site)
            .await
            .unwrap()
            .unwrap()
            .id,
        publish
    );
    // The version records the scheduling user as its author.
    assert_eq!(
        account
            .current_site_publish(&site)
            .await
            .unwrap()
            .unwrap()
            .published_by,
        user.as_str()
    );

    // ---- a finished schedule cannot be finished or failed again ------------
    assert_not_found(
        store
            .finish_site_publish_schedule(&tenant, &scheduled.id, &publish)
            .await,
    );
    assert_not_found(
        store
            .fail_site_publish_schedule(&tenant, &scheduled.id, "too late")
            .await,
    );
    assert_conflict(
        account
            .cancel_site_publish_schedule(&site, &scheduled.id)
            .await,
    );

    // Leave the shared test database as it was found: the sweep is
    // cross-tenant, so a leftover pending row would follow later runs around.
    account.delete_site(&site).await.unwrap();
}

/// Cancelling, the refusals around it, and what a publish that refuses leaves
/// behind for the tenant to read.
#[tokio::test]
async fn cancelling_is_visible_and_a_refused_publish_says_why() {
    let _sweeping = sweeping().await;
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let tenant = store.create_tenant("site-schedule-cancel").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("owner@site-cancel.test")
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);
    let site = ready_site(&account, "cancel").await;
    let at = OffsetDateTime::now_utc() + Duration::hours(6);

    // ---- the input rules ---------------------------------------------------
    let past = assert_validation(
        account
            .schedule_site_publish(&site, OffsetDateTime::now_utc() - Duration::minutes(1))
            .await,
    );
    assert!(past.contains("future"), "{past}");
    let far = assert_validation(
        account
            .schedule_site_publish(&site, OffsetDateTime::now_utc() + Duration::days(400))
            .await,
    );
    assert!(far.contains("365"), "{far}");
    assert_not_found(account.schedule_site_publish(&SiteId::generate(), at).await);

    // ---- cancel ------------------------------------------------------------
    let schedule = account.schedule_site_publish(&site, at).await.unwrap();
    let cancelled = account
        .cancel_site_publish_schedule(&site, &schedule.id)
        .await
        .unwrap();
    assert_eq!(cancelled.status, SitePublishScheduleStatus::Cancelled);
    assert!(cancelled.finished_at.is_some());
    // The record survives, and nothing is pending any more.
    assert!(
        account
            .site_publish_schedule(&site)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        account
            .site_publish_schedules(&site, 50)
            .await
            .unwrap()
            .len(),
        1
    );
    // A cancelled schedule never becomes due.
    set_publish_at(&pool, &schedule.id, "now() - INTERVAL '1 minute'").await;
    assert!(
        store
            .claim_due_site_publishes(10)
            .await
            .unwrap()
            .iter()
            .all(|row| row.schedule != schedule.id)
    );
    // Cancelling twice, and cancelling something that never existed.
    assert_conflict(
        account
            .cancel_site_publish_schedule(&site, &schedule.id)
            .await,
    );
    assert_not_found(
        account
            .cancel_site_publish_schedule(&site, &SitePublishScheduleId::generate())
            .await,
    );

    // ---- a publish that refuses is terminal, with the reason on the row ----
    let empty = account
        .create_site("Empty", &subdomain("cancel-empty"))
        .await
        .unwrap();
    let doomed = account
        .schedule_site_publish(&empty, OffsetDateTime::now_utc() + Duration::hours(1))
        .await
        .unwrap();
    set_publish_at(&pool, &doomed.id, "now() - INTERVAL '1 minute'").await;
    let claimed = store
        .claim_due_site_publishes(10)
        .await
        .unwrap()
        .into_iter()
        .find(|row| row.schedule == doomed.id)
        .expect("claimed");
    let refusal = assert_conflict(
        store
            .for_account(claimed.tenant.clone(), claimed.requested_by.clone())
            .publish_site(&claimed.site)
            .await,
    );
    store
        .fail_site_publish_schedule(&claimed.tenant, &claimed.schedule, &refusal)
        .await
        .unwrap();
    let failed = account.site_publish_schedules(&empty, 50).await.unwrap();
    assert_eq!(failed[0].status, SitePublishScheduleStatus::Failed);
    assert_eq!(failed[0].last_error.as_deref(), Some(refusal.as_str()));
    // Terminal: the sweeper never picks it up again.
    assert!(
        store
            .claim_due_site_publishes(10)
            .await
            .unwrap()
            .iter()
            .all(|row| row.schedule != doomed.id)
    );
    // And the site stayed a draft — a refused publish leaves no version.
    assert_eq!(
        account.site(&empty).await.unwrap().unwrap().status,
        SiteStatus::Draft
    );

    // ---- an over-long reason is stored bounded -----------------------------
    let long = account
        .schedule_site_publish(&empty, OffsetDateTime::now_utc() + Duration::hours(2))
        .await
        .unwrap();
    set_publish_at(&pool, &long.id, "now() - INTERVAL '1 minute'").await;
    store.claim_due_site_publishes(10).await.unwrap();
    store
        .fail_site_publish_schedule(&tenant, &long.id, &"é".repeat(2_000))
        .await
        .unwrap();
    let stored = account.site_publish_schedules(&empty, 50).await.unwrap();
    let stored = stored
        .iter()
        .find(|row| row.id == long.id)
        .expect("the failed row");
    assert_eq!(
        stored
            .last_error
            .as_deref()
            .map(|value| value.chars().count()),
        Some(alo_store::SITE_PUBLISH_SCHEDULE_ERROR_MAX_CHARS)
    );

    account.delete_site(&site).await.unwrap();
    account.delete_site(&empty).await.unwrap();
}

/// Two sweepers racing over the same due rows, and a worker that dies
/// mid-publish: every schedule is handed out exactly once, an abandoned claim
/// is retried a bounded number of times, and then fails where the tenant can
/// see it.
#[tokio::test]
async fn concurrent_sweepers_claim_each_schedule_once_and_a_dead_worker_gives_up() {
    let _sweeping = sweeping().await;
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let tenant = store.create_tenant("site-schedule-race").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("owner@site-race.test")
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);

    let mut sites = Vec::new();
    let mut schedules = Vec::new();
    for index in 0..4 {
        let site = ready_site(&account, &format!("race{index}")).await;
        let schedule = account
            .schedule_site_publish(&site, OffsetDateTime::now_utc() + Duration::hours(1))
            .await
            .unwrap();
        set_publish_at(&pool, &schedule.id, "now() - INTERVAL '1 minute'").await;
        sites.push(site);
        schedules.push(schedule.id);
    }

    // ---- two sweeps at the same instant ------------------------------------
    let (left, right) = tokio::join!(
        store.claim_due_site_publishes(10),
        store.claim_due_site_publishes(10)
    );
    let mut claimed: Vec<SitePublishScheduleId> = left
        .unwrap()
        .into_iter()
        .chain(right.unwrap())
        .map(|row| row.schedule)
        .filter(|id| schedules.contains(id))
        .collect();
    let total = claimed.len();
    claimed.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    claimed.dedup_by(|a, b| a.as_str() == b.as_str());
    assert_eq!(claimed.len(), 4, "every schedule was claimed");
    assert_eq!(total, 4, "and none of them twice");

    // ---- one worker dies: its row is re-offered once its claim goes stale --
    let abandoned = schedules[0].clone();
    assert!(
        store
            .claim_due_site_publishes(10)
            .await
            .unwrap()
            .iter()
            .all(|row| row.schedule != abandoned),
        "a fresh claim is not re-offered"
    );
    age_claim(&pool, &abandoned, "now() - INTERVAL '30 minutes'").await;
    let again = store.claim_due_site_publishes(10).await.unwrap();
    let retried = again
        .iter()
        .find(|row| row.schedule == abandoned)
        .expect("the abandoned schedule came back");
    assert_eq!(retried.attempts, 2);

    // ---- and it gives up rather than retrying forever ----------------------
    age_claim(&pool, &abandoned, "now() - INTERVAL '30 minutes'").await;
    let third = store.claim_due_site_publishes(10).await.unwrap();
    assert_eq!(
        third
            .iter()
            .find(|row| row.schedule == abandoned)
            .expect("third attempt")
            .attempts,
        3
    );
    age_claim(&pool, &abandoned, "now() - INTERVAL '30 minutes'").await;
    let fourth = store.claim_due_site_publishes(10).await.unwrap();
    assert!(
        fourth.iter().all(|row| row.schedule != abandoned),
        "the attempt budget is spent"
    );
    let written_off = account.site_publish_schedules(&sites[0], 50).await.unwrap();
    assert_eq!(written_off[0].status, SitePublishScheduleStatus::Failed);
    assert_eq!(
        written_off[0].last_error.as_deref(),
        Some(alo_store::SITE_PUBLISH_INTERRUPTED)
    );
    // Written off, so the site can be scheduled again.
    assert!(
        account
            .site_publish_schedule(&sites[0])
            .await
            .unwrap()
            .is_none()
    );

    for site in &sites {
        account.delete_site(site).await.unwrap();
    }
}

/// Another tenant's scheduled publish is unreadable, unmovable, uncancellable,
/// and cannot be completed or failed on their behalf — and every refusal looks
/// exactly like a schedule that never existed.
#[tokio::test]
async fn another_tenants_schedule_is_invisible_and_untouchable() {
    let _sweeping = sweeping().await;
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let owner_tenant = store.create_tenant("site-schedule-a").await.unwrap();
    let owner_user = store
        .for_tenant(owner_tenant.clone())
        .create_user("owner@schedule-a.test")
        .await
        .unwrap();
    let owner = store.for_account(owner_tenant.clone(), owner_user);
    let site = ready_site(&owner, "tenant-a").await;
    let schedule = owner
        .schedule_site_publish(&site, OffsetDateTime::now_utc() + Duration::hours(3))
        .await
        .unwrap();

    let other_tenant = store.create_tenant("site-schedule-b").await.unwrap();
    let other_user = store
        .for_tenant(other_tenant.clone())
        .create_user("intruder@schedule-b.test")
        .await
        .unwrap();
    let intruder = store.for_account(other_tenant.clone(), other_user);
    let intruder_site = ready_site(&intruder, "tenant-b").await;

    // ---- reading -----------------------------------------------------------
    assert!(
        intruder
            .site_publish_schedule(&site)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        intruder
            .site_publish_schedules(&site, 50)
            .await
            .unwrap()
            .is_empty()
    );

    // ---- moving and cancelling ---------------------------------------------
    assert_not_found(
        intruder
            .schedule_site_publish(&site, OffsetDateTime::now_utc() + Duration::hours(9))
            .await,
    );
    assert_not_found(
        intruder
            .cancel_site_publish_schedule(&site, &schedule.id)
            .await,
    );
    // Even naming the foreign schedule under a site the intruder does own.
    assert_not_found(
        intruder
            .cancel_site_publish_schedule(&intruder_site, &schedule.id)
            .await,
    );

    // ---- completing on someone else's behalf -------------------------------
    set_publish_at(&pool, &schedule.id, "now() - INTERVAL '1 minute'").await;
    let claimed = store
        .claim_due_site_publishes(10)
        .await
        .unwrap()
        .into_iter()
        .find(|row| row.schedule == schedule.id)
        .expect("claimed");
    assert_eq!(claimed.tenant, owner_tenant);
    let publish = store
        .for_account(claimed.tenant.clone(), claimed.requested_by.clone())
        .publish_site(&claimed.site)
        .await
        .unwrap();
    assert_not_found(
        store
            .finish_site_publish_schedule(&other_tenant, &schedule.id, &publish)
            .await,
    );
    assert_not_found(
        store
            .fail_site_publish_schedule(&other_tenant, &schedule.id, "not yours")
            .await,
    );
    // A version of another site cannot be pinned onto this schedule either.
    let foreign_site = ready_site(&intruder, "tenant-b-live").await;
    let foreign_publish = intruder.publish_site(&foreign_site).await.unwrap();
    assert_not_found(
        store
            .finish_site_publish_schedule(&owner_tenant, &schedule.id, &foreign_publish)
            .await,
    );
    assert_not_found(
        store
            .finish_site_publish_schedule(&owner_tenant, &schedule.id, &SitePublishId::generate())
            .await,
    );

    // The owner's schedule is untouched by any of it, and still claimable.
    let owner_view = owner.site_publish_schedule(&site).await.unwrap().unwrap();
    assert_eq!(owner_view.status, SitePublishScheduleStatus::Publishing);
    assert!(owner_view.last_error.is_none());
    store
        .finish_site_publish_schedule(&owner_tenant, &schedule.id, &publish)
        .await
        .unwrap();

    // ---- deleting the site takes its schedules with it ---------------------
    owner.delete_site(&site).await.unwrap();
    assert!(
        owner
            .site_publish_schedules(&site, 50)
            .await
            .unwrap()
            .is_empty()
    );
}

/// A small pool of this test's own, for the two clock helpers below (the
/// store's pool is deliberately not public).
async fn raw_pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap()
}

/// Moves a schedule's moment with raw SQL — the store deliberately refuses a
/// publish time in the past, so a test that needs a *due* row builds it here
/// rather than by sleeping.
async fn set_publish_at(pool: &PgPool, schedule: &SitePublishScheduleId, when: &str) {
    sqlx::query(&format!(
        "UPDATE site_publish_schedules SET publish_at = {when} WHERE id = $1"
    ))
    .bind(schedule.as_str())
    .execute(pool)
    .await
    .unwrap();
}

/// Ages a claim so the sweeper treats the worker as dead.
async fn age_claim(pool: &PgPool, schedule: &SitePublishScheduleId, when: &str) {
    sqlx::query(&format!(
        "UPDATE site_publish_schedules SET claimed_at = {when} WHERE id = $1"
    ))
    .bind(schedule.as_str())
    .execute(pool)
    .await
    .unwrap();
}
