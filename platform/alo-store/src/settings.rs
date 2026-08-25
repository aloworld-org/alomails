//! Mail settings persistence (Law 3: kept out of `account.rs`/`store.rs`): a
//! per-user signature and the tenant-wide organization footer. Both are HTML
//! fragments the compose surface inserts; an unset value is the empty string.
//!
//! New table/column land in migration 0017 and are not in the offline query
//! cache, so these use the runtime `sqlx::query*` path.

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::TenantId;
use crate::store::Store;
use time::OffsetDateTime;

/// A user's out-of-office auto-reply, and when it applies.
///
/// The window is the whole point of the type: "on" and "off" were never how
/// anybody uses this. You set it the evening before you leave and expect it to
/// stop by itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutOfOffice {
    /// Whether the reply is switched on at all.
    pub enabled: bool,
    /// The reply's subject.
    pub subject: String,
    /// The reply's body.
    pub message: String,
    /// When it starts. `None` means it is already in effect.
    pub from: Option<OffsetDateTime>,
    /// When it stops. `None` means until switched off by hand.
    pub to: Option<OffsetDateTime>,
}

impl OutOfOffice {
    /// Whether a reply should be sent at `now`.
    ///
    /// Each bound is independent and either may be absent, which is what makes
    /// the two familiar cases fall out of one rule: no `from` is "starting
    /// now", no `to` is "until I say otherwise" — the behaviour before there
    /// was a window at all.
    ///
    /// The end is exclusive. A holiday "to the 15th" that still replied on the
    /// 15th would answer the colleague who waited for you to be back.
    #[must_use]
    pub fn active_at(&self, now: OffsetDateTime) -> bool {
        self.enabled
            && self.from.is_none_or(|start| now >= start)
            && self.to.is_none_or(|end| now < end)
    }
}

/// The two spellings of one identity's signature (RFC 8621 §6.1).
///
/// Both are stored rather than one converted from the other, because a client
/// that round-trips `textSignature` must get its own text back — a conversion
/// looks like corruption to whoever typed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentitySignature {
    /// The plain-text spelling.
    pub text: String,
    /// The HTML spelling.
    pub html: String,
}

impl AccountStore {
    /// The signature stored for one send identity, or `None` when that
    /// identity has never been given its own — the caller then falls back to
    /// the account-level [`Self::signature`], which is what every identity
    /// used before per-identity signatures existed.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn identity_signature(&self, address: &str) -> Result<Option<IdentitySignature>> {
        let row = sqlx::query_as::<_, (String, String)>(
            "SELECT text_signature, html_signature FROM identity_signatures              WHERE tenant_id = $1 AND user_id = $2 AND address = $3",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(address)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(text, html)| IdentitySignature { text, html }))
    }

    /// Sets one send identity's signature; both spellings empty deletes the
    /// row, which restores the fall-back to the account-level signature rather
    /// than pinning an explicit "nothing" over it.
    ///
    /// The address is stored as given. The *caller* is responsible for only
    /// passing an address this user may send from — the JMAP layer resolves an
    /// identity id to an owned address before it gets here — so a row can never
    /// name an identity the account door would refuse to send as.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn set_identity_signature(
        &self,
        address: &str,
        text: &str,
        html: &str,
    ) -> Result<()> {
        if text.is_empty() && html.is_empty() {
            sqlx::query(
                "DELETE FROM identity_signatures                  WHERE tenant_id = $1 AND user_id = $2 AND address = $3",
            )
            .bind(self.tenant.as_str())
            .bind(self.user.as_str())
            .bind(address)
            .execute(&self.pool)
            .await?;
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO identity_signatures              (tenant_id, user_id, address, text_signature, html_signature)              VALUES ($1, $2, $3, $4, $5)              ON CONFLICT (tenant_id, user_id, address) DO UPDATE              SET text_signature = $4, html_signature = $5, updated_at = now()",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(address)
        .bind(text)
        .bind(html)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// This user's mail signature (HTML), or empty if unset.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn signature(&self) -> Result<String> {
        let sig: Option<String> = sqlx::query_scalar(
            "SELECT signature FROM user_settings WHERE tenant_id = $1 AND user_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_optional(&self.pool)
        .await?;
        Ok(sig.unwrap_or_default())
    }

    /// Sets this user's mail signature (HTML). Upsert; `updated_at` is bumped.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn set_signature(&self, signature: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO user_settings (tenant_id, user_id, signature) VALUES ($1, $2, $3) \
             ON CONFLICT (tenant_id, user_id) DO UPDATE SET signature = $3, updated_at = now()",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(signature)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// This user's mail filter rules as an opaque JSON string (the structured
    /// form the settings UI edits), or `"[]"` if unset. alo-jmap owns the
    /// rule model; the store only persists the text.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn filters(&self) -> Result<String> {
        let json: Option<String> = sqlx::query_scalar(
            "SELECT filters FROM user_settings WHERE tenant_id = $1 AND user_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_optional(&self.pool)
        .await?;
        Ok(json.unwrap_or_else(|| "[]".to_owned()))
    }

    /// Persists this user's mail filter rules (opaque JSON). Upsert. Does not
    /// touch the Sieve script — the caller rebuilds the managed script after.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn set_filters(&self, filters_json: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO user_settings (tenant_id, user_id, filters) VALUES ($1, $2, $3) \
             ON CONFLICT (tenant_id, user_id) DO UPDATE SET filters = $3, updated_at = now()",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(filters_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Persists the out-of-office state only (no Sieve script side effect), so a
    /// caller that regenerates a *combined* managed script (filters + vacation)
    /// controls script installation itself. See [`set_out_of_office`] for the
    /// self-contained variant.
    ///
    /// [`set_out_of_office`]: Self::set_out_of_office
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn set_out_of_office_state(
        &self,
        enabled: bool,
        subject: &str,
        message: &str,
        from: Option<OffsetDateTime>,
        to: Option<OffsetDateTime>,
    ) -> Result<()> {
        // A window that ends before it starts never fires, and reads to the
        // person who set it exactly like the feature being broken. The database
        // refuses it too; this is the readable half of that pair.
        if let (Some(start), Some(end)) = (from, to)
            && start >= end
        {
            return Err(StoreError::Validation(
                "the out-of-office end must be after its start".to_owned(),
            ));
        }
        sqlx::query(
            "INSERT INTO user_settings \
             (tenant_id, user_id, ooo_enabled, ooo_subject, ooo_message, ooo_from, ooo_to) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT (tenant_id, user_id) DO UPDATE \
             SET ooo_enabled = $3, ooo_subject = $4, ooo_message = $5, \
                 ooo_from = $6, ooo_to = $7, updated_at = now()",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(enabled)
        .bind(subject)
        .bind(message)
        .bind(from)
        .bind(to)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// This user's out-of-office reply and the window it applies in.
    ///
    /// A user who has never opened the setting has no row, which reads as
    /// "off" rather than as an error.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn out_of_office(&self) -> Result<OutOfOffice> {
        let row = sqlx::query_as::<
            _,
            (
                bool,
                String,
                String,
                Option<OffsetDateTime>,
                Option<OffsetDateTime>,
            ),
        >(
            "SELECT ooo_enabled, ooo_subject, ooo_message, ooo_from, ooo_to \
             FROM user_settings WHERE tenant_id = $1 AND user_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_optional(&self.pool)
        .await?;
        Ok(match row {
            Some((enabled, subject, message, from, to)) => OutOfOffice {
                enabled,
                subject,
                message,
                from,
                to,
            },
            None => OutOfOffice {
                enabled: false,
                subject: String::new(),
                message: String::new(),
                from: None,
                to: None,
            },
        })
    }

    /// Sets the out-of-office auto-reply. When `enabled`, installs and activates
    /// a managed `out-of-office` Sieve `vacation` script (the existing vacation
    /// machinery then delivers the reply, with its per-correspondent
    /// suppression); when disabled, deactivates and removes it. The state is
    /// also persisted so the settings UI can show it.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] if the generated script fails to compile (should
    /// not happen — the message is escaped); [`StoreError::Db`] on failure.
    pub async fn set_out_of_office(
        &self,
        enabled: bool,
        subject: &str,
        message: &str,
        from: Option<OffsetDateTime>,
        to: Option<OffsetDateTime>,
    ) -> Result<()> {
        // One statement writes this row, in `set_out_of_office_state`. Two
        // would be two things to keep in step about one fact.
        self.set_out_of_office_state(enabled, subject, message, from, to)
            .await?;

        // The script is installed whenever the reply is switched on, including
        // for a holiday that has not started yet: the window is read when a
        // message arrives, so a future window needs nothing scheduled and
        // nothing to be running on the day it opens.
        if enabled {
            self.put_sieve_script(OOO_SCRIPT, &ooo_script(subject, message))
                .await?;
            self.activate_sieve_script(Some(OOO_SCRIPT)).await?;
        } else {
            // Deactivate before delete — `delete_sieve_script` refuses an active
            // script (it must never leave delivery with a dangling active row).
            self.activate_sieve_script(None).await?;
            // Best-effort remove; absent is fine.
            if let Err(error) = self.delete_sieve_script(OOO_SCRIPT).await
                && !matches!(error, StoreError::NotFound)
            {
                return Err(error);
            }
        }
        Ok(())
    }
}

/// The name of the managed out-of-office Sieve script.
const OOO_SCRIPT: &str = "out-of-office";

/// The Sieve `:handle` the managed out-of-office reply carries.
///
/// It is what lets delivery tell *this* reply from one a user wrote themselves
/// in their own Sieve script. Only the managed one is gated on the window in
/// settings: someone who writes their own `vacation` rule means it to fire when
/// their rule says, not when a settings screen they never opened says.
pub const OOO_HANDLE: &str = "alo-out-of-office";

/// Builds a `vacation` Sieve script from the user's subject + message, escaping
/// the two characters special to a Sieve quoted string (`\` and `"`). A
/// 7-day per-correspondent suppression window is used.
fn ooo_script(subject: &str, message: &str) -> String {
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let subject_arg = if subject.trim().is_empty() {
        String::new()
    } else {
        format!(" :subject \"{}\"", esc(subject))
    };
    format!(
        "require [\"vacation\"];\nvacation :days 7 :handle \"{OOO_HANDLE}\"{subject_arg} \"{}\";",
        esc(message)
    )
}

impl Store {
    /// The tenant's organization footer (HTML), appended to outgoing mail, or
    /// empty if unset.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn org_footer(&self, tenant: &TenantId) -> Result<String> {
        let footer: Option<String> =
            sqlx::query_scalar("SELECT org_footer FROM tenants WHERE id = $1")
                .bind(tenant.as_str())
                .fetch_optional(self.pool())
                .await?;
        Ok(footer.unwrap_or_default())
    }

    /// Sets the tenant's organization footer (HTML). Admin-set (ADR 0012 gate
    /// enforced by the caller).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the tenant does not exist;
    /// [`StoreError::Db`] on failure.
    pub async fn set_org_footer(&self, tenant: &TenantId, footer: &str) -> Result<()> {
        let done = sqlx::query("UPDATE tenants SET org_footer = $2 WHERE id = $1")
            .bind(tenant.as_str())
            .bind(footer)
            .execute(self.pool())
            .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::OutOfOffice;
    use time::OffsetDateTime;
    use time::macros::datetime;

    /// A reply with the given window, switched on.
    fn away(from: Option<OffsetDateTime>, to: Option<OffsetDateTime>) -> OutOfOffice {
        OutOfOffice {
            enabled: true,
            subject: "Away".to_owned(),
            message: "I am away".to_owned(),
            from,
            to,
        }
    }

    const SEP_01: OffsetDateTime = datetime!(2026-09-01 00:00:00 UTC);
    const SEP_08: OffsetDateTime = datetime!(2026-09-08 12:00:00 UTC);
    const SEP_15: OffsetDateTime = datetime!(2026-09-15 00:00:00 UTC);

    #[test]
    fn switched_off_never_replies_whatever_the_window_says() {
        let mut ooo = away(Some(SEP_01), Some(SEP_15));
        ooo.enabled = false;
        assert!(!ooo.active_at(SEP_08), "the switch outranks the window");
    }

    #[test]
    fn no_window_is_the_behaviour_we_had_before() {
        // Every row written before the window existed reads as (None, None),
        // and must go on behaving exactly as it did: on means on.
        assert!(away(None, None).active_at(SEP_08));
    }

    #[test]
    fn a_holiday_set_in_advance_stays_quiet_until_it_starts() {
        let ooo = away(Some(SEP_08), Some(SEP_15));
        assert!(!ooo.active_at(SEP_01), "set on the 1st for the 8th");
    }

    #[test]
    fn it_replies_inside_the_window() {
        assert!(away(Some(SEP_01), Some(SEP_15)).active_at(SEP_08));
    }

    #[test]
    fn the_start_is_inclusive() {
        // "Away from the 1st" means the message arriving at one minute past
        // midnight gets the reply — and so does the one arriving exactly at it.
        assert!(away(Some(SEP_01), Some(SEP_15)).active_at(SEP_01));
    }

    #[test]
    fn the_end_is_exclusive_so_the_day_you_are_back_is_yours() {
        // The whole reason the end is exclusive: whoever writes on the morning
        // you return should reach you, not a message saying you are away.
        let ooo = away(Some(SEP_01), Some(SEP_15));
        assert!(!ooo.active_at(SEP_15));
        assert!(ooo.active_at(SEP_15 - time::Duration::seconds(1)));
    }

    #[test]
    fn an_open_ended_window_runs_until_switched_off() {
        let ooo = away(Some(SEP_01), None);
        assert!(ooo.active_at(SEP_08));
        assert!(ooo.active_at(SEP_15), "no end means no end");
    }

    #[test]
    fn an_end_without_a_start_is_in_effect_immediately() {
        let ooo = away(None, Some(SEP_15));
        assert!(ooo.active_at(SEP_01));
        assert!(!ooo.active_at(SEP_15));
    }
}
