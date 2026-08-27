//! The campaign return path's intake log (mail M4.4, ADR 0044 §4) — the
//! host-level receipt of what came back to the bounce address, and the one
//! read that turns a bounced address into the tenants whose campaigns mailed
//! it.
//!
//! ADR 0044 §4: *feedback loops are day one, not hardening. Hard bounces
//! suppress immediately.* The suppression itself is the tenant-scoped
//! [`suppress_campaign_address`](crate::TenantStore::suppress_campaign_address)
//! (C1.3) — this module never writes a suppression. What it owns is the two
//! host-level facts around that call:
//!
//! - **the receipt** ([`Store::record_campaign_bounce`]): one row per message
//!   the return path accepted, with its verdict and the raw bytes (bounded),
//!   because the message that matters most is the one that could *not* be
//!   parsed — a provider's nonstandard bounce is diagnosed from the bytes,
//!   never from a verdict of `none`;
//! - **the mapping** ([`Store::tenants_with_sent_campaign_recipient`]): which
//!   tenants' campaign mail actually went to the bounced address. The return
//!   path is one system mailbox shared by every tenant's campaign mail, so the
//!   arriving report names no tenant; `campaign_send_recipients` rows in state
//!   `sent` are the ground truth of who mailed whom, and only those tenants
//!   receive the suppression.
//!
//! Both live on [`Store`] rather than [`TenantStore`](crate::TenantStore) for
//! the same reason [`account_by_email`](Store::account_by_email) does: the MX
//! is the caller, and the MX sits above tenants by construction. Nothing here
//! reads message content beyond storing it, and nothing here sends anything.

use crate::error::{Result, StoreError};
use crate::id::{CampaignBounceId, TenantId};
use crate::store::Store;

/// The most message bytes one intake row keeps. A DSN is a few KiB of report
/// plus the original's headers; a provider that returns the entire original
/// body can exceed this, and the tail of a large attachment diagnoses
/// nothing — the row records the true wire size beside the truncated bytes.
pub const CAMPAIGN_BOUNCE_MESSAGE_MAX: usize = 256 * 1024;

/// The longest reported recipient address stored (the RFC 5321 path ceiling,
/// the same bound `campaign_send_recipients` holds its addresses to).
const BOUNCE_RECIPIENT_MAX: usize = 320;

/// The longest RFC 3463 status token stored (`5.7.999` is 7; anything past
/// this is not a status, whatever the report calls it).
const BOUNCE_STATUS_MAX: usize = 32;

/// What one message at the return path amounted to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BounceVerdict {
    /// An RFC 3464 report of a settled permanent failure (`Action: failed`,
    /// `Status: 5.x.x`) — the only verdict that suppresses (ADR 0044 §4: a
    /// soft failure retries, and only a settled one suppresses).
    Hard,
    /// A report of a transient condition (`4.x.x`, or `Action: delayed`).
    /// Recorded and deliberately not acted on: retrying is the sender's own
    /// machinery, and suppression is irreversible.
    Soft,
    /// Nothing to act on — not a delivery-status report at all, or one that
    /// reports only success. Stored so it can be read, never crashed on.
    None,
}

impl BounceVerdict {
    /// The stored token. Stable: it is written into rows that outlive
    /// releases.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Hard => "hard",
            Self::Soft => "soft",
            Self::None => "none",
        }
    }
}

/// One recorded intake, as it is stored — the operator's answer to "what
/// came back, and did it act".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CampaignBounce {
    pub id: CampaignBounceId,
    pub verdict: String,
    pub recipient: Option<String>,
    pub status: Option<String>,
    pub suppressed: i32,
    /// The stored bytes (truncated to [`CAMPAIGN_BOUNCE_MESSAGE_MAX`]).
    pub message: Vec<u8>,
    /// The true size on the wire, so truncation is visible.
    pub message_size: i64,
}

/// One message the return path accepted, ready to record.
#[derive(Debug, Clone)]
pub struct NewCampaignBounce<'a> {
    pub verdict: BounceVerdict,
    /// The reported address the verdict is about — already normalised by the
    /// caller (the same fold `campaign_send_recipients` holds), or `None`
    /// when the report named no usable one.
    pub recipient: Option<&'a str>,
    /// The RFC 3463 enhanced status as reported (e.g. `5.1.1`).
    pub status: Option<&'a str>,
    /// How many tenant suppressions this message fired.
    pub suppressed: i32,
    /// The message as received on the wire; stored truncated to
    /// [`CAMPAIGN_BOUNCE_MESSAGE_MAX`], with the true size beside it.
    pub message: &'a [u8],
}

/// One receipt row as the database returns it, in [`lookup_sql`]'s column
/// order.
type BounceRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    i32,
    Vec<u8>,
    i64,
);

/// The insert. Host-level by design — see the module docs for why this table
/// carries no tenant.
fn record_sql() -> &'static str {
    "INSERT INTO campaign_bounces \
         (id, verdict, recipient, status, suppressed, message, message_size) \
     VALUES ($1, $2, $3, $4, $5, $6, $7)"
}

/// The receipt read, by id.
fn lookup_sql() -> &'static str {
    "SELECT id, verdict, recipient, status, suppressed, message, message_size \
       FROM campaign_bounces \
      WHERE id = $1"
}

/// The mapping read: which tenants' campaign mail went to this address. Only
/// rows in state `sent` count — an enrolment that never left cannot have
/// bounced, and a `failed` row already settled its own way.
fn tenants_sql() -> &'static str {
    "SELECT DISTINCT tenant_id FROM campaign_send_recipients \
      WHERE address = $1 AND state = 'sent'"
}

impl Store {
    /// Records one message the campaign return path accepted.
    ///
    /// Idempotence lives in the caller's ordering, not here: the intake
    /// suppresses first (itself idempotent — the first reason stands) and
    /// records last, so a delivery retried after a mid-flight fault re-runs
    /// the idempotent half and the receipt is written once, on the attempt
    /// that answered the sender `250`.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the recipient or status exceeds its
    /// bound; [`StoreError::Db`] on failure.
    pub async fn record_campaign_bounce(
        &self,
        bounce: &NewCampaignBounce<'_>,
    ) -> Result<CampaignBounceId> {
        if bounce
            .recipient
            .is_some_and(|r| r.chars().count() > BOUNCE_RECIPIENT_MAX)
        {
            return Err(StoreError::Validation(format!(
                "a bounced recipient address fits in {BOUNCE_RECIPIENT_MAX} characters"
            )));
        }
        if bounce
            .status
            .is_some_and(|s| s.chars().count() > BOUNCE_STATUS_MAX)
        {
            return Err(StoreError::Validation(format!(
                "a bounce status fits in {BOUNCE_STATUS_MAX} characters"
            )));
        }
        if bounce.suppressed < 0 {
            return Err(StoreError::Validation(
                "a bounce cannot have fired a negative number of suppressions".to_owned(),
            ));
        }
        let id = CampaignBounceId::generate();
        let kept = &bounce.message[..bounce.message.len().min(CAMPAIGN_BOUNCE_MESSAGE_MAX)];
        sqlx::query(record_sql())
            .bind(id.as_str())
            .bind(bounce.verdict.as_str())
            .bind(bounce.recipient)
            .bind(bounce.status)
            .bind(bounce.suppressed)
            .bind(kept)
            .bind(i64::try_from(bounce.message.len()).unwrap_or(i64::MAX))
            .execute(self.pool())
            .await
            .map_err(StoreError::Db)?;
        Ok(id)
    }

    /// One recorded intake, by id — `None` when there is no such receipt.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn campaign_bounce(&self, id: &CampaignBounceId) -> Result<Option<CampaignBounce>> {
        let row: Option<BounceRow> = sqlx::query_as(lookup_sql())
            .bind(id.as_str())
            .fetch_optional(self.pool())
            .await
            .map_err(StoreError::Db)?;
        Ok(row.map(
            |(id, verdict, recipient, status, suppressed, message, message_size)| CampaignBounce {
                id: CampaignBounceId::new(id),
                verdict,
                recipient,
                status,
                suppressed,
                message,
                message_size,
            },
        ))
    }

    /// The tenants whose campaign mail actually went to `address` — the ones
    /// a hard bounce of it suppresses into, and nobody else.
    ///
    /// The address is folded by the same rule the recipient ledger and the
    /// suppression list fold by, so "we mailed them" and "they bounced" mean
    /// the same person. An address that rule refuses maps to no tenants
    /// (`Ok(vec![])`) rather than an error: a mangled `Final-Recipient` in
    /// somebody else's bounce format is that report's problem, not a fault in
    /// our intake.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn tenants_with_sent_campaign_recipient(
        &self,
        address: &str,
    ) -> Result<Vec<TenantId>> {
        let Some(address) = crate::campaign_audience::normalise_address(address) else {
            return Ok(Vec::new());
        };
        let rows: Vec<(String,)> = sqlx::query_as(tenants_sql())
            .bind(&address)
            .fetch_all(self.pool())
            .await
            .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(|(t,)| TenantId::new(t)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The identifiers a SQL string contains — the same helper the other
    /// campaign modules keep, for the same promise.
    fn identifiers(sql: &str) -> Vec<&str> {
        sql.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .filter(|token| !token.is_empty())
            .collect()
    }

    #[test]
    fn no_query_in_this_module_can_read_the_per_user_address_book() {
        for sql in [record_sql(), lookup_sql(), tenants_sql()] {
            assert!(
                !identifiers(sql).contains(&"contacts"),
                "a campaign bounce query names the per-user address book: {sql}"
            );
        }
    }

    #[test]
    fn the_mapping_counts_only_mail_that_actually_left() {
        // An enrolment that never left cannot have bounced; a bounce for it
        // would suppress somebody the tenant never mailed.
        let sql = tenants_sql();
        assert!(sql.contains("state = 'sent'"), "unguarded mapping: {sql}");
        assert!(
            !identifiers(sql).contains(&"campaign_suppression"),
            "the mapping must read the ledger, not the consequence: {sql}"
        );
    }

    #[test]
    fn nothing_in_this_module_writes_a_suppression() {
        // The suppression is C1.3's tenant-scoped seam; this module firing it
        // directly would be a second, host-level door into a tenant's list.
        for sql in [record_sql(), lookup_sql(), tenants_sql()] {
            assert!(
                !identifiers(sql).contains(&"campaign_suppression"),
                "a bounce query touches the suppression table: {sql}"
            );
        }
    }

    #[test]
    fn verdict_tokens_are_the_migration_s_three() {
        assert_eq!(BounceVerdict::Hard.as_str(), "hard");
        assert_eq!(BounceVerdict::Soft.as_str(), "soft");
        assert_eq!(BounceVerdict::None.as_str(), "none");
    }
}
