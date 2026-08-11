//! Publishing a website at a chosen moment (ADR 0036, S2.05a) — the model
//! behind "go live on Monday at 09:00" rather than "go live now".
//!
//! A row of `site_publish_schedules` is an **intention**, never a version. The
//! immutable record of what the internet served stays [`crate::site_publish`],
//! written by the ordinary publish path when a worker claims a due row and
//! runs it through the scheduling user's account door. Keeping the two apart
//! is what lets an intention be cancelled, moved, or fail without leaving a
//! version behind — and it means scheduled publishing adds no second way to
//! freeze a site, only a second moment to call the first one.
//!
//! Three properties this module is responsible for:
//!
//! - **One future per website.** A partial unique index admits a single
//!   `scheduled`/`publishing` row per site, and [`AccountStore::schedule_site_publish`]
//!   takes the site row's lock before writing, so two editors scheduling at
//!   the same instant produce one intention, not two publishes.
//! - **At-most-once under concurrent sweepers.** [`Store::claim_due_site_publishes`]
//!   marks rows `publishing` in the statement that reads them, with
//!   `FOR UPDATE SKIP LOCKED`, so a second sweeper walks past a claimed row
//!   instead of publishing the same site twice.
//! - **No silent stall.** A worker that dies mid-publish leaves a `publishing`
//!   row; the claim re-offers it once its claim has gone stale
//!   ([`SITE_PUBLISH_CLAIM_STALE_MINUTES`]) and gives up — visibly, as
//!   `failed` — after [`SITE_PUBLISH_MAX_ATTEMPTS`], rather than leaving the
//!   tenant watching a schedule that will never resolve.
//!
//! A publish failure itself (no home page, a collection that no longer
//! resolves) is **terminal**: it is a statement about the site's content that
//! retrying in ten minutes cannot change, so the reason is recorded verbatim
//! for the tenant to act on. Only an interrupted attempt is retried.

use time::{Duration, OffsetDateTime};

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{SiteId, SitePublishId, SitePublishScheduleId, TenantId, UserId};
use crate::store::Store;

/// How far ahead a publish may be scheduled. A year is generous for a
/// marketing site launch and keeps a typo (`2924-01-01`) from parking a row in
/// the sweeper's index forever.
pub const SITE_PUBLISH_SCHEDULE_MAX_AHEAD_DAYS: i64 = 365;

/// How long a claimed publish may stay unfinished before the sweeper assumes
/// the worker died and offers the row again.
pub const SITE_PUBLISH_CLAIM_STALE_MINUTES: i32 = 10;

/// How many times one scheduled publish may be claimed before it is declared
/// failed. Only interrupted attempts consume one: a publish that refuses is
/// terminal on the first try.
pub const SITE_PUBLISH_MAX_ATTEMPTS: i32 = 3;

/// Most schedule rows one history read returns.
pub const MAX_SITE_PUBLISH_SCHEDULE_HISTORY: i64 = 50;

/// Longest failure reason stored on a schedule row. The reason is a store
/// error message (a rule the site broke), never request content.
pub const SITE_PUBLISH_SCHEDULE_ERROR_MAX_CHARS: usize = 500;

/// The message a schedule carries when its worker never came back.
pub const SITE_PUBLISH_INTERRUPTED: &str =
    "publishing was interrupted and did not finish; schedule it again";

/// Where one scheduled publish is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SitePublishScheduleStatus {
    /// Waiting for its moment.
    Scheduled,
    /// Claimed by a worker and running right now.
    Publishing,
    /// It produced a version ([`SitePublishSchedule::publish`]).
    Published,
    /// The tenant called it off before it ran.
    Cancelled,
    /// It ran and refused, or was abandoned — see
    /// [`SitePublishSchedule::last_error`].
    Failed,
}

impl SitePublishScheduleStatus {
    /// The stable token this status is stored and named by on the wire.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scheduled => "scheduled",
            Self::Publishing => "publishing",
            Self::Published => "published",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    /// Whether the schedule is still going to happen — the two states the
    /// per-site uniqueness rule counts.
    #[must_use]
    pub fn is_pending(self) -> bool {
        matches!(self, Self::Scheduled | Self::Publishing)
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "scheduled" => Ok(Self::Scheduled),
            "publishing" => Ok(Self::Publishing),
            "published" => Ok(Self::Published),
            "cancelled" => Ok(Self::Cancelled),
            "failed" => Ok(Self::Failed),
            _ => Err(StoreError::Conflict(
                "scheduled publish has an unknown stored status".to_owned(),
            )),
        }
    }
}

/// One scheduled publish of one website, as the tenant sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitePublishSchedule {
    pub id: SitePublishScheduleId,
    pub site: SiteId,
    /// The moment the website should go live, in UTC. The surface that shows
    /// it is responsible for saying what that is in the reader's own time.
    pub publish_at: OffsetDateTime,
    pub status: SitePublishScheduleStatus,
    /// The user whose account door the publish runs through, and who the
    /// resulting version records as its author.
    pub requested_by: UserId,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    /// When a worker last picked this row up.
    pub claimed_at: Option<OffsetDateTime>,
    /// When it reached a terminal state.
    pub finished_at: Option<OffsetDateTime>,
    /// How many times a worker has claimed it.
    pub attempts: i32,
    /// The version it produced, once it produced one.
    pub publish: Option<SitePublishId>,
    /// Why it failed, in the words the tenant can act on.
    pub last_error: Option<String>,
}

/// A due scheduled publish, as the sweeper needs it: which site, whose
/// account door to publish through, and the row to report back against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueSitePublish {
    /// The tenant this publish belongs to — the only tenant it may touch.
    pub tenant: TenantId,
    /// The account door the publish runs through (the scheduling user).
    pub requested_by: UserId,
    pub site: SiteId,
    pub schedule: SitePublishScheduleId,
    pub publish_at: OffsetDateTime,
    /// Which attempt this is, starting at 1.
    pub attempts: i32,
}

impl AccountStore {
    /// Schedules `site` to be published at `at`, or moves an existing pending
    /// schedule to the new moment (the id is kept, so a surface watching one
    /// schedule keeps watching it). The site's row is locked first, so
    /// concurrent scheduling serializes into one intention rather than racing
    /// the per-site uniqueness rule into an error.
    ///
    /// Nothing about the site's *content* is checked here: a site with no home
    /// page yet may still be scheduled, because the author has until the
    /// chosen moment to finish it. What the site cannot do is fail silently —
    /// a refusal at publish time is recorded on the row.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site isn't the tenant's;
    /// [`StoreError::Validation`] when `at` is in the past or further than
    /// [`SITE_PUBLISH_SCHEDULE_MAX_AHEAD_DAYS`] ahead;
    /// [`StoreError::Conflict`] when the site is being published right now;
    /// [`StoreError::Db`].
    pub async fn schedule_site_publish(
        &self,
        site: &SiteId,
        at: OffsetDateTime,
    ) -> Result<SitePublishSchedule> {
        let now = OffsetDateTime::now_utc();
        if at <= now {
            return Err(StoreError::Validation(
                "a scheduled publish must be in the future".to_owned(),
            ));
        }
        if at > now + Duration::days(SITE_PUBLISH_SCHEDULE_MAX_AHEAD_DAYS) {
            return Err(StoreError::Validation(format!(
                "a publish can be scheduled at most {SITE_PUBLISH_SCHEDULE_MAX_AHEAD_DAYS} days ahead"
            )));
        }
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // Existence check and serialization point in one: two schedulers meet
        // here, so the second sees the first one's row instead of colliding on
        // the partial unique index.
        let owned: Option<String> =
            sqlx::query_scalar("SELECT id FROM sites WHERE tenant_id = $1 AND id = $2 FOR UPDATE")
                .bind(self.tenant.as_str())
                .bind(site.as_str())
                .fetch_optional(&mut *tx)
                .await
                .map_err(StoreError::Db)?;
        if owned.is_none() {
            return Err(StoreError::NotFound);
        }
        let pending: Option<(String, String)> = sqlx::query_as(
            "SELECT id, status FROM site_publish_schedules \
             WHERE tenant_id = $1 AND site_id = $2 AND status IN ('scheduled', 'publishing')",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let id = match pending {
            Some((_, status)) if status == SitePublishScheduleStatus::Publishing.as_str() => {
                return Err(StoreError::Conflict(
                    "this website is being published right now".to_owned(),
                ));
            }
            Some((id, _)) => {
                sqlx::query(
                    "UPDATE site_publish_schedules \
                        SET publish_at = $3, requested_by = $4, last_error = NULL, \
                            updated_at = now() \
                      WHERE tenant_id = $1 AND id = $2",
                )
                .bind(self.tenant.as_str())
                .bind(&id)
                .bind(at)
                .bind(self.user.as_str())
                .execute(&mut *tx)
                .await
                .map_err(StoreError::Db)?;
                SitePublishScheduleId::new(id)
            }
            None => {
                let id = SitePublishScheduleId::generate();
                sqlx::query(
                    "INSERT INTO site_publish_schedules \
                         (tenant_id, id, site_id, publish_at, status, requested_by) \
                     VALUES ($1, $2, $3, $4, $5, $6)",
                )
                .bind(self.tenant.as_str())
                .bind(id.as_str())
                .bind(site.as_str())
                .bind(at)
                .bind(SitePublishScheduleStatus::Scheduled.as_str())
                .bind(self.user.as_str())
                .execute(&mut *tx)
                .await
                .map_err(StoreError::Db)?;
                id
            }
        };
        let row = sqlx::query_as::<_, SitePublishScheduleRow>(&format!(
            "SELECT {SCHEDULE_COLUMNS} FROM site_publish_schedules WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        row.into_schedule()
    }

    /// The site's pending scheduled publish, if it has one. A site of another
    /// tenant reads as `None`, exactly as a site with nothing scheduled does.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure; [`StoreError::Conflict`] if a stored
    /// status is unreadable.
    pub async fn site_publish_schedule(
        &self,
        site: &SiteId,
    ) -> Result<Option<SitePublishSchedule>> {
        let row = sqlx::query_as::<_, SitePublishScheduleRow>(&format!(
            "SELECT {SCHEDULE_COLUMNS} FROM site_publish_schedules \
             WHERE tenant_id = $1 AND site_id = $2 AND status IN ('scheduled', 'publishing')"
        ))
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(SitePublishScheduleRow::into_schedule).transpose()
    }

    /// Everything ever scheduled for the site, the next (or most recent)
    /// moment first, capped at `limit` rows (clamped to
    /// [`MAX_SITE_PUBLISH_SCHEDULE_HISTORY`]). Another tenant's site reads as
    /// an empty list, by design.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure; [`StoreError::Conflict`] if a stored
    /// status is unreadable.
    pub async fn site_publish_schedules(
        &self,
        site: &SiteId,
        limit: i64,
    ) -> Result<Vec<SitePublishSchedule>> {
        let limit = limit.clamp(1, MAX_SITE_PUBLISH_SCHEDULE_HISTORY);
        let rows = sqlx::query_as::<_, SitePublishScheduleRow>(&format!(
            "SELECT {SCHEDULE_COLUMNS} FROM site_publish_schedules \
             WHERE tenant_id = $1 AND site_id = $2 ORDER BY publish_at DESC, id DESC LIMIT $3"
        ))
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter()
            .map(SitePublishScheduleRow::into_schedule)
            .collect()
    }

    /// Calls off a scheduled publish. The row is kept as `cancelled` — the
    /// tenant asked for something and then changed their mind, and a surface
    /// that shows "you cancelled this" is kinder than one where the entry
    /// simply disappears.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the schedule isn't this tenant's site's,
    /// or doesn't exist; [`StoreError::Conflict`] when it has already run,
    /// already been cancelled, or is running right now (a publish in flight
    /// cannot be recalled — the version it makes can be rolled back);
    /// [`StoreError::Db`].
    pub async fn cancel_site_publish_schedule(
        &self,
        site: &SiteId,
        schedule: &SitePublishScheduleId,
    ) -> Result<SitePublishSchedule> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let current: Option<String> = sqlx::query_scalar(
            "SELECT status FROM site_publish_schedules \
             WHERE tenant_id = $1 AND site_id = $2 AND id = $3 FOR UPDATE",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(schedule.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let current = SitePublishScheduleStatus::parse(&current.ok_or(StoreError::NotFound)?)?;
        match current {
            SitePublishScheduleStatus::Scheduled => {}
            SitePublishScheduleStatus::Publishing => {
                return Err(StoreError::Conflict(
                    "this website is being published right now".to_owned(),
                ));
            }
            _ => {
                return Err(StoreError::Conflict(
                    "this scheduled publish has already finished".to_owned(),
                ));
            }
        }
        let row = sqlx::query_as::<_, SitePublishScheduleRow>(&format!(
            "UPDATE site_publish_schedules \
                SET status = $4, finished_at = now(), updated_at = now() \
              WHERE tenant_id = $1 AND site_id = $2 AND id = $3 \
          RETURNING {SCHEDULE_COLUMNS}"
        ))
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(schedule.as_str())
        .bind(SitePublishScheduleStatus::Cancelled.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        row.into_schedule()
    }
}

impl Store {
    /// Claims up to `limit` scheduled publishes that are due, oldest moment
    /// first, marking each `publishing` in the statement that reads it.
    /// Concurrent sweepers skip each other's locked rows
    /// (`FOR UPDATE SKIP LOCKED`) instead of publishing one site twice.
    ///
    /// The same call first writes off rows whose worker never came back and
    /// which have used up [`SITE_PUBLISH_MAX_ATTEMPTS`], so an interrupted
    /// publish ends as a visible `failed` rather than a schedule that stays
    /// "publishing" forever.
    ///
    /// System-level by design: the sweep spans tenants, and every returned row
    /// carries the tenant and the account door the publish must run through.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn claim_due_site_publishes(&self, limit: i64) -> Result<Vec<DueSitePublish>> {
        let mut tx = self.pool().begin().await.map_err(StoreError::Db)?;
        sqlx::query(
            "UPDATE site_publish_schedules \
                SET status = 'failed', last_error = $1, finished_at = now(), updated_at = now() \
              WHERE status = 'publishing' \
                AND claimed_at < now() - make_interval(mins => $2) \
                AND attempts >= $3",
        )
        .bind(SITE_PUBLISH_INTERRUPTED)
        .bind(SITE_PUBLISH_CLAIM_STALE_MINUTES)
        .bind(SITE_PUBLISH_MAX_ATTEMPTS)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let rows = sqlx::query_as::<_, DueRow>(
            "UPDATE site_publish_schedules s \
                SET status = 'publishing', claimed_at = now(), \
                    attempts = s.attempts + 1, updated_at = now() \
              WHERE (s.tenant_id, s.id) IN ( \
                    SELECT tenant_id, id FROM site_publish_schedules \
                     WHERE (status = 'scheduled' AND publish_at <= now()) \
                        OR (status = 'publishing' \
                            AND claimed_at < now() - make_interval(mins => $2) \
                            AND attempts < $3) \
                     ORDER BY publish_at, id \
                     LIMIT $1 \
                     FOR UPDATE SKIP LOCKED) \
          RETURNING s.tenant_id, s.requested_by, s.site_id, s.id, s.publish_at, s.attempts",
        )
        .bind(limit)
        .bind(SITE_PUBLISH_CLAIM_STALE_MINUTES)
        .bind(SITE_PUBLISH_MAX_ATTEMPTS)
        .fetch_all(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(DueRow::into_due).collect())
    }

    /// Records that a claimed schedule produced `publish`. The version must be
    /// one of the same tenant's, of the same site — anything else is
    /// [`StoreError::NotFound`], the same answer an unknown schedule gets.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the schedule isn't claimed, isn't that
    /// tenant's, or the version doesn't belong to its site;
    /// [`StoreError::Db`].
    pub async fn finish_site_publish_schedule(
        &self,
        tenant: &TenantId,
        schedule: &SitePublishScheduleId,
        publish: &SitePublishId,
    ) -> Result<()> {
        let done = sqlx::query(
            "UPDATE site_publish_schedules s \
                SET status = 'published', publish_id = $3, last_error = NULL, \
                    finished_at = now(), updated_at = now() \
              WHERE s.tenant_id = $1 AND s.id = $2 AND s.status = 'publishing' \
                AND EXISTS (SELECT 1 FROM site_publishes p \
                             WHERE p.tenant_id = s.tenant_id AND p.site_id = s.site_id \
                               AND p.id = $3)",
        )
        .bind(tenant.as_str())
        .bind(schedule.as_str())
        .bind(publish.as_str())
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Records that a claimed schedule refused, with the reason the tenant
    /// needs in order to fix it (truncated to
    /// [`SITE_PUBLISH_SCHEDULE_ERROR_MAX_CHARS`]). Terminal: a site that
    /// cannot be published now will not become publishable by being retried
    /// in ten minutes, so the tenant edits and schedules again.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the schedule isn't claimed or isn't that
    /// tenant's; [`StoreError::Db`].
    pub async fn fail_site_publish_schedule(
        &self,
        tenant: &TenantId,
        schedule: &SitePublishScheduleId,
        reason: &str,
    ) -> Result<()> {
        let reason: String = reason
            .chars()
            .take(SITE_PUBLISH_SCHEDULE_ERROR_MAX_CHARS)
            .collect();
        let done = sqlx::query(
            "UPDATE site_publish_schedules \
                SET status = 'failed', last_error = $3, finished_at = now(), updated_at = now() \
              WHERE tenant_id = $1 AND id = $2 AND status = 'publishing'",
        )
        .bind(tenant.as_str())
        .bind(schedule.as_str())
        .bind(&reason)
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}

/// The columns every schedule read returns — the one list, shared by the
/// `SELECT`s and by the `RETURNING` of the write that answers with a row.
const SCHEDULE_COLUMNS: &str = "id, site_id, publish_at, status, requested_by, created_at, \
     updated_at, claimed_at, finished_at, attempts, publish_id, last_error";

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct SitePublishScheduleRow {
    id: String,
    site_id: String,
    publish_at: OffsetDateTime,
    status: String,
    requested_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    claimed_at: Option<OffsetDateTime>,
    finished_at: Option<OffsetDateTime>,
    attempts: i32,
    publish_id: Option<String>,
    last_error: Option<String>,
}

impl SitePublishScheduleRow {
    fn into_schedule(self) -> Result<SitePublishSchedule> {
        Ok(SitePublishSchedule {
            id: SitePublishScheduleId::new(self.id),
            site: SiteId::new(self.site_id),
            publish_at: self.publish_at,
            status: SitePublishScheduleStatus::parse(&self.status)?,
            requested_by: UserId::new(self.requested_by),
            created_at: self.created_at,
            updated_at: self.updated_at,
            claimed_at: self.claimed_at,
            finished_at: self.finished_at,
            attempts: self.attempts,
            publish: self.publish_id.map(SitePublishId::new),
            last_error: self.last_error,
        })
    }
}

#[derive(sqlx::FromRow)]
struct DueRow {
    tenant_id: String,
    requested_by: String,
    site_id: String,
    id: String,
    publish_at: OffsetDateTime,
    attempts: i32,
}

impl DueRow {
    fn into_due(self) -> DueSitePublish {
        DueSitePublish {
            tenant: TenantId::new(self.tenant_id),
            requested_by: UserId::new(self.requested_by),
            site: SiteId::new(self.site_id),
            schedule: SitePublishScheduleId::new(self.id),
            publish_at: self.publish_at,
            attempts: self.attempts,
        }
    }
}
