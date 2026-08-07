//! Insights dashboards — the boards a tenant reads its numbers from
//! (ADR 0037, wave BI-1), reached through the account door like every other
//! business record.
//!
//! A dashboard is **tenant-wide** in BI-1: every member of a tenant sees every
//! board. Spaces-scoped sharing is wanted and real, but it is the same
//! cross-cutting role question B4.12 owns, where the accountant is the first
//! scoped role; half-deciding it from its narrowest caller is how a permission
//! model gets settled by accident (`docs/design/insights.md`, "Tenancy").
//!
//! A board holds no numbers — its tiles ([`crate::insight_tiles`]) hold
//! *questions*, and the answers are evaluated from the documents every time.
//!
//! One board can be **seeded** by us: the zero-setup Business overview
//! (BI1.06) is written with [`BUSINESS_OVERVIEW_KEY`], and the partial unique
//! index on `(tenant_id, system_key)` is what makes that seed idempotent and
//! race-free without a lock. From the moment it exists it is an ordinary
//! board — renamable, with tiles addable and removable — because a dashboard
//! nobody can edit is a second kind of dashboard, and the first request would
//! be to change one tile on it. That a seed *ran* is recorded separately, in
//! the ledger [`crate::insight_overview`] keeps, so a tenant that throws the
//! overview away is not handed a new one the next morning.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::billing_field::required;
use crate::error::{Result, StoreError};
use crate::id::InsightDashboardId;

/// A board name is a tab, not a sentence.
pub const DASHBOARD_NAME_MAX_CHARS: usize = 120;

/// Most boards one tenant may hold. A guard against a runaway client or an
/// enthusiastic agent, not a licensing limit — thirty tabs is already more
/// than anybody reads.
pub const DASHBOARDS_PER_TENANT_MAX: i64 = 30;

/// The `system_key` of the zero-setup Business overview (BI1.06). The only
/// seeded board BI-1 mints.
pub const BUSINESS_OVERVIEW_KEY: &str = "business_overview";

/// Longest `system_key` we accept — ours are short dotted words.
const SYSTEM_KEY_MAX_CHARS: usize = 64;

/// The columns every read of a dashboard selects, in `DashboardRow` order.
const DASHBOARD_COLS: &str = "id, name, system_key, created_by, created_at, updated_at";

/// The writable shape of a dashboard. Used for create and rename alike (a
/// rename is a full replace of the writable fields — the route layer merges a
/// partial `PATCH` onto the stored record before calling).
#[derive(Debug, Clone, Default)]
pub struct NewDashboard {
    /// The board's label. Required, non-blank.
    pub name: String,
}

/// A stored dashboard.
#[derive(Debug, Clone)]
pub struct Dashboard {
    /// Opaque id, unique within the tenant.
    pub id: InsightDashboardId,
    /// The board's label.
    pub name: String,
    /// `Some` when we seeded the board (see [`BUSINESS_OVERVIEW_KEY`]); `None`
    /// for a board a user made. It marks *where the board came from* and
    /// grants it no special behaviour.
    pub system_key: Option<String>,
    /// The user who created the record — for a seeded board, whoever opened
    /// Insights first.
    pub created_by: String,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Last modification time.
    pub updated_at: OffsetDateTime,
}

impl Dashboard {
    /// Whether we seeded this board rather than a user creating it.
    pub fn is_seeded(&self) -> bool {
        self.system_key.is_some()
    }
}

/// Validates and normalises a board's writable fields. Pure — no database, so
/// the rules are unit-tested directly.
pub(crate) fn normalize(input: &NewDashboard) -> Result<String> {
    required("name", &input.name, DASHBOARD_NAME_MAX_CHARS)
}

/// Checks a `system_key`. It is *our* input, never a caller's, so a bad one is
/// a bug — and a bug that writes an unreachable seed marker is worse than one
/// that refuses to write at all.
pub(crate) fn normalize_key(key: &str) -> Result<String> {
    let key = key.trim();
    if key.is_empty() || key.chars().count() > SYSTEM_KEY_MAX_CHARS {
        return Err(StoreError::Validation(format!(
            "system key must be 1 to {SYSTEM_KEY_MAX_CHARS} characters"
        )));
    }
    if !key
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return Err(StoreError::Validation(
            "system key must be lowercase letters, digits and underscores".to_owned(),
        ));
    }
    Ok(key.to_owned())
}

/// Turns the seeded-board uniqueness violation into the conflict the seed
/// reads as "somebody else got there first".
pub(crate) fn map_key_conflict(error: sqlx::Error) -> StoreError {
    match error {
        sqlx::Error::Database(ref db) if db.code().as_deref() == Some("23505") => {
            StoreError::Conflict("this tenant already has that seeded dashboard".to_owned())
        }
        other => StoreError::Db(other),
    }
}

/// Writes one board inside `tx`. The single insert both the public create and
/// the Business overview seed ([`crate::insight_overview`]) go through, so a
/// seeded board and a typed one are the same row.
///
/// # Errors
/// [`StoreError::Conflict`] when the tenant already holds a board with this
/// `system_key` — which is what makes the seed idempotent without a lock;
/// [`StoreError::Db`] on failure.
pub(crate) async fn insert_dashboard(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    id: &InsightDashboardId,
    name: &str,
    system_key: Option<&str>,
    created_by: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO insight_dashboards (tenant_id, id, name, system_key, created_by) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(tenant)
    .bind(id.as_str())
    .bind(name)
    .bind(system_key)
    .bind(created_by)
    .execute(&mut **tx)
    .await
    .map_err(map_key_conflict)?;
    Ok(())
}

impl AccountStore {
    /// Creates a board a user made — no `system_key`, no tiles yet.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a blank or over-long name, or when the
    /// tenant already holds [`DASHBOARDS_PER_TENANT_MAX`] boards;
    /// [`StoreError::Db`] on failure.
    pub async fn create_insight_dashboard(
        &self,
        input: &NewDashboard,
    ) -> Result<InsightDashboardId> {
        self.insert_insight_dashboard(&normalize(input)?, None)
            .await
    }

    /// Creates a board **we** seeded, marked with `system_key` so it is
    /// written exactly once per tenant. The caller for BI-1 is the Business
    /// overview seed (BI1.06); the name arrives already translated, like a
    /// CRM pipeline seed's stage names.
    ///
    /// # Errors
    /// [`StoreError::Validation`] as for [`Self::create_insight_dashboard`],
    /// or on a malformed key; [`StoreError::Conflict`] when the tenant already
    /// has a board with this key — which is exactly what a concurrent first
    /// visit should get, and what makes the seed race-free;
    /// [`StoreError::Db`] on failure.
    pub async fn create_seeded_insight_dashboard(
        &self,
        input: &NewDashboard,
        system_key: &str,
    ) -> Result<InsightDashboardId> {
        let name = normalize(input)?;
        let key = normalize_key(system_key)?;
        self.insert_insight_dashboard(&name, Some(&key)).await
    }

    /// The one insert both creates go through, cap check included.
    ///
    /// The cap is counted and enforced inside one transaction. Under READ
    /// COMMITTED two simultaneous creates could both see room and land the
    /// tenant on 31 boards; that is accepted deliberately — the cap is a
    /// runaway guard, not an invariant anything reads, and paying for a table
    /// lock on every create to make it exact would be the wrong trade.
    async fn insert_insight_dashboard(
        &self,
        name: &str,
        system_key: Option<&str>,
    ) -> Result<InsightDashboardId> {
        let id = InsightDashboardId::generate();
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let held: i64 =
            sqlx::query_scalar("SELECT count(*) FROM insight_dashboards WHERE tenant_id = $1")
                .bind(self.tenant.as_str())
                .fetch_one(&mut *tx)
                .await
                .map_err(StoreError::Db)?;
        if held >= DASHBOARDS_PER_TENANT_MAX {
            return Err(StoreError::Validation(format!(
                "a tenant may hold at most {DASHBOARDS_PER_TENANT_MAX} dashboards"
            )));
        }
        insert_dashboard(
            &mut tx,
            self.tenant.as_str(),
            &id,
            name,
            system_key,
            self.user.as_str(),
        )
        .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// The tenant's boards, oldest first — so the seeded overview, which is
    /// the first board a tenant ever has, stays the first tab.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn insight_dashboards(&self) -> Result<Vec<Dashboard>> {
        let rows = sqlx::query_as::<_, DashboardRow>(&format!(
            "SELECT {DASHBOARD_COLS} FROM insight_dashboards \
             WHERE tenant_id = $1 ORDER BY created_at, id"
        ))
        .bind(self.tenant.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(DashboardRow::into_dashboard).collect())
    }

    /// One board of the tenant, or `None` — including when the id belongs to
    /// another tenant, which is indistinguishable by design.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn insight_dashboard(&self, id: &InsightDashboardId) -> Result<Option<Dashboard>> {
        let row = sqlx::query_as::<_, DashboardRow>(&format!(
            "SELECT {DASHBOARD_COLS} FROM insight_dashboards WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(DashboardRow::into_dashboard))
    }

    /// The tenant's board carrying `system_key`, if it has been seeded. The
    /// seed's "have I run for this tenant?" question.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a malformed key; [`StoreError::Db`] on
    /// failure.
    pub async fn insight_dashboard_by_key(&self, system_key: &str) -> Result<Option<Dashboard>> {
        let key = normalize_key(system_key)?;
        let row = sqlx::query_as::<_, DashboardRow>(&format!(
            "SELECT {DASHBOARD_COLS} FROM insight_dashboards \
             WHERE tenant_id = $1 AND system_key = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(&key)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(DashboardRow::into_dashboard))
    }

    /// Renames a board. A seeded board renames like any other: its
    /// `system_key` is untouched, so the seed still never runs twice.
    ///
    /// # Errors
    /// [`StoreError::Validation`] as for create; [`StoreError::NotFound`] when
    /// the board isn't the tenant's; [`StoreError::Db`] on failure.
    pub async fn rename_insight_dashboard(
        &self,
        id: &InsightDashboardId,
        input: &NewDashboard,
    ) -> Result<()> {
        let name = normalize(input)?;
        let done = sqlx::query(
            "UPDATE insight_dashboards SET name = $3, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&name)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Deletes a board and, with it, its tiles (the composite foreign key
    /// cascades). Deletion is real here, unlike a billing document or a CRM
    /// board: a dashboard is a *view* of records, never a record of anything,
    /// so nothing is lost that the documents underneath do not still hold.
    ///
    /// Deleting a seeded board is allowed, and it does **not** come back: the
    /// seed asks whether the tenant ever had that key, so a tenant that threw
    /// the overview away is not handed it again every morning.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the board isn't the tenant's;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_insight_dashboard(&self, id: &InsightDashboardId) -> Result<()> {
        let done = sqlx::query("DELETE FROM insight_dashboards WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct DashboardRow {
    id: String,
    name: String,
    system_key: Option<String>,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl DashboardRow {
    fn into_dashboard(self) -> Dashboard {
        Dashboard {
            id: InsightDashboardId::new(self.id),
            name: self.name,
            system_key: self.system_key,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invalid<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn a_name_is_required_trimmed_and_bounded() {
        let name = normalize(&NewDashboard {
            name: "  Cash  ".to_owned(),
        })
        .unwrap_or_default();
        assert_eq!(name, "Cash");
        for blank in ["", "   ", "\t\n"] {
            let message = invalid(normalize(&NewDashboard {
                name: blank.to_owned(),
            }));
            assert!(message.contains("name"), "{message}");
        }
        let over = normalize(&NewDashboard {
            name: "x".repeat(DASHBOARD_NAME_MAX_CHARS + 1),
        });
        assert!(invalid(over).contains("at most"));
        assert!(
            normalize(&NewDashboard {
                name: "x".repeat(DASHBOARD_NAME_MAX_CHARS),
            })
            .is_ok(),
            "the bound is inclusive"
        );
    }

    #[test]
    fn a_system_key_is_a_lowercase_token_and_ours_passes_it() {
        assert_eq!(
            normalize_key(BUSINESS_OVERVIEW_KEY).unwrap_or_default(),
            BUSINESS_OVERVIEW_KEY
        );
        assert_eq!(normalize_key(" cash_2 ").unwrap_or_default(), "cash_2");
        for bad in ["", "   ", "Business", "business-overview", "büro", "a b"] {
            assert!(
                matches!(normalize_key(bad), Err(StoreError::Validation(_))),
                "expected rejection: {bad:?}"
            );
        }
        assert!(normalize_key(&"a".repeat(SYSTEM_KEY_MAX_CHARS + 1)).is_err());
    }
}
