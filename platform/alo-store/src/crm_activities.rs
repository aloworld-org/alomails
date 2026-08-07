//! What was said and done on a deal (alo CRM, ADR 0035, wave B2) — the notes,
//! the logged calls and the meetings that make a deal a record rather than a
//! number on a board.
//!
//! Three rules shape this file (`docs/design/crm.md`, "Activities and next
//! steps"):
//!
//! - **An activity is written once.** There is no edit: a correction is another
//!   note, which is what a log of what was said and done ought to be.
//! - **Only its author may delete it**, and a colleague who tries gets
//!   [`StoreError::Forbidden`] rather than a `NotFound` — the row is readable
//!   tenant-wide, so hiding its existence from someone already reading it would
//!   be theatre.
//! - **A next step is not an activity.** It is a real task in the tasks module,
//!   linked back by ADR 0021's source link and lived in
//!   [`crate::crm_next_steps`]. This table has no due date and no done flag,
//!   deliberately: two to-do lists in one workspace is how a CRM becomes the
//!   system nobody updates.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::billing_field::required;
use crate::error::{Result, StoreError};
use crate::id::{CrmActivityId, CrmDealId};

/// The longest note we store, in characters (`docs/design/crm.md`, "Bounds").
/// A note is what was said on a call, not the attachment of a contract.
pub const ACTIVITY_BODY_MAX_CHARS: usize = 10_000;

/// The most entries one deal's log may hold.
///
/// The same shape as the per-deal conversation cap
/// ([`crate::crm_deal_threads::DEAL_THREADS_MAX`]) and for the same reason: the
/// drawer reads the log whole, so the read is bounded by the record rather than
/// by a cursor nobody paged. A deal that has collected five hundred notes has
/// stopped being one opportunity.
pub const DEAL_ACTIVITIES_MAX: i64 = 500;

/// What an entry in a deal's log is.
///
/// A closed vocabulary, because it is what the drawer draws an icon from and
/// what a report will count by; free text would be three spellings of "call"
/// inside a month.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ActivityKind {
    /// Something worth writing down.
    #[default]
    Note,
    /// A telephone call that happened.
    Call,
    /// A meeting that happened.
    Meeting,
}

impl ActivityKind {
    /// The value the `kind` column carries, and the one a route reads and
    /// writes — one word means one thing on both sides.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Call => "call",
            Self::Meeting => "meeting",
        }
    }

    /// Parses a kind a caller sent, or `None` if it is not one we know. The
    /// route edge answers `None` with `422`: an unrecognised kind must never be
    /// silently stored as a note, because the log would then say a call was
    /// never made.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "note" => Some(Self::Note),
            "call" => Some(Self::Call),
            "meeting" => Some(Self::Meeting),
            _ => None,
        }
    }
}

/// A new entry in a deal's log.
#[derive(Debug, Clone, Default)]
pub struct NewActivity {
    /// What it is.
    pub kind: ActivityKind,
    /// What was said. Required, non-blank, bounded by
    /// [`ACTIVITY_BODY_MAX_CHARS`].
    pub body: String,
    /// When it happened, or `None` for now. A call logged an hour later is
    /// dated the hour it took place, not the hour somebody found time to type
    /// it up.
    pub happened_at: Option<OffsetDateTime>,
}

/// One stored entry in a deal's log.
#[derive(Debug, Clone)]
pub struct Activity {
    /// Opaque id, unique within the tenant.
    pub id: CrmActivityId,
    /// The deal it belongs to.
    pub deal_id: CrmDealId,
    /// What it is.
    pub kind: ActivityKind,
    /// What was said.
    pub body: String,
    /// When it happened.
    pub happened_at: OffsetDateTime,
    /// Who wrote it — the only colleague who may delete it again.
    pub author_user_id: String,
    /// When it was written, which is not always when it happened.
    pub created_at: OffsetDateTime,
}

/// One row as the database hands it back.
#[derive(sqlx::FromRow)]
struct ActivityRow {
    id: String,
    deal_id: String,
    kind: String,
    body: String,
    happened_at: OffsetDateTime,
    author_user_id: String,
    created_at: OffsetDateTime,
}

impl ActivityRow {
    /// Decodes a stored row. An unknown `kind` is a row that should not exist —
    /// the column carries a `CHECK` — so it is a decode failure rather than a
    /// guess: reporting a logged call as a note is worse than an error.
    fn into_activity(self) -> Result<Activity> {
        let kind = ActivityKind::parse(&self.kind).ok_or_else(|| {
            StoreError::Db(sqlx::Error::Decode(
                "crm_activities.kind is not a known kind".into(),
            ))
        })?;
        Ok(Activity {
            id: CrmActivityId::new(self.id),
            deal_id: CrmDealId::new(self.deal_id),
            kind,
            body: self.body,
            happened_at: self.happened_at,
            author_user_id: self.author_user_id,
            created_at: self.created_at,
        })
    }
}

impl AccountStore {
    /// Writes one entry into a deal's log, authored by the acting user.
    ///
    /// The deal's row lock is both the existence check and what serialises two
    /// colleagues writing at once, so the cap below cannot be walked past by a
    /// concurrent write — the same shape the conversation link uses.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the deal is not this tenant's;
    /// [`StoreError::Validation`] on a blank or over-long body;
    /// [`StoreError::Conflict`] beyond [`DEAL_ACTIVITIES_MAX`];
    /// [`StoreError::Db`] on failure.
    pub async fn add_crm_activity(
        &self,
        deal: &CrmDealId,
        input: &NewActivity,
    ) -> Result<CrmActivityId> {
        let body = required("body", &input.body, ACTIVITY_BODY_MAX_CHARS)?;
        let id = CrmActivityId::generate();
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query_scalar::<_, String>(
            "SELECT id FROM crm_deals WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(self.tenant.as_str())
        .bind(deal.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?
        .ok_or(StoreError::NotFound)?;

        let held: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM crm_activities WHERE tenant_id = $1 AND deal_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(deal.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if held >= DEAL_ACTIVITIES_MAX {
            return Err(StoreError::Conflict(format!(
                "a deal may hold at most {DEAL_ACTIVITIES_MAX} activities"
            )));
        }

        sqlx::query(
            "INSERT INTO crm_activities \
                 (tenant_id, id, deal_id, kind, body, happened_at, author_user_id) \
             VALUES ($1, $2, $3, $4, $5, COALESCE($6, now()), $7)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(deal.as_str())
        .bind(input.kind.as_str())
        .bind(&body)
        .bind(input.happened_at)
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// One deal's log, most recent first, bounded by [`DEAL_ACTIVITIES_MAX`] —
    /// which the write path enforces, so this read is whole and not a page.
    ///
    /// The log is readable by every member of the tenant, exactly like the deal
    /// it hangs on.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the deal is not this tenant's — never an
    /// empty list, which would be an existence oracle;
    /// [`StoreError::Db`] on failure.
    pub async fn crm_activities(&self, deal: &CrmDealId) -> Result<Vec<Activity>> {
        if self.crm_deal(deal).await?.is_none() {
            return Err(StoreError::NotFound);
        }
        let rows = sqlx::query_as::<_, ActivityRow>(
            "SELECT id, deal_id, kind, body, happened_at, author_user_id, created_at \
             FROM crm_activities WHERE tenant_id = $1 AND deal_id = $2 \
             ORDER BY happened_at DESC, id DESC",
        )
        .bind(self.tenant.as_str())
        .bind(deal.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(ActivityRow::into_activity).collect()
    }

    /// Deletes one entry, on the word of the colleague who wrote it.
    ///
    /// A member of the tenant who did not write it is refused with
    /// [`StoreError::Forbidden`] and not a `NotFound`: they can already read the
    /// row, so hiding its existence would be theatre rather than privacy. An
    /// entry of another tenant is the ordinary [`StoreError::NotFound`].
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the entry is not this tenant's;
    /// [`StoreError::Forbidden`] when the caller did not write it;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_crm_activity(&self, id: &CrmActivityId) -> Result<()> {
        let author: Option<String> = sqlx::query_scalar(
            "SELECT author_user_id FROM crm_activities WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let author = author.ok_or(StoreError::NotFound)?;
        if author != self.user.as_str() {
            return Err(StoreError::Forbidden);
        }
        sqlx::query(
            "DELETE FROM crm_activities WHERE tenant_id = $1 AND id = $2 \
                     AND author_user_id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(self.user.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kind_means_the_same_word_on_both_sides() {
        for kind in [
            ActivityKind::Note,
            ActivityKind::Call,
            ActivityKind::Meeting,
        ] {
            assert_eq!(ActivityKind::parse(kind.as_str()), Some(kind));
        }
    }

    #[test]
    fn an_unknown_kind_is_never_quietly_a_note() {
        for bad in ["", "NOTE", "email", "task", "0"] {
            assert_eq!(ActivityKind::parse(bad), None, "{bad}");
        }
        assert_eq!(ActivityKind::default(), ActivityKind::Note);
    }

    #[test]
    fn a_stored_row_with_an_unknown_kind_is_a_decode_failure_not_a_guess() {
        let row = ActivityRow {
            id: "cra_1".to_owned(),
            deal_id: "crd_1".to_owned(),
            kind: "seance".to_owned(),
            body: "…".to_owned(),
            happened_at: OffsetDateTime::UNIX_EPOCH,
            author_user_id: "usr_1".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
        };
        assert!(matches!(row.into_activity(), Err(StoreError::Db(_))));
    }
}
