//! Cost and abuse control for the site visitor assistant (ADR 0040 §3, item
//! S3.02c). The assistant is an anonymous endpoint that feeds a metered model,
//! which makes it a bill any stranger on the internet can run up — so every
//! site has a **monthly spending ceiling that is defaulted rather than
//! blank**, the assistant is off until the tenant switches it on, and at the
//! ceiling it does not degrade quietly: the public gate reads `Exhausted`, the
//! widget offers the contact form instead, and the owner is told once, in
//! their inbox.
//!
//! Money is integer euro cents (the money law); the ceiling is **spend, not
//! tokens** — tokens are our unit, not a customer's. The ledger is one row
//! per site per UTC calendar month, so a new month is a fresh budget by key,
//! not by reset job. The gate always computes exhaustion live from
//! `spent >= ceiling`, so raising the ceiling reopens the assistant
//! immediately; the `ceiling_hit_at` stamp exists only so the owner is
//! notified exactly once per site-month.

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{SiteId, TenantId, UserId};
use crate::model::AiConfigRow;
use crate::site_public::{PublishedSite, SitePublicStore};
use crate::store::Store;

/// The ceiling a site spends under until its tenant chooses one: €10.00 per
/// month. Defaulted rather than blank (ADR 0040 §3) — there is no state in
/// which the assistant is on with no ceiling.
pub const DEFAULT_CHAT_MONTHLY_CEILING_CENTS: i64 = 1_000;
/// The lowest settable ceiling, €1.00 — below that "on" would be a lie.
pub const MIN_CHAT_MONTHLY_CEILING_CENTS: i64 = 100;
/// The highest settable ceiling, €10 000.00 — above that a typo, not a plan.
pub const MAX_CHAT_MONTHLY_CEILING_CENTS: i64 = 1_000_000;

/// The ledger key for the month containing `at`, in UTC: `YYYY-MM`.
#[must_use]
pub fn chat_month_key(at: OffsetDateTime) -> String {
    let utc = at.to_offset(time::UtcOffset::UTC);
    format!("{:04}-{:02}", utc.year(), u8::from(utc.month()))
}

/// A site's assistant settings joined with its ledger for one month — what
/// the settings screen shows and the edit routes return.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteChatSettings {
    pub enabled: bool,
    pub monthly_ceiling_cents: i64,
    /// The month the spend figures below describe (`YYYY-MM`, UTC).
    pub month: String,
    pub spent_cents: i64,
    /// Live truth, not the notification stamp: `spent >= ceiling` right now,
    /// so lowering the ceiling below what is already spent reads as hit.
    pub ceiling_hit: bool,
}

/// The public gate's answer for one visitor question, read per request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatGate {
    /// The tenant never switched the assistant on (or switched it off).
    /// Absence of a settings row reads as this — fail closed.
    Disabled,
    /// The assistant may answer; `remaining_cents` of this month's budget
    /// stand between it and [`ChatGate::Exhausted`].
    Ready { remaining_cents: i64 },
    /// This month's ceiling is spent: the assistant says it is unavailable
    /// and offers the contact form instead (never a quiet degradation).
    Exhausted,
}

/// Everything the notification sweep needs to tell one owner their
/// assistant's ceiling was hit, resolved in the claim itself.
#[derive(Debug, Clone)]
pub struct ChatCeilingNotification {
    /// The tenant the ledger row belongs to — the only tenant whose inbox
    /// the notification may reach.
    pub tenant: TenantId,
    /// The site's creator: the account whose inbox receives the message.
    pub owner: UserId,
    pub site_name: String,
    pub site_subdomain: String,
    /// The exhausted month (`YYYY-MM`, UTC).
    pub month: String,
    pub monthly_ceiling_cents: i64,
    pub spent_cents: i64,
}

fn validate_ceiling(cents: i64) -> Result<()> {
    if !(MIN_CHAT_MONTHLY_CEILING_CENTS..=MAX_CHAT_MONTHLY_CEILING_CENTS).contains(&cents) {
        return Err(StoreError::Conflict(format!(
            "the assistant's monthly spending ceiling must be between \
             {MIN_CHAT_MONTHLY_CEILING_CENTS} and {MAX_CHAT_MONTHLY_CEILING_CENTS} cents"
        )));
    }
    Ok(())
}

impl AccountStore {
    /// The site's assistant settings plus its ledger for `month`. A site
    /// that never touched the settings reads as the defaults: off, with the
    /// default ceiling — never blank.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the site isn't the tenant's;
    /// [`StoreError::Db`] on backend failure.
    pub async fn site_chat_settings(&self, site: &SiteId, month: &str) -> Result<SiteChatSettings> {
        let row = sqlx::query_as::<_, (Option<bool>, Option<i64>, Option<i64>)>(
            "SELECT st.enabled, st.monthly_ceiling_cents, sp.spent_cents \
             FROM sites s \
             LEFT JOIN site_chat_settings st \
               ON st.tenant_id = s.tenant_id AND st.site_id = s.id \
             LEFT JOIN site_chat_spend sp \
               ON sp.tenant_id = s.tenant_id AND sp.site_id = s.id AND sp.month = $3 \
             WHERE s.tenant_id = $1 AND s.id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(month)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let Some((enabled, ceiling, spent)) = row else {
            return Err(StoreError::NotFound);
        };
        let monthly_ceiling_cents = ceiling.unwrap_or(DEFAULT_CHAT_MONTHLY_CEILING_CENTS);
        let spent_cents = spent.unwrap_or(0);
        Ok(SiteChatSettings {
            enabled: enabled.unwrap_or(false),
            monthly_ceiling_cents,
            month: month.to_owned(),
            spent_cents,
            ceiling_hit: spent_cents >= monthly_ceiling_cents,
        })
    }

    /// Sets the assistant's switch and ceiling in one write — the same screen
    /// sets both (ADR 0040 §3) — and returns the resulting view for `month`.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] when the ceiling is outside
    /// [`MIN_CHAT_MONTHLY_CEILING_CENTS`]..=[`MAX_CHAT_MONTHLY_CEILING_CENTS`];
    /// [`StoreError::NotFound`] when the site isn't the tenant's;
    /// [`StoreError::Db`] on backend failure.
    pub async fn set_site_chat_settings(
        &self,
        site: &SiteId,
        enabled: bool,
        monthly_ceiling_cents: i64,
        month: &str,
    ) -> Result<SiteChatSettings> {
        validate_ceiling(monthly_ceiling_cents)?;
        let done = sqlx::query(
            "INSERT INTO site_chat_settings (tenant_id, site_id, enabled, monthly_ceiling_cents) \
             SELECT s.tenant_id, s.id, $3, $4 FROM sites s \
             WHERE s.tenant_id = $1 AND s.id = $2 \
             ON CONFLICT (tenant_id, site_id) DO UPDATE \
                SET enabled = EXCLUDED.enabled, \
                    monthly_ceiling_cents = EXCLUDED.monthly_ceiling_cents, \
                    updated_at = now()",
        )
        .bind(self.tenant.as_str())
        .bind(site.as_str())
        .bind(enabled)
        .bind(monthly_ceiling_cents)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        self.site_chat_settings(site, month).await
    }
}

impl SitePublicStore {
    /// The per-request gate in front of one visitor question: may the
    /// resolved site's assistant answer right now, within `month`'s budget?
    /// Scoped by the resolved value's private tenant pairing, like every
    /// other read on this door. No settings row reads as
    /// [`ChatGate::Disabled`] — fail closed.
    ///
    /// # Errors
    /// [`StoreError::Db`] on backend failure.
    pub async fn chat_gate(&self, site: &PublishedSite, month: &str) -> Result<ChatGate> {
        let row = sqlx::query_as::<_, (bool, i64, i64)>(
            "SELECT st.enabled, st.monthly_ceiling_cents, COALESCE(sp.spent_cents, 0) \
             FROM site_chat_settings st \
             LEFT JOIN site_chat_spend sp \
               ON sp.tenant_id = st.tenant_id AND sp.site_id = st.site_id AND sp.month = $3 \
             WHERE st.tenant_id = $1 AND st.site_id = $2",
        )
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .bind(month)
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(match row {
            None | Some((false, _, _)) => ChatGate::Disabled,
            Some((true, ceiling, spent)) if spent >= ceiling => ChatGate::Exhausted,
            Some((true, ceiling, spent)) => ChatGate::Ready {
                remaining_cents: ceiling - spent,
            },
        })
    }

    /// The resolved site's tenant's default AI backend, for the visitor
    /// assistant's own model call (S3.02e) — the same row, mapped the same
    /// way, as the authenticated door's `default_ai_config`: the enabled
    /// default provider, first listed model. `None` means no usable backend
    /// is configured and the assistant is honestly unavailable.
    ///
    /// # Errors
    /// [`StoreError::Db`] on backend failure.
    pub async fn tenant_ai_config(&self, site: &PublishedSite) -> Result<Option<AiConfigRow>> {
        let row = sqlx::query_as::<_, (String, String, Option<String>, bool)>(
            "SELECT base_url, model, api_key, enabled FROM ai_providers \
             WHERE tenant_id = $1 AND is_default AND enabled LIMIT 1",
        )
        .bind(site.tenant.as_str())
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(|(base_url, model, api_key, enabled)| AiConfigRow {
            base_url,
            // A provider may enable several models (stored comma-separated);
            // the first is the active model the AI features request.
            model: model
                .split(',')
                .next()
                .map(str::trim)
                .unwrap_or("")
                .to_owned(),
            api_key,
            enabled,
        }))
    }

    /// Adds `cents` of model spend to the resolved site's ledger for `month`,
    /// atomically, stamping `ceiling_hit_at` in the same statement when this
    /// write is the one that crosses the ceiling — exactly once per
    /// site-month, however many writers race. Returns `true` for that
    /// crossing write (the moment the owner-notification becomes due).
    ///
    /// # Errors
    /// [`StoreError::Validation`] when `cents` is not positive;
    /// [`StoreError::Db`] on backend failure.
    pub async fn record_chat_spend(
        &self,
        site: &PublishedSite,
        month: &str,
        cents: i64,
    ) -> Result<bool> {
        if cents <= 0 {
            return Err(StoreError::Validation(
                "recorded assistant spend must be a positive number of cents".to_owned(),
            ));
        }
        let (spent, ceiling): (i64, i64) = sqlx::query_as(
            "WITH ceiling AS ( \
                 SELECT COALESCE((SELECT monthly_ceiling_cents FROM site_chat_settings \
                                   WHERE tenant_id = $1 AND site_id = $2), $5) AS cents) \
             INSERT INTO site_chat_spend (tenant_id, site_id, month, spent_cents, ceiling_hit_at) \
             VALUES ($1, $2, $3, $4, \
                     CASE WHEN $4 >= (SELECT cents FROM ceiling) THEN now() END) \
             ON CONFLICT (tenant_id, site_id, month) DO UPDATE \
                SET spent_cents = site_chat_spend.spent_cents + $4, \
                    ceiling_hit_at = COALESCE(site_chat_spend.ceiling_hit_at, \
                        CASE WHEN site_chat_spend.spent_cents + $4 >= \
                                  (SELECT cents FROM ceiling) THEN now() END), \
                    updated_at = now() \
             RETURNING spent_cents, (SELECT cents FROM ceiling)",
        )
        .bind(site.tenant.as_str())
        .bind(site.site.as_str())
        .bind(month)
        .bind(cents)
        .bind(DEFAULT_CHAT_MONTHLY_CEILING_CENTS)
        .fetch_one(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(spent >= ceiling && spent - cents < ceiling)
    }
}

impl Store {
    /// Claims up to `limit` hit ceilings awaiting owner notification, oldest
    /// hit first, marking each notified in the same statement (at-most-once —
    /// a crash between claim and delivery loses a notification but can never
    /// duplicate one; the settings screen still shows the hit either way).
    /// Concurrent sweeps skip each other's locked rows (`FOR UPDATE SKIP
    /// LOCKED`).
    ///
    /// System-level by design: the sweep spans tenants, and each returned row
    /// carries the tenant + owner the delivery must scope itself to.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn claim_chat_ceiling_notifications(
        &self,
        limit: i64,
    ) -> Result<Vec<ChatCeilingNotification>> {
        let rows = sqlx::query_as::<_, ClaimRow>(
            "UPDATE site_chat_spend sp \
                SET hit_notified_at = now() \
               FROM sites s \
               LEFT JOIN site_chat_settings st \
                 ON st.tenant_id = s.tenant_id AND st.site_id = s.id \
              WHERE s.tenant_id = sp.tenant_id AND s.id = sp.site_id \
                AND (sp.tenant_id, sp.site_id, sp.month) IN ( \
                    SELECT tenant_id, site_id, month FROM site_chat_spend \
                     WHERE ceiling_hit_at IS NOT NULL AND hit_notified_at IS NULL \
                     ORDER BY ceiling_hit_at, site_id \
                     LIMIT $1 \
                     FOR UPDATE SKIP LOCKED) \
             RETURNING sp.tenant_id, s.created_by AS owner, s.name AS site_name, \
                       s.subdomain AS site_subdomain, sp.month, \
                       COALESCE(st.monthly_ceiling_cents, $2) AS monthly_ceiling_cents, \
                       sp.spent_cents",
        )
        .bind(limit)
        .bind(DEFAULT_CHAT_MONTHLY_CEILING_CENTS)
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(ClaimRow::into_notification).collect())
    }
}

#[derive(sqlx::FromRow)]
struct ClaimRow {
    tenant_id: String,
    owner: String,
    site_name: String,
    site_subdomain: String,
    month: String,
    monthly_ceiling_cents: i64,
    spent_cents: i64,
}

impl ClaimRow {
    fn into_notification(self) -> ChatCeilingNotification {
        ChatCeilingNotification {
            tenant: TenantId::new(self.tenant_id),
            owner: UserId::new(self.owner),
            site_name: self.site_name,
            site_subdomain: self.site_subdomain,
            month: self.month,
            monthly_ceiling_cents: self.monthly_ceiling_cents,
            spent_cents: self.spent_cents,
        }
    }
}
