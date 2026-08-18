//! The outbound queue: claims spooled messages, resolves and delivers
//! them, tracks per-recipient state durably, retries with backoff,
//! and generates DSNs for permanent failures.
//!
//! Relay safety (M2 design decision): delivery is off by default and
//! this type is only constructed when outbound is explicitly enabled,
//! because M1 accepts any recipient — turning on delivery without the
//! AUTH gate (M3) would make an exposed instance an open relay. A
//! smarthost route is the supported self-hosted mode and the test
//! seam.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::backoff;
use crate::client::{DeliveryError, OutboundSession, RcptOutcome, SMTP_PORT, TlsRequirement};
use crate::dsn::{self, FailedRecipient};
use crate::egress::EgressMap;
use crate::envelope::Envelope;
use crate::resolver::{MailHost, MxResolve, ResolveFailure};
use crate::spool::Spool;

/// Where outbound mail is sent.
#[derive(Debug, Clone)]
pub enum Route {
    /// Normal MX-based delivery to each recipient's domain.
    Mx,
    /// All mail relayed to one host (self-hosted smarthost / test).
    Smarthost(SocketAddr),
}

/// Tunable delivery policy.
#[derive(Debug, Clone)]
pub struct QueuePolicy {
    /// Our hostname for outbound EHLO and DSN authorship.
    pub hostname: String,
    /// Delivery route.
    pub route: Route,
    /// First-retry base delay.
    pub retry_base: Duration,
    /// Retry delay cap.
    pub retry_cap: Duration,
    /// Attempts before a transient failure becomes permanent (DSN).
    pub max_attempts: u32,
    /// Outbound send rate per destination domain (messages/minute; `0`
    /// disables). Protects the sending IP's reputation.
    pub rate_per_min: u32,
    /// Burst depth for the send-rate limiter.
    pub rate_burst: u32,
    /// Which source address a message leaves by, chosen from its envelope
    /// sender (ADR 0044 §1). Empty means the kernel chooses.
    pub egress: EgressMap,
}

/// Per-recipient delivery progress, persisted in the state sidecar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum RcptState {
    Pending,
    Delivered,
    /// Permanently failed; carries the DSN diagnostic.
    Failed {
        status: String,
        diagnostic: String,
    },
    /// No domain to route to yet (e.g. bare `postmaster`) — parked
    /// until the local store exists (M5).
    LocalPending,
}

/// The durable queue state for one message (versioned sidecar).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MessageState {
    version: u16,
    attempts: u32,
    /// Unix seconds before which the next attempt must not run; absent
    /// on a never-attempted message. Lets a deferred message wait in
    /// `cur/` without being retried early.
    #[serde(default)]
    next_attempt_at: Option<i64>,
    /// Set once a DSN has been (or is about to be) enqueued for this
    /// message's failures. Persisted BEFORE the DSN is stored so a
    /// crash-recovery pass skips re-enqueue — at-most-once bounces,
    /// never a bounce storm (the residual risk is a single lost DSN if
    /// we crash in the sub-op window, preferred over duplicates).
    #[serde(default)]
    dsn_enqueued: bool,
    recipients: BTreeMap<String, RcptState>,
}

impl MessageState {
    const VERSION: u16 = 1;

    fn new(rcpts: &[String]) -> Self {
        let recipients = rcpts
            .iter()
            .map(|r| {
                let state = if r.contains('@') {
                    RcptState::Pending
                } else {
                    RcptState::LocalPending
                };
                (r.clone(), state)
            })
            .collect();
        Self {
            version: Self::VERSION,
            attempts: 0,
            next_attempt_at: None,
            dsn_enqueued: false,
            recipients,
        }
    }

    fn is_due(&self, now_secs: i64) -> bool {
        self.next_attempt_at.is_none_or(|due| now_secs >= due)
    }

    fn pending_by_domain(&self) -> BTreeMap<String, Vec<String>> {
        let mut by_domain: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (rcpt, state) in &self.recipients {
            if *state == RcptState::Pending
                && let Some(domain) = rcpt.rsplit('@').next()
            {
                by_domain
                    .entry(domain.to_owned())
                    .or_default()
                    .push(rcpt.clone());
            }
        }
        by_domain
    }

    /// True once every recipient reached a terminal state (delivered
    /// or permanently failed) — the message can be finished. A
    /// `LocalPending` recipient is NOT terminal: it is parked for M5,
    /// and finishing would drop it (design note: never silently
    /// dropped).
    fn all_terminal(&self) -> bool {
        self.recipients
            .values()
            .all(|s| matches!(s, RcptState::Delivered | RcptState::Failed { .. }))
    }

    /// True while any recipient still needs a delivery attempt.
    fn has_active_pending(&self) -> bool {
        self.recipients.values().any(|s| *s == RcptState::Pending)
    }

    /// True when the only non-terminal recipients are local (parked
    /// for M5) — the message is held, not retried or dropped.
    fn is_parked(&self) -> bool {
        !self.all_terminal()
            && !self.has_active_pending()
            && self
                .recipients
                .values()
                .any(|s| *s == RcptState::LocalPending)
    }

    fn failures(&self) -> Vec<FailedRecipient> {
        self.recipients
            .iter()
            .filter_map(|(rcpt, state)| match state {
                RcptState::Failed { status, diagnostic } => Some(FailedRecipient {
                    recipient: rcpt.clone(),
                    status: status.clone(),
                    diagnostic: diagnostic.clone(),
                }),
                _ => None,
            })
            .collect()
    }
}

/// The outbound queue.
pub struct Queue {
    spool: Arc<Spool>,
    resolver: Arc<dyn MxResolve>,
    policy: QueuePolicy,
    /// Per-destination outbound send-rate limiter (RFC-neutral abuse
    /// control); disabled when `policy.rate_per_min == 0`.
    send_rate: crate::sendrate::SendRateLimiter,
}

/// What one processing pass did (for logging/tests).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PassReport {
    /// Messages fully delivered and removed this pass.
    pub delivered: usize,
    /// Messages that bounced (DSN generated) and were removed.
    pub bounced: usize,
    /// Messages left pending for a later pass.
    pub deferred: usize,
}

impl Queue {
    /// Builds a queue. Construction implies outbound is enabled — the
    /// caller is responsible for the relay-safety gate.
    pub fn new(spool: Arc<Spool>, resolver: Arc<dyn MxResolve>, policy: QueuePolicy) -> Self {
        let send_rate =
            crate::sendrate::SendRateLimiter::new(policy.rate_per_min, policy.rate_burst);
        Self {
            spool,
            resolver,
            policy,
            send_rate,
        }
    }

    /// Processes fresh and due-for-retry messages once: claims new
    /// arrivals, picks up deferred messages in `cur/` whose backoff
    /// has elapsed, attempts delivery, updates durable state, and
    /// either completes, bounces, or defers each.
    ///
    /// # Errors
    /// Only spool I/O errors that prevent listing; per-message
    /// delivery failures are handled as state transitions, not errors.
    pub async fn process_once(&self) -> std::io::Result<PassReport> {
        let now_secs = jiff::Timestamp::now().as_second();
        let mut report = PassReport::default();

        // Fresh arrivals: claim new/ -> cur/ (at-most-once; a lost
        // race just means another worker took it).
        for id in self.spool.list()? {
            if self.spool.claim(&id).is_err() {
                continue;
            }
            self.dispatch(&id, now_secs, &mut report).await;
        }

        // Deferred messages already in cur/, due for another attempt.
        for id in self.spool.list_claimed()? {
            let due = match self.spool.read_state(&id)? {
                Some(bytes) => match serde_json::from_slice::<MessageState>(&bytes) {
                    Ok(state) => state.is_due(now_secs),
                    // Unreadable state: attempt it rather than stall.
                    Err(_) => true,
                },
                // Claimed but no state yet (crash between claim and
                // first persist): pick it up.
                None => true,
            };
            if due {
                self.dispatch(&id, now_secs, &mut report).await;
            }
        }
        Ok(report)
    }

    async fn dispatch(&self, id: &str, now_secs: i64, report: &mut PassReport) {
        match self.process_claimed(id, now_secs).await {
            Ok(Disposition::Delivered) => report.delivered += 1,
            Ok(Disposition::Bounced) => report.bounced += 1,
            Ok(Disposition::Deferred) => report.deferred += 1,
            Err(error) => {
                // Leave it claimed for the next pass; never lose it.
                tracing::error!(%id, %error, "message processing failed; deferring");
                report.deferred += 1;
            }
        }
    }

    async fn process_claimed(&self, id: &str, now_secs: i64) -> std::io::Result<Disposition> {
        let (envelope, message) = self.spool.read_claimed(id)?;
        let mut state = self.load_or_init_state(id, &envelope)?;

        // A message with no deliverable recipients (only local ones
        // left) is parked for M5 — held in the spool, logged, never
        // retried or dropped. No attempt is spent on it.
        if !state.has_active_pending() {
            if state.all_terminal() {
                self.persist_state(id, &state)?;
                let bounced = !state.failures().is_empty();
                self.finish(id, &envelope, &message, &mut state)?;
                return Ok(if bounced {
                    Disposition::Bounced
                } else {
                    Disposition::Delivered
                });
            }
            return self.park(id, &mut state, now_secs);
        }

        // Send-rate limiting (outbound abuse control): a domain over its
        // rate is skipped this pass — its recipients stay Pending, no
        // attempt is spent — so a burst is smoothed to a steady rate
        // instead of bounced. An attempt is only counted when at least
        // one domain was actually contacted.
        let mut attempted = false;
        let mut rate_deferred = false;
        for (domain, rcpts) in state.pending_by_domain() {
            if !self.send_rate.try_acquire(&domain, now_secs) {
                tracing::info!(%domain, "outbound send rate reached; deferring this domain");
                rate_deferred = true;
                continue;
            }
            attempted = true;
            let outcomes = self
                .deliver_to_domain(&domain, &envelope, &message, &rcpts)
                .await;
            for (rcpt, outcome) in outcomes {
                state.recipients.insert(rcpt, outcome);
            }
        }
        if attempted {
            state.attempts += 1;
        }

        if state.all_terminal() {
            self.persist_state(id, &state)?;
            let bounced = !state.failures().is_empty();
            self.finish(id, &envelope, &message, &mut state)?;
            return Ok(if bounced {
                Disposition::Bounced
            } else {
                Disposition::Delivered
            });
        }

        // Every pending domain was rate-limited (nothing contacted):
        // reschedule soon, without spending an attempt or applying
        // exponential backoff, so throughput tracks the configured rate
        // rather than decaying. A message can never bounce from rate
        // limiting alone (no attempt was counted).
        if !attempted && rate_deferred {
            const RATE_RETRY_SECS: i64 = 30;
            state.next_attempt_at = Some(now_secs.saturating_add(RATE_RETRY_SECS));
            self.persist_state(id, &state)?;
            return Ok(Disposition::Deferred);
        }

        // Out of attempts: expire whatever is still pending, then
        // either finish (if now all-terminal) or park. A mixed message
        // (failures + local recipients) is held whole until M5 rather
        // than DSN'd piecemeal — its failures bounce when finish() runs
        // after local delivery exists. Held, logged, never dropped.
        if state.attempts >= self.policy.max_attempts {
            self.expire_pending(&mut state);
            if state.all_terminal() {
                self.persist_state(id, &state)?;
                self.finish(id, &envelope, &message, &mut state)?;
                return Ok(Disposition::Bounced);
            }
            return self.park(id, &mut state, now_secs);
        }

        let delay = self.retry_delay(id, state.attempts);
        state.next_attempt_at = Some(now_secs.saturating_add(delay.as_secs() as i64));
        self.persist_state(id, &state)?;
        Ok(Disposition::Deferred)
    }

    /// Parks a message that has only local (M5) recipients left:
    /// schedules a slow revisit so it neither hot-loops nor is lost.
    fn park(
        &self,
        id: &str,
        state: &mut MessageState,
        now_secs: i64,
    ) -> std::io::Result<Disposition> {
        const PARK_REVISIT: i64 = 3600;
        if !state.is_parked() {
            tracing::error!(%id, "park called on a non-parked message — invariant broken");
        }
        tracing::info!(%id, "message held for local delivery (M5); not retried or dropped");
        state.next_attempt_at = Some(now_secs.saturating_add(PARK_REVISIT));
        self.persist_state(id, state)?;
        Ok(Disposition::Deferred)
    }

    /// Resolves a domain and delivers to its recipients, mapping the
    /// results to per-recipient states. Transient problems leave
    /// recipients `Pending` (retry); permanent ones set `Failed`.
    async fn deliver_to_domain(
        &self,
        domain: &str,
        envelope: &Envelope,
        message: &[u8],
        rcpts: &[String],
    ) -> Vec<(String, RcptState)> {
        let hosts = match &self.policy.route {
            Route::Smarthost(_addr) => Vec::new(), // unused; connect below
            Route::Mx => match self.resolver.resolve(domain).await {
                Ok(hosts) => hosts,
                Err(ResolveFailure::Permanent { reason }) => {
                    return fail_all(rcpts, "5.1.2", &reason);
                }
                Err(ResolveFailure::Transient { reason }) => {
                    tracing::info!(%domain, %reason, "transient resolution; will retry");
                    return keep_pending(rcpts);
                }
            },
        };

        match self
            .connect_and_deliver(&hosts, envelope, message, rcpts)
            .await
        {
            Ok(outcomes) => rcpts
                .iter()
                .cloned()
                .zip(outcomes)
                .map(|(rcpt, outcome)| (rcpt, outcome_to_state(&outcome)))
                .collect(),
            Err(DeliveryError::Rejected { reply, .. }) if !reply.is_transient() => {
                // A pre-RCPT 5xx (e.g. bad MAIL) fails the whole batch.
                fail_all(
                    rcpts,
                    &reply_status(&reply),
                    &format!("smtp; {} {}", reply.code, reply.first_line()),
                )
            }
            Err(error) => {
                tracing::info!(%domain, %error, "transient delivery failure; will retry");
                keep_pending(rcpts)
            }
        }
    }

    async fn connect_and_deliver(
        &self,
        hosts: &[MailHost],
        envelope: &Envelope,
        message: &[u8],
        rcpts: &[String],
    ) -> Result<Vec<RcptOutcome>, DeliveryError> {
        // The sending identity's own address, when it has one. Read from the
        // envelope sender because that is the identity SPF is evaluated for.
        let source = self.policy.egress.source_for(envelope.mail_from.as_deref());
        let mut dane = "opportunistic";
        let mut session = match &self.policy.route {
            Route::Smarthost(addr) => {
                OutboundSession::connect_addr(*addr, &self.policy.hostname, source).await?
            }
            Route::Mx => {
                let mut last = DeliveryError::Connect {
                    host: "no hosts".to_owned(),
                    reason: "empty MX list".to_owned(),
                };
                let mut connected = None;
                for host in hosts {
                    match OutboundSession::connect(
                        &host.host,
                        &host.ips,
                        SMTP_PORT,
                        &self.policy.hostname,
                        host.tls.clone(),
                        source,
                    )
                    .await
                    {
                        Ok(session) => {
                            dane = match &host.tls {
                                TlsRequirement::Opportunistic => "opportunistic",
                                TlsRequirement::Required => "tls-required",
                                TlsRequirement::DaneEe(_) => "dane-verified",
                            };
                            connected = Some(session);
                            break;
                        }
                        Err(error) => last = error,
                    }
                }
                match connected {
                    Some(session) => session,
                    None => return Err(last),
                }
            }
        };

        tracing::info!(
            tls = session.is_tls(),
            dane,
            rcpts = rcpts.len(),
            egress = source.map(|ip| ip.to_string()).unwrap_or_default(),
            "delivering outbound"
        );
        let result = session
            .deliver(envelope.mail_from.as_deref(), rcpts, message)
            .await;
        session.quit().await;
        result
    }

    fn load_or_init_state(&self, id: &str, envelope: &Envelope) -> std::io::Result<MessageState> {
        match self.spool.read_state(id)? {
            Some(bytes) => match serde_json::from_slice(&bytes) {
                Ok(state) => Ok(state),
                // A corrupt sidecar must not wedge the message in an
                // error-defer loop forever (the due-check already
                // treats unreadable state as "attempt it"). Rebuild
                // from the envelope and start over rather than stall.
                Err(error) => {
                    tracing::error!(%id, %error, "state sidecar unreadable; rebuilding from envelope");
                    Ok(MessageState::new(&envelope.rcpt_to))
                }
            },
            None => Ok(MessageState::new(&envelope.rcpt_to)),
        }
    }

    fn persist_state(&self, id: &str, state: &MessageState) -> std::io::Result<()> {
        let bytes = serde_json::to_vec_pretty(state).map_err(std::io::Error::other)?;
        self.spool.write_state(id, &bytes)
    }

    /// Finalizes a settled message: bounce any failures, then remove.
    ///
    /// Idempotent across a crash: the `dsn_enqueued` flag is persisted
    /// before the DSN is stored, so re-entering `finish` on a recovery
    /// pass will not enqueue a second bounce.
    fn finish(
        &self,
        id: &str,
        envelope: &Envelope,
        message: &[u8],
        state: &mut MessageState,
    ) -> std::io::Result<()> {
        let failures = state.failures();
        if !failures.is_empty() && !state.dsn_enqueued {
            // Persist the intent BEFORE creating the DSN so a crash in
            // the window skips re-enqueue on recovery (at-most-once).
            state.dsn_enqueued = true;
            self.persist_state(id, state)?;
            self.enqueue_dsn(id, envelope, message, &failures)?;
        }
        self.spool.complete(id)
    }

    /// Composes a DSN and enqueues it through the spool — unless the
    /// original arrived from the null sender (RFC 5321 §4.5.5: never
    /// bounce a bounce).
    fn enqueue_dsn(
        &self,
        id: &str,
        envelope: &Envelope,
        message: &[u8],
        failures: &[FailedRecipient],
    ) -> std::io::Result<()> {
        let Some(sender) = &envelope.mail_from else {
            tracing::warn!(%id, "null-sender message failed; suppressing DSN (RFC 5321 §4.5.5)");
            return Ok(());
        };
        let now = jiff::Timestamp::now().to_zoned(jiff::tz::TimeZone::UTC);
        let dsn_id = self.spool.next_id();
        let body = dsn::compose(
            &self.policy.hostname,
            sender,
            id,
            failures,
            &dsn::header_block(message),
            &now,
        );
        let dsn_envelope = Envelope {
            helo: self.policy.hostname.clone(),
            peer: "local".to_owned(),
            mail_from: None, // DSNs are sent from <>
            rcpt_to: vec![sender.clone()],
            received_at: now.timestamp().to_string(),
        };
        self.spool.store(&dsn_id, &dsn_envelope, body.as_bytes())?;
        tracing::info!(original = %id, dsn = %dsn_id, "DSN enqueued");
        Ok(())
    }

    fn expire_pending(&self, state: &mut MessageState) {
        let attempts = state.attempts;
        for value in state.recipients.values_mut() {
            if *value == RcptState::Pending {
                *value = RcptState::Failed {
                    status: "4.4.7".to_owned(), // delivery time expired
                    diagnostic: format!("X-alo; delivery gave up after {attempts} attempts"),
                };
            }
        }
    }

    /// Delay before this message should next be attempted, given the
    /// attempt count already recorded (for a scheduler to sleep on).
    pub fn retry_delay(&self, id: &str, attempt: u32) -> Duration {
        backoff::next_delay(id, attempt, self.policy.retry_base, self.policy.retry_cap)
    }
}

enum Disposition {
    Delivered,
    Bounced,
    Deferred,
}

fn outcome_to_state(outcome: &RcptOutcome) -> RcptState {
    match outcome {
        RcptOutcome::Delivered => RcptState::Delivered,
        RcptOutcome::Transient(_) => RcptState::Pending,
        RcptOutcome::Permanent(reply) => RcptState::Failed {
            status: reply_status(reply),
            diagnostic: format!("smtp; {} {}", reply.code, reply.first_line()),
        },
    }
}

fn reply_status(reply: &crate::client_reply::ServerReply) -> String {
    // Reuse the DSN status extraction over the reply's first line.
    let synthetic = FailedRecipient::from_reply("x", reply.code, reply.first_line());
    synthetic.status
}

fn fail_all(rcpts: &[String], status: &str, reason: &str) -> Vec<(String, RcptState)> {
    rcpts
        .iter()
        .map(|r| {
            (
                r.clone(),
                RcptState::Failed {
                    status: status.to_owned(),
                    diagnostic: format!("smtp; {reason}"),
                },
            )
        })
        .collect()
}

fn keep_pending(rcpts: &[String]) -> Vec<(String, RcptState)> {
    rcpts
        .iter()
        .map(|r| (r.clone(), RcptState::Pending))
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn corrupt_state_deserialization_is_recoverable() {
        // A garbage sidecar must not parse; the queue rebuilds from
        // the envelope instead of looping forever (review finding).
        assert!(serde_json::from_slice::<MessageState>(b"{ not json").is_err());
        let rebuilt = MessageState::new(&["a@x.example".to_owned()]);
        assert_eq!(rebuilt.attempts, 0);
        assert!(!rebuilt.dsn_enqueued);
    }

    #[test]
    fn dsn_enqueued_flag_defaults_false_and_round_trips() {
        let mut state = MessageState::new(&["a@x.example".to_owned()]);
        assert!(!state.dsn_enqueued);
        state.dsn_enqueued = true;
        let json = serde_json::to_vec(&state).unwrap();
        let back: MessageState = serde_json::from_slice(&json).unwrap();
        assert!(back.dsn_enqueued, "flag survives to block a second bounce");
    }

    #[test]
    fn state_routes_domainless_recipients_to_local_pending() {
        let state = MessageState::new(&["alice@example.com".to_owned(), "postmaster".to_owned()]);
        assert_eq!(state.recipients["alice@example.com"], RcptState::Pending);
        assert_eq!(state.recipients["postmaster"], RcptState::LocalPending);
        // A routable recipient is still active; the local one is not
        // terminal, so the message is neither done nor parked yet.
        assert!(state.has_active_pending());
        assert!(!state.all_terminal());
        assert!(!state.is_parked());
    }

    #[test]
    fn local_only_message_is_parked_not_dropped() {
        let mut state = MessageState::new(&["postmaster".to_owned()]);
        assert!(!state.has_active_pending());
        assert!(!state.all_terminal(), "must not look done — would drop it");
        assert!(state.is_parked());
        // A delivered + local mix: delivered is terminal, local parks.
        state
            .recipients
            .insert("a@x.example".to_owned(), RcptState::Delivered);
        assert!(state.is_parked());
    }

    #[test]
    fn due_gating_respects_next_attempt() {
        let mut state = MessageState::new(&["a@x.example".to_owned()]);
        assert!(state.is_due(1000), "never-attempted is always due");
        state.next_attempt_at = Some(2000);
        assert!(!state.is_due(1999));
        assert!(state.is_due(2000));
    }

    #[test]
    fn pending_grouped_by_domain() {
        let state = MessageState::new(&[
            "a@x.example".to_owned(),
            "b@x.example".to_owned(),
            "c@y.example".to_owned(),
        ]);
        let by_domain = state.pending_by_domain();
        assert_eq!(by_domain["x.example"].len(), 2);
        assert_eq!(by_domain["y.example"], vec!["c@y.example"]);
    }

    #[test]
    fn state_serialization_round_trips_versioned() {
        let mut state = MessageState::new(&["a@x.example".to_owned()]);
        state.attempts = 3;
        state.recipients.insert(
            "a@x.example".to_owned(),
            RcptState::Failed {
                status: "5.1.1".to_owned(),
                diagnostic: "smtp; 550 no user".to_owned(),
            },
        );
        let json = serde_json::to_vec(&state).unwrap();
        let back: MessageState = serde_json::from_slice(&json).unwrap();
        assert_eq!(back.version, MessageState::VERSION);
        assert_eq!(back.attempts, 3);
        assert_eq!(back.failures().len(), 1);
    }
}
