//! Sieve storage and delivery-time filtering on [`AccountStore`], kept out
//! of `account.rs` (Law 3: its reason to change is filtering, not the core
//! mail model). Everything is account-scoped by construction — a user's
//! scripts, suppression rows, and redirect budget all carry
//! `(tenant, user)`, and `deliver_sieve` files only into the owner's own
//! mailboxes. No script failure ever loses mail (implicit keep). See
//! `docs/design/sieve-filtering.md`.

use alo_sieve::{Action, EvalContext, Limits, Message as SieveMsg};

use time::OffsetDateTime;

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::MailboxId;

/// The default vacation suppression window when `:days` is absent.
const DEFAULT_VACATION_DAYS: u32 = 7;
/// Per-account redirect budget: at most this many redirects per window.
const REDIRECT_CAP: i64 = 100;
/// The redirect budget window, in seconds (rolling).
const REDIRECT_WINDOW_SECS: i64 = 3600;
/// Max Sieve scripts per account (matches JMAP `maxNumberScripts`).
const MAX_SCRIPTS: i64 = 100;
/// Max script-name length (matches JMAP `maxSizeScriptName`).
const MAX_SCRIPT_NAME_LEN: usize = 512;

/// One of an account's Sieve scripts (metadata).
#[derive(Debug, Clone)]
pub struct SieveScriptMeta {
    /// Script name (unique per account).
    pub name: String,
    /// Whether this is the active script run at delivery.
    pub active: bool,
    /// Content length in bytes.
    pub size: i64,
}

/// An action the delivery bridge must perform through the outbound queue —
/// the store never sends mail itself. Safety budgets (redirect rate, loop,
/// vacation suppression) are already applied before an action lands here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutboundAction {
    /// Forward a copy of the message to `address` (with the owner identity).
    Redirect { address: String },
    /// Send a vacation auto-reply to `to`.
    Vacation {
        /// Correspondent to reply to.
        to: String,
        /// Reply subject, if the script set one.
        subject: Option<String>,
        /// Reply From, if the script set one.
        from: Option<String>,
        /// Reply body.
        reason: String,
    },
}

/// The result of a Sieve delivery: the outbound actions for the bridge and
/// any warnings (logged; never shown to a remote sender).
#[derive(Debug, Clone, Default)]
pub struct SieveDelivery {
    /// Actions requiring the outbound queue.
    pub outbound: Vec<OutboundAction>,
    /// Non-fatal warnings.
    pub warnings: Vec<String>,
    /// `true` if the message was filed somewhere (keep/fileinto); `false`
    /// only when the script `discard`ed it.
    pub filed: bool,
}

impl AccountStore {
    // ---- script CRUD ---------------------------------------------------

    /// Lists this account's Sieve scripts.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn list_sieve_scripts(&self) -> Result<Vec<SieveScriptMeta>> {
        let rows = sqlx::query!(
            "SELECT name, active, length(content) AS \"size!\" FROM sieve_scripts \
             WHERE tenant_id = $1 AND user_id = $2 ORDER BY name",
            self.tenant().as_str(),
            self.user().as_str()
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| SieveScriptMeta {
                name: r.name,
                active: r.active,
                size: i64::from(r.size),
            })
            .collect())
    }

    /// Fetches one script's content by name.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if no such script for this account.
    pub async fn sieve_script(&self, name: &str) -> Result<String> {
        let row = sqlx::query!(
            "SELECT content FROM sieve_scripts WHERE tenant_id = $1 AND user_id = $2 AND name = $3",
            self.tenant().as_str(),
            self.user().as_str(),
            name
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)?;
        Ok(row.content)
    }

    /// Stores (creates or replaces) a script after **compile-validating** it
    /// — an invalid script is rejected, never stored.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] with the compile error if `content` does not
    /// compile; [`StoreError::Db`] on failure.
    pub async fn put_sieve_script(&self, name: &str, content: &str) -> Result<()> {
        // Enforce the advertised caps (JMAP maxSizeScriptName / maxNumberScripts).
        if name.is_empty() || name.len() > MAX_SCRIPT_NAME_LEN {
            return Err(StoreError::Conflict(
                "script name length out of range".to_owned(),
            ));
        }
        // Validate before storing (RFC 9661 `SieveScript/set` invalidScript).
        if let Err(e) = alo_sieve::compile(content, Limits::default()) {
            return Err(StoreError::Conflict(format!("invalid Sieve script: {e}")));
        }
        // A *new* name may not exceed the per-account script cap.
        let existing = sqlx::query!(
            "SELECT count(*) FILTER (WHERE name = $3) AS \"has!\", count(*) AS \"total!\" \
             FROM sieve_scripts WHERE tenant_id = $1 AND user_id = $2",
            self.tenant().as_str(),
            self.user().as_str(),
            name
        )
        .fetch_one(&self.pool)
        .await?;
        if existing.has == 0 && existing.total >= MAX_SCRIPTS {
            return Err(StoreError::Conflict(format!(
                "too many scripts (max {MAX_SCRIPTS})"
            )));
        }
        let id = crate::id::MailboxId::generate(); // opaque id generator reuse
        sqlx::query!(
            "INSERT INTO sieve_scripts (id, tenant_id, user_id, name, content) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (tenant_id, user_id, name) \
             DO UPDATE SET content = $5, updated_at = now()",
            id.as_str(),
            self.tenant().as_str(),
            self.user().as_str(),
            name,
            content
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Deletes a script (must not be the active one).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if absent; [`StoreError::Conflict`] if it is
    /// the active script.
    pub async fn delete_sieve_script(&self, name: &str) -> Result<()> {
        let row = sqlx::query!(
            "DELETE FROM sieve_scripts \
             WHERE tenant_id = $1 AND user_id = $2 AND name = $3 AND NOT active \
             RETURNING 1 AS one",
            self.tenant().as_str(),
            self.user().as_str(),
            name
        )
        .fetch_optional(&self.pool)
        .await?;
        if row.is_some() {
            return Ok(());
        }
        // Distinguish "not found" from "is active".
        let exists = sqlx::query!(
            "SELECT active FROM sieve_scripts WHERE tenant_id = $1 AND user_id = $2 AND name = $3",
            self.tenant().as_str(),
            self.user().as_str(),
            name
        )
        .fetch_optional(&self.pool)
        .await?;
        match exists {
            Some(_) => Err(StoreError::Conflict(
                "cannot delete the active script".to_owned(),
            )),
            None => Err(StoreError::NotFound),
        }
    }

    /// Activates a script (deactivating any other), or — with `name = None`
    /// — deactivates all (no filtering). At most one active per account.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] if the named script does not exist.
    pub async fn activate_sieve_script(&self, name: Option<&str>) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        sqlx::query!(
            "UPDATE sieve_scripts SET active = FALSE \
             WHERE tenant_id = $1 AND user_id = $2 AND active",
            self.tenant().as_str(),
            self.user().as_str()
        )
        .execute(&mut *tx)
        .await?;
        if let Some(name) = name {
            let updated = sqlx::query!(
                "UPDATE sieve_scripts SET active = TRUE \
                 WHERE tenant_id = $1 AND user_id = $2 AND name = $3",
                self.tenant().as_str(),
                self.user().as_str(),
                name
            )
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if updated == 0 {
                return Err(StoreError::NotFound);
            }
        }
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// The active script's content, if any.
    async fn active_sieve_script(&self) -> Result<Option<String>> {
        let row = sqlx::query!(
            "SELECT content FROM sieve_scripts \
             WHERE tenant_id = $1 AND user_id = $2 AND active",
            self.tenant().as_str(),
            self.user().as_str()
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.content))
    }

    /// This account's primary email address (for vacation owner tests).
    async fn owner_email(&self) -> Result<String> {
        let row = sqlx::query!(
            "SELECT email FROM users WHERE tenant_id = $1 AND id = $2",
            self.tenant().as_str(),
            self.user().as_str()
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)?;
        Ok(row.email)
    }

    // ---- delivery ------------------------------------------------------

    /// Delivers `raw` through the account's active Sieve script (the delivery
    /// entry SMTP local delivery / migration call). Runs after spam scoring
    /// (the message already carries its trust/spam headers), files store-side
    /// actions into the owner's mailboxes, and returns outbound actions
    /// (redirect / vacation) for the caller to enqueue. **No script failure
    /// loses mail:** a missing/invalid/over-budget script falls back to
    /// implicit keep into the Inbox.
    ///
    /// # Errors
    /// [`StoreError::Db`]/[`StoreError::Blob`] on a store failure (the
    /// message was not accepted; the caller retries).
    pub async fn deliver_sieve(
        &self,
        raw: &[u8],
        envelope_from: Option<&str>,
        envelope_to: &str,
    ) -> Result<SieveDelivery> {
        let Some(content) = self.active_sieve_script().await? else {
            // No active script → straight to the Inbox.
            let inbox = self.inbox().await?;
            self.ingest(&inbox, raw).await?;
            return Ok(SieveDelivery {
                filed: true,
                ..Default::default()
            });
        };

        let owner = self.owner_email().await?;
        let (actions, mut warnings) =
            self.evaluate(&content, raw, envelope_from, envelope_to, &owner);

        // Collect filing targets (deduplicated) and the union of flags.
        let mut targets: Vec<MailboxId> = Vec::new();
        let mut flags: Vec<String> = Vec::new();
        let mut outbound = Vec::new();
        let inbox = self.inbox().await?;

        for action in actions {
            match action {
                Action::Keep { flags: f } => {
                    push_unique(&mut targets, inbox.clone());
                    merge_flags(&mut flags, f);
                }
                Action::FileInto { mailbox, flags: f } => {
                    match self.resolve_mailbox_path(&mailbox).await? {
                        Some(id) => push_unique(&mut targets, id),
                        None => {
                            // Auto-create OFF: degrade to keep into Inbox.
                            warnings.push(format!(
                                "fileinto: mailbox '{mailbox}' does not exist; kept to Inbox"
                            ));
                            push_unique(&mut targets, inbox.clone());
                        }
                    }
                    merge_flags(&mut flags, f);
                }
                Action::Redirect { address } => {
                    if redirect_loop_guarded(raw, envelope_from) {
                        warnings.push("redirect suppressed: loop/auto-submitted guard".to_owned());
                    } else if self.redirect_budget_ok().await? {
                        outbound.push(OutboundAction::Redirect { address });
                    } else {
                        warnings.push("redirect suppressed: account rate budget".to_owned());
                    }
                }
                Action::Vacation(v) => {
                    let handle = v
                        .handle
                        .clone()
                        .unwrap_or_else(|| format!("auto:{:x}", fnv1a(v.reason.as_bytes())));
                    let days = v.days.unwrap_or(DEFAULT_VACATION_DAYS);

                    // The managed out-of-office reply answers only inside the
                    // window the user set. Checked here, where the reply is
                    // decided, rather than by a timer that switches it on and
                    // off: a timer is a second thing that can be down, and down
                    // means either answering for somebody who is at their desk
                    // or staying silent while they are away.
                    //
                    // A `vacation` from a hand-written script is not gated —
                    // that rule fires when its author said it should.
                    if handle == crate::settings::OOO_HANDLE {
                        let ooo = self.out_of_office().await?;
                        if !ooo.active_at(OffsetDateTime::now_utc()) {
                            warnings.push(
                                "vacation suppressed: outside the out-of-office window".to_owned(),
                            );
                            continue;
                        }
                    }

                    if self.vacation_should_send(&handle, &v.to, days).await? {
                        outbound.push(OutboundAction::Vacation {
                            to: v.to,
                            subject: v.subject,
                            from: v.from,
                            reason: v.reason,
                        });
                    } else {
                        warnings
                            .push("vacation suppressed: recent reply to correspondent".to_owned());
                    }
                }
            }
        }

        // File the message: ingest into the first target, add memberships for
        // the rest (one message object, multiple mailboxes), apply flags.
        let mut filed = false;
        if let Some((first, rest)) = targets.split_first() {
            let message = self.ingest(first, raw).await?;
            for mb in rest {
                if self.add_to_mailbox(&message, mb).await.is_err() {
                    warnings.push("could not file a copy into an additional mailbox".to_owned());
                }
            }
            for flag in &flags {
                // Map the IMAP flag (`\Seen`) to the store's JMAP keyword
                // (`$seen`); an unmappable system flag (e.g. `\Recent`) is
                // dropped rather than persisted as a bogus keyword.
                match map_sieve_flag(flag) {
                    Some(kw) => {
                        if self.set_keyword(&message, &kw, true).await.is_err() {
                            warnings.push(format!("could not set flag {flag}"));
                        }
                    }
                    None => warnings.push(format!("flag {flag} is not settable; ignored")),
                }
            }
            filed = true;
        }
        // If no targets, the script discarded the message (nothing filed).

        Ok(SieveDelivery {
            outbound,
            warnings,
            filed,
        })
    }

    /// Compiles + evaluates the active script; any compile/runtime failure
    /// degrades to a single implicit `keep` so mail is never lost.
    fn evaluate(
        &self,
        content: &str,
        raw: &[u8],
        envelope_from: Option<&str>,
        envelope_to: &str,
        owner: &str,
    ) -> (Vec<Action>, Vec<String>) {
        let script = match alo_sieve::compile(content, Limits::default()) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "active Sieve script failed to compile; implicit keep");
                return (
                    vec![Action::Keep { flags: Vec::new() }],
                    vec![format!("active script does not compile: {e}")],
                );
            }
        };
        let msg = SieveMsg::parse(
            raw,
            envelope_from.map(str::to_owned),
            envelope_to.to_owned(),
        );
        let ctx = EvalContext::new(vec![owner.to_owned()]);
        match alo_sieve::evaluate(&script, &msg, &ctx) {
            Ok(outcome) => (outcome.actions, outcome.warnings),
            Err(e) => {
                tracing::warn!(error = %e, "Sieve evaluation aborted; implicit keep");
                (
                    vec![Action::Keep { flags: Vec::new() }],
                    vec![format!("script aborted: {e}")],
                )
            }
        }
    }

    /// Resolves a Sieve `fileinto` mailbox name (IMAP-style `/` hierarchy,
    /// `INBOX` reserved) to one of this account's mailbox ids. `None` if it
    /// does not exist (auto-create is off).
    async fn resolve_mailbox_path(&self, path: &str) -> Result<Option<MailboxId>> {
        let all = self.imap_mailboxes().await?;
        let segments: Vec<&str> = path.split('/').collect();
        let Some(first) = segments.first() else {
            return Ok(None);
        };
        let mut current = if first.eq_ignore_ascii_case("INBOX") {
            all.iter().find(|m| m.role.as_deref() == Some("inbox"))
        } else {
            all.iter().find(|m| {
                m.parent_id.is_none() && m.role.as_deref() != Some("inbox") && m.name == *first
            })
        };
        for seg in &segments[1..] {
            let Some(parent) = current else {
                return Ok(None);
            };
            current = all
                .iter()
                .find(|m| m.parent_id.as_ref() == Some(&parent.id) && m.name == *seg);
        }
        Ok(current.map(|m| m.id.clone()))
    }

    /// Whether a vacation reply may be sent to `correspondent` under `handle`
    /// now (records the send if so). Suppresses a second reply within `days`.
    async fn vacation_should_send(
        &self,
        handle: &str,
        correspondent: &str,
        days: u32,
    ) -> Result<bool> {
        let row = sqlx::query!(
            "INSERT INTO vacation_responses (tenant_id, user_id, handle, correspondent, last_sent) \
             VALUES ($1, $2, $3, $4, now()) \
             ON CONFLICT (tenant_id, user_id, handle, correspondent) \
             DO UPDATE SET last_sent = now() \
             WHERE vacation_responses.last_sent < now() - make_interval(days => $5) \
             RETURNING 1 AS one",
            self.tenant().as_str(),
            self.user().as_str(),
            handle,
            correspondent,
            i32::try_from(days).unwrap_or(i32::MAX)
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.is_some())
    }

    /// Charges one redirect against the account's rolling-window budget;
    /// `false` if over the cap (the redirect is then suppressed).
    async fn redirect_budget_ok(&self) -> Result<bool> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let row = sqlx::query!(
            "INSERT INTO redirect_budget (tenant_id, user_id, window_start, count) \
             VALUES ($1, $2, now(), 0) \
             ON CONFLICT (tenant_id, user_id) DO NOTHING",
            self.tenant().as_str(),
            self.user().as_str()
        );
        row.execute(&mut *tx).await?;
        let current = sqlx::query!(
            "SELECT count, \
             EXTRACT(EPOCH FROM (now() - window_start))::bigint AS \"age!\" \
             FROM redirect_budget WHERE tenant_id = $1 AND user_id = $2 FOR UPDATE",
            self.tenant().as_str(),
            self.user().as_str()
        )
        .fetch_one(&mut *tx)
        .await?;
        let ok = if current.age > REDIRECT_WINDOW_SECS {
            // Window elapsed: reset to a fresh window with this redirect.
            sqlx::query!(
                "UPDATE redirect_budget SET window_start = now(), count = 1 \
                 WHERE tenant_id = $1 AND user_id = $2",
                self.tenant().as_str(),
                self.user().as_str()
            )
            .execute(&mut *tx)
            .await?;
            true
        } else if current.count < REDIRECT_CAP {
            sqlx::query!(
                "UPDATE redirect_budget SET count = count + 1 \
                 WHERE tenant_id = $1 AND user_id = $2",
                self.tenant().as_str(),
                self.user().as_str()
            )
            .execute(&mut *tx)
            .await?;
            true
        } else {
            false
        };
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(ok)
    }
}

/// Loop guard for redirect (independent of the engine's self-redirect
/// check): never redirect a message that is auto-submitted, has a null
/// return path, or has already traversed too many hops.
fn redirect_loop_guarded(raw: &[u8], envelope_from: Option<&str>) -> bool {
    if envelope_from.map(str::trim).unwrap_or("").is_empty() {
        return true; // null return-path
    }
    let msg = SieveMsg::parse(raw, None, String::new());
    if let Some(v) = msg.header_values("Auto-Submitted").first()
        && !v.trim().eq_ignore_ascii_case("no")
    {
        return true;
    }
    // A message that has already been forwarded many times is dropped.
    let received = msg.header_values("Received").len();
    received > 25
}

fn push_unique(v: &mut Vec<MailboxId>, id: MailboxId) {
    if !v.contains(&id) {
        v.push(id);
    }
}

fn merge_flags(into: &mut Vec<String>, from: Vec<String>) {
    for f in from {
        if !into.contains(&f) {
            into.push(f);
        }
    }
}

/// Maps a Sieve/IMAP flag to the store's JMAP keyword (RFC 8621 §4.1.1):
/// system flags become their `$`-keyword, `\Recent`/unknown `\`-flags are
/// unsettable (`None`), and a custom keyword is lowercased. Mirrors
/// `alo-imap`'s mapping (the store cannot depend on the imap crate).
fn map_sieve_flag(flag: &str) -> Option<String> {
    const SYSTEM: &[(&str, &str)] = &[
        ("\\seen", "$seen"),
        ("\\flagged", "$flagged"),
        ("\\answered", "$answered"),
        ("\\draft", "$draft"),
        ("\\deleted", "$deleted"),
    ];
    let lower = flag.to_ascii_lowercase();
    if let Some((_, kw)) = SYSTEM.iter().find(|(imap, _)| *imap == lower) {
        return Some((*kw).to_owned());
    }
    if flag.starts_with('\\') {
        return None; // \Recent and other unsettable system flags
    }
    Some(lower)
}

/// FNV-1a for a stable default vacation handle from the reason text.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}
