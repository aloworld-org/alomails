//! Inbound local delivery: carry a received message the last hop, SMTP →
//! `alo-store`, with Sieve at the boundary. The one place `alo-smtp`
//! meets the store and the outbound queue. Enabled only on the MX role when
//! a database URL is configured; the submission role and the outbound queue
//! are untouched. See `docs/design/local-delivery.md`.

use std::sync::Arc;

use alo_store::{OutboundAction, Store};

use crate::authmail::AuthMail;
use crate::envelope::Envelope;
use crate::error::SmtpError;
use crate::spool::Spool;

/// The result of delivering one message to its local recipients.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryOutcome {
    /// Every recipient was filed (or intentionally discarded by Sieve).
    Delivered,
    /// A transient store/blob failure — the message is NOT accepted, so the
    /// sender must retry (no mail loss). Maps to a `4xx` at end of DATA.
    Transient,
}

/// Local delivery into the account store.
pub struct LocalDelivery {
    store: Arc<Store>,
    /// The inbound spool — reused to *enqueue* Sieve's outbound actions
    /// (redirect/vacation) for the existing outbound queue runner.
    spool: Arc<Spool>,
    hostname: String,
    /// ARC sealer (RFC 8617) for Sieve redirects: the trust-stack
    /// context holding the signing keys. `None` disables sealing
    /// (forwards then break downstream DMARC — dev/test only).
    sealer: Option<Arc<AuthMail>>,
    /// The campaign return path (M4.4): the one address whose mail is a
    /// bounce intake rather than a mailbox. Lowercased at construction;
    /// `None` disables the intake (today's behaviour byte-for-byte).
    campaign_return_path: Option<String>,
}

impl LocalDelivery {
    /// Connects the store for local delivery.
    ///
    /// # Errors
    /// [`SmtpError::Config`] if the blob directory or store cannot be opened.
    pub async fn connect(
        database_url: &str,
        blob_dir: &std::path::Path,
        spool: Arc<Spool>,
        hostname: String,
    ) -> Result<Self, SmtpError> {
        // A DURABLE on-disk blob backend so a delivered body survives a
        // restart (multi-node production swaps in Garage/S3 behind the
        // store's `garage` feature).
        let blobs = alo_store::BlobStore::local(blob_dir, 50 * 1024 * 1024).map_err(|e| {
            SmtpError::Config {
                message: format!("local delivery: cannot open the blob store: {e}"),
            }
        })?;
        let store = Store::connect(database_url, blobs)
            .await
            .map_err(|e| SmtpError::Config {
                message: format!("local delivery: cannot connect to the store: {e}"),
            })?;
        store.migrate().await.map_err(|e| SmtpError::Config {
            message: format!("local delivery: store migration failed: {e}"),
        })?;
        Ok(Self::from_store(Arc::new(store), spool, hostname))
    }

    /// Builds local delivery over an existing store (used by embedders and
    /// tests that already hold a `Store`, so the same pool and blob backend
    /// are shared).
    pub fn from_store(store: Arc<Store>, spool: Arc<Spool>, hostname: String) -> Self {
        Self {
            store,
            spool,
            hostname,
            sealer: None,
            campaign_return_path: None,
        }
    }

    /// Installs the trust-stack context whose keys ARC-seal Sieve
    /// redirects (RFC 8617). Without it, forwards go out unsealed.
    #[must_use]
    pub fn with_arc_sealer(mut self, sealer: Arc<AuthMail>) -> Self {
        self.sealer = Some(sealer);
        self
    }

    /// Routes one address to the campaign bounce intake instead of a
    /// mailbox (M4.4). `None` disables the intake.
    #[must_use]
    pub fn with_campaign_return_path(mut self, address: Option<String>) -> Self {
        self.campaign_return_path = address.map(|a| a.to_ascii_lowercase());
        self
    }

    /// Whether `email` is the campaign return path. Case-insensitive, and
    /// subaddress-tolerant the way every other local mailbox is (RFC 5233):
    /// `bounces+anything@…` reaches the same intake, so a future
    /// per-recipient return path (VERP, roadmap C2.10) is already
    /// deliverable without another RCPT change.
    fn is_campaign_return_path(&self, email: &str) -> bool {
        let Some(bounce) = &self.campaign_return_path else {
            return false;
        };
        let email = email.to_ascii_lowercase();
        &email == bounce || strip_subaddress(&email).is_some_and(|base| &base == bounce)
    }

    /// The shared store handle, so the submission role can build a
    /// `alo-identity` over the same pool.
    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    /// Whether `email` is a real local mailbox (for the RCPT-time check).
    /// Subaddress-aware: `user+tag@domain` resolves to `user@domain`.
    pub async fn recipient_exists(&self, email: &str) -> bool {
        // The campaign return path is deliverable by configuration, not by
        // having a mailbox — its delivery is the bounce intake.
        if self.is_campaign_return_path(email) {
            return true;
        }
        // A user/alias, or a distribution-list address with at least one member.
        matches!(self.resolve_recipients(email).await, Ok(targets) if !targets.is_empty())
    }

    /// Resolves a recipient to its account, trying the address as-is then
    /// with any `+detail` stripped (subaddress, RFC 5233 — the mailbox is
    /// `user@domain`, the `+tag` is delivery detail the Sieve script tests).
    async fn resolve_account(
        &self,
        email: &str,
    ) -> Result<Option<(alo_store::TenantId, alo_store::UserId)>, alo_store::StoreError> {
        if let Some(ids) = self.store.account_by_email(email).await? {
            return Ok(Some(ids));
        }
        match strip_subaddress(email) {
            Some(base) => self.store.account_by_email(&base).await,
            None => Ok(None),
        }
    }

    /// Expands a recipient to its target account(s): one for a user/alias, or
    /// the member accounts for a distribution-list address. Empty means the
    /// address matches nothing local.
    async fn resolve_recipients(
        &self,
        email: &str,
    ) -> Result<Vec<(alo_store::TenantId, alo_store::UserId)>, alo_store::StoreError> {
        if let Some(ids) = self.resolve_account(email).await? {
            return Ok(vec![ids]);
        }
        // Not a user/alias — a distribution-list address fans out to members.
        self.store.list_members_by_address(email).await
    }

    /// Delivers `message` to each local recipient through the account's
    /// Sieve script. A list address fans out to every member's inbox.
    /// Per-target and independent; a transient store fault for **any** target
    /// returns [`DeliveryOutcome::Transient`] (the conservative multi-recipient
    /// reply — RFC 5321 §6.1, duplicate over loss). Sieve's outbound actions
    /// are enqueued for the outbound queue.
    pub async fn deliver(
        &self,
        message: &[u8],
        mail_from: Option<&str>,
        rcpts: &[String],
    ) -> DeliveryOutcome {
        for rcpt in rcpts {
            // The campaign return path is not a mailbox: its delivery is the
            // bounce intake (M4.4). A store fault defers exactly like a
            // mailbox delivery fault — the sender retries, and the intake's
            // suppress-first ordering makes the retry safe.
            if self.is_campaign_return_path(rcpt) {
                match crate::bounce_intake::intake_campaign_bounce(&self.store, message).await {
                    Ok(_receipt) => continue,
                    Err(error) => {
                        tracing::error!(%error, "campaign bounce intake failed; deferring");
                        return DeliveryOutcome::Transient;
                    }
                }
            }
            let targets = match self.resolve_recipients(rcpt).await {
                Ok(targets) if !targets.is_empty() => targets,
                // Accepted at RCPT but gone now (rare TOCTOU), or a DB error:
                // transient, so the sender retries rather than lose the mail.
                Ok(_) => {
                    tracing::warn!("recipient not found at DATA time; deferring");
                    return DeliveryOutcome::Transient;
                }
                Err(error) => {
                    tracing::error!(%error, "recipient lookup failed; deferring");
                    return DeliveryOutcome::Transient;
                }
            };
            for (tenant, user) in targets {
                if !self
                    .deliver_one(tenant, user, message, mail_from, rcpt)
                    .await
                {
                    return DeliveryOutcome::Transient;
                }
            }
        }
        DeliveryOutcome::Delivered
    }

    /// Files `message` into one account via its Sieve script and enqueues any
    /// outbound actions. Returns `false` on a transient store fault (the caller
    /// defers the whole message).
    async fn deliver_one(
        &self,
        tenant: alo_store::TenantId,
        user: alo_store::UserId,
        message: &[u8],
        mail_from: Option<&str>,
        rcpt: &str,
    ) -> bool {
        let acc = self.store.for_account(tenant, user);
        match acc.deliver_sieve(message, mail_from, rcpt).await {
            Ok(delivery) => {
                for warning in &delivery.warnings {
                    tracing::info!(warning = %warning, "sieve delivery warning");
                }
                // Enqueue redirect/vacation. An enqueue failure does NOT defer
                // the whole message — the message IS filed; the outbound action
                // is best-effort and logged.
                for action in delivery.outbound {
                    if let Err(error) = self.enqueue(action, message, mail_from, rcpt).await {
                        tracing::error!(%error, "failed to enqueue sieve outbound action");
                    }
                }
                true
            }
            Err(error) => {
                tracing::error!(%error, "store delivery failed; deferring");
                false
            }
        }
    }

    /// Enqueues a Sieve outbound action into the spool for the outbound
    /// queue runner. Attacker-influenced strings are CR/LF-stripped before
    /// any header is built (injection guard).
    async fn enqueue(
        &self,
        action: OutboundAction,
        message: &[u8],
        mail_from: Option<&str>,
        owner: &str,
    ) -> std::io::Result<()> {
        let (envelope, body) = match action {
            OutboundAction::Redirect { address } => {
                // Forward the original message; keep the original return-path
                // (avoids backscatter). The message already carries a
                // Received: stamp, so the store's loop ceiling bites on any
                // cycle back through us.
                let envelope = Envelope {
                    helo: self.hostname.clone(),
                    peer: "local-delivery".to_owned(),
                    mail_from: mail_from.map(str::to_owned),
                    rcpt_to: vec![strip_crlf(&address)],
                    received_at: jiff::Timestamp::now().to_string(),
                };
                // ARC-seal the forward (RFC 8617): SPF breaks at the
                // next hop, so attest the verdicts we computed at
                // ingress. Sealed with the forwarding account's domain
                // key; any failure forwards unsealed (mail flows).
                let body = match &self.sealer {
                    Some(auth) => {
                        let domain = owner.rsplit_once('@').map(|(_, d)| d).unwrap_or_default();
                        match auth.seal_arc(message, domain).await {
                            Some(set) => {
                                let mut sealed = Vec::with_capacity(set.len() + message.len());
                                sealed.extend_from_slice(set.as_bytes());
                                sealed.extend_from_slice(message);
                                sealed
                            }
                            None => message.to_vec(),
                        }
                    }
                    None => message.to_vec(),
                };
                (envelope, body)
            }
            OutboundAction::Vacation {
                to,
                subject,
                from,
                reason,
            } => {
                let body =
                    build_vacation_reply(&to, subject.as_deref(), from.as_deref(), owner, &reason);
                // Null return-path (RFC 3834 §5) so the auto-reply can never
                // itself trigger a bounce loop.
                let envelope = Envelope {
                    helo: self.hostname.clone(),
                    peer: "local-delivery".to_owned(),
                    mail_from: None,
                    rcpt_to: vec![strip_crlf(&to)],
                    received_at: jiff::Timestamp::now().to_string(),
                };
                (envelope, body)
            }
        };
        let id = self.spool.next_id();
        let spool = Arc::clone(&self.spool);
        tokio::task::spawn_blocking(move || spool.store(&id, &envelope, &body))
            .await
            .map_err(std::io::Error::other)?
    }

    /// One-shot startup migration: any spooled message destined **entirely**
    /// for local recipients is delivered into the store and removed from the
    /// spool. Must run **before** the outbound queue runner starts (so there
    /// is no concurrent claim). Entries with a non-local recipient are left
    /// for the outbound queue. Returns the count migrated.
    ///
    /// # Errors
    /// [`std::io::Error`] if the spool cannot be listed.
    pub async fn migrate_spool(&self) -> std::io::Result<usize> {
        let ids = self.spool.list()?;
        let mut migrated = 0;
        for id in ids {
            let (envelope, message) = match self.spool.read(&id) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if envelope.rcpt_to.is_empty() {
                continue;
            }
            // All recipients must be local to migrate (else it is outbound).
            let mut all_local = true;
            for rcpt in &envelope.rcpt_to {
                if !self.recipient_exists(rcpt).await {
                    all_local = false;
                    break;
                }
            }
            if !all_local {
                continue;
            }
            if self
                .deliver(&message, envelope.mail_from.as_deref(), &envelope.rcpt_to)
                .await
                == DeliveryOutcome::Delivered
            {
                // Remove from the spool: claim (new→cur) then complete.
                if self.spool.claim(&id).is_ok() {
                    let _ = self.spool.complete(&id);
                    migrated += 1;
                }
            }
        }
        if migrated > 0 {
            tracing::info!(migrated, "migrated spooled local mail into the store");
        }
        Ok(migrated)
    }
}

/// Strips a `+detail` subaddress: `user+tag@domain` → `user@domain`.
/// `None` if there is no `+` in the local part.
fn strip_subaddress(email: &str) -> Option<String> {
    let (local, domain) = email.split_once('@')?;
    let (user, _tag) = local.split_once('+')?;
    Some(format!("{user}@{domain}"))
}

/// Strips CR and LF (and other controls) from a header/envelope value so an
/// attacker-influenced Sieve string cannot inject a header or SMTP command.
fn strip_crlf(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

/// Builds a vacation auto-reply message. All header values are CR/LF-safe.
fn build_vacation_reply(
    to: &str,
    subject: Option<&str>,
    from: Option<&str>,
    owner: &str,
    reason: &str,
) -> Vec<u8> {
    let from_hdr = strip_crlf(from.unwrap_or(owner));
    let to_hdr = strip_crlf(to);
    let subject_hdr = strip_crlf(subject.unwrap_or("Automatic reply"));
    let date = jiff::Zoned::now().strftime("%a, %d %b %Y %H:%M:%S %z");
    format!(
        "From: {from_hdr}\r\n\
         To: {to_hdr}\r\n\
         Subject: {subject_hdr}\r\n\
         Date: {date}\r\n\
         Auto-Submitted: auto-replied\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         \r\n\
         {reason}\r\n"
    )
    .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crlf_is_stripped_from_headers() {
        assert_eq!(strip_crlf("hi\r\nBcc: evil@x"), "hiBcc: evil@x");
        assert_eq!(strip_crlf("clean subject"), "clean subject");
    }

    #[test]
    fn vacation_reply_headers_are_injection_safe() {
        let body = build_vacation_reply(
            "victim@x.test\r\nBcc: leak@evil.test",
            Some("Re:\r\nX-Injected: yes"),
            None,
            "owner@x.test",
            "I am away",
        );
        let text = String::from_utf8_lossy(&body);
        // The injected content must not become a new header LINE (CR/LF
        // stripped, so it can only survive inline within a value).
        assert!(!text.contains("\r\nBcc:"), "{text}");
        assert!(!text.contains("\r\nX-Injected:"), "{text}");
        assert!(text.contains("Auto-Submitted: auto-replied"));
        assert!(text.contains("From: owner@x.test"));
    }
}
