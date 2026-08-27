//! Bridge from the SMTP transaction to `alo-auth-mail`: at DATA on
//! the MX role it runs SPF + DKIM + DMARC and stamps `Received-SPF` and
//! `Authentication-Results` (the RFC 8601 contract) onto the message,
//! applying the DMARC disposition; at submission it DKIM-signs.
//!
//! Kept separate from the transport (`server`) and the pure auth logic
//! (`alo-auth-mail`) so each has one reason to change.

use std::net::IpAddr;
use std::sync::Arc;

use alo_auth_mail::arc;
use alo_auth_mail::authres::AuthenticationResults;
use alo_auth_mail::dkim::keystore::{
    KeyFuture, KeyStore, KeyStoreError, ed25519_signing_key_from_seed, rsa_signing_key_from_der,
};
use alo_auth_mail::dkim::{self, Message, SignParams};
use alo_auth_mail::dmarc::{self, Disposition, DmarcResult};
use alo_auth_mail::resolver::Resolver;
use alo_auth_mail::spf::{self, Mailbox, SpfQuery};
use alo_store::Store;

use crate::clamav::{ClamVerdict, ClamavClient};
use crate::rspamd::{RspamdAction, RspamdClient, RspamdMeta};

/// A one-key keystore over an already-resolved per-domain DKIM key (ADR 0014),
/// so the shared `dkim::sign` can consume a stored Ed25519 seed. Rebuilds the
/// PKCS#8 key on demand and holds only the short-lived seed.
struct SingleKeyStore {
    domain: String,
    selector: String,
    /// The stored `a=` family. Held rather than assumed, because the same table
    /// now carries both and reading an RSA key as an Ed25519 seed produces a
    /// signature nothing can verify - which surfaces as delivery trouble weeks
    /// later, not as an error here.
    algorithm: String,
    seed: zeroize::Zeroizing<Vec<u8>>,
}

impl KeyStore for SingleKeyStore {
    fn get<'a>(&'a self, domain: &'a str, selector: &'a str) -> KeyFuture<'a> {
        Box::pin(async move {
            if domain.eq_ignore_ascii_case(&self.domain) && selector == self.selector {
                let unusable = |reason: &str| KeyStoreError::Unusable {
                    domain: domain.to_owned(),
                    selector: selector.to_owned(),
                    reason: reason.to_owned(),
                };
                match self.algorithm.as_str() {
                    "rsa" => rsa_signing_key_from_der(&self.seed)
                        .ok_or_else(|| unusable("stored RSA DKIM key unusable")),
                    "ed25519" => ed25519_signing_key_from_seed(&self.seed)
                        .ok_or_else(|| unusable("stored Ed25519 DKIM seed unusable")),
                    // Refused rather than guessed: signing with the wrong
                    // algorithm produces a valid-looking signature that every
                    // verifier rejects.
                    other => Err(unusable(&format!("unknown DKIM algorithm {other:?}"))),
                }
            } else {
                Err(KeyStoreError::NotFound {
                    domain: domain.to_owned(),
                    selector: selector.to_owned(),
                })
            }
        })
    }
}

/// DKIM signing configuration for the submission path.
pub struct SigningConfig {
    /// The key backend.
    pub keys: Arc<dyn KeyStore>,
    /// Signing domain (`d=`).
    pub domain: String,
    /// Selector (`s=`) of the first signature; also the key that seals
    /// ARC (a set is a chain with one `i=` per hop, so ARC never
    /// dual-signs). In a dual-signing deployment the wiring puts the
    /// RSA key here — when two keys are configured, the RSA one leads.
    pub selector: String,
    /// Selector of a second signature (RFC 8463 dual-signing): when
    /// set, every outbound message carries one signature per key, so a
    /// verifier that cannot read Ed25519 still has the RSA one. `None`
    /// signs once — byte-identical to before the second key existed.
    pub second_selector: Option<String>,
}

/// The trust-stack context attached to a listener. `disabled()` (no
/// resolver, no signer) is the default for tests and for a receive-only
/// dev run; `run` installs a real resolver and, on submission, a signer.
pub struct AuthMail {
    hostname: String,
    resolver: Option<Arc<dyn Resolver>>,
    signing: Option<SigningConfig>,
    /// Per-tenant DKIM keys (ADR 0014): when set, a message is signed with the
    /// stored key for its `From` domain, falling back to `signing` (the
    /// configured file key) when the domain has no stored key.
    dkim_store: Option<Arc<Store>>,
    rspamd: Option<Arc<RspamdClient>>,
    clamav: Option<Arc<ClamavClient>>,
}

/// What the inbound gauntlet decided the transaction should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InboundOutcome {
    /// Accept and spool the message.
    Accept,
    /// Refuse: DMARC `p=reject` on an authenticated failure (550).
    RejectDmarc,
    /// Refuse: Rspamd `reject` action (550).
    RejectSpam,
    /// Refuse: a ClamAV signature matched (550). The signature name
    /// travels in [`InboundResult::virus`].
    RejectVirus,
    /// Defer: Rspamd soft-reject/greylist, or either scanner
    /// (Rspamd/ClamAV) was unreachable and policy is fail-closed (451).
    DeferSpam,
}

/// The result of the inbound gauntlet: headers to prepend, the outcome,
/// and (when a forged header had to be removed) the rewritten body.
pub struct InboundResult {
    /// `Received-SPF` + `Authentication-Results` blocks, each ending in
    /// CRLF, ready to prepend to the message. Empty when the outcome is
    /// a reject/defer (the message is not spooled).
    pub headers: String,
    /// What the caller should do with the transaction.
    pub outcome: InboundOutcome,
    /// `Some(bytes)` when a pre-existing `Authentication-Results` (with
    /// our authserv-id) or `Received-SPF` header was stripped from the
    /// received message (RFC 8601 §5); the caller must spool these bytes
    /// in place of the original. `None` when nothing was removed.
    pub stripped_body: Option<Vec<u8>>,
    /// The DMARC evaluation, when a policy was discovered — the row the
    /// caller records for aggregate reporting (RFC 7489 §7.2). `None`
    /// when the sender publishes no DMARC record (nothing to report to)
    /// or discovery hit a transient DNS error.
    pub dmarc_event: Option<DmarcEvent>,
    /// The matched malware signature name (sanitized) when the outcome
    /// is [`InboundOutcome::RejectVirus`]; `None` otherwise.
    pub virus: Option<String>,
}

/// One DMARC evaluation, in aggregate-report terms (RFC 7489 §7.2):
/// the applied disposition and the alignment outcomes for the From
/// domain. No message content — exactly what the report discloses.
#[derive(Debug, Clone)]
pub struct DmarcEvent {
    /// The RFC 5322 From domain the policy was evaluated for.
    pub from_domain: String,
    /// Applied disposition token (`none`/`quarantine`/`reject`),
    /// after `pct=` sampling.
    pub disposition: &'static str,
    /// DKIM alignment outcome (§3.1).
    pub dkim_aligned: bool,
    /// SPF alignment outcome (§3.1).
    pub spf_aligned: bool,
}

impl AuthMail {
    /// A context that performs no authentication and no signing.
    pub fn disabled(hostname: impl Into<String>) -> Self {
        Self {
            hostname: hostname.into(),
            resolver: None,
            signing: None,
            dkim_store: None,
            rspamd: None,
            clamav: None,
        }
    }

    /// Installs the DNS resolver used for SPF/DKIM/DMARC lookups.
    #[must_use]
    pub fn with_resolver(mut self, resolver: Arc<dyn Resolver>) -> Self {
        self.resolver = Some(resolver);
        self
    }

    /// Installs the Rspamd spam-scoring client (MX only).
    #[must_use]
    pub fn with_rspamd(mut self, rspamd: Arc<RspamdClient>) -> Self {
        self.rspamd = Some(rspamd);
        self
    }

    /// Installs the ClamAV malware scanner (MX only, fail-closed).
    #[must_use]
    pub fn with_clamav(mut self, clamav: Arc<ClamavClient>) -> Self {
        self.clamav = Some(clamav);
        self
    }

    /// Installs the submission DKIM signer.
    #[must_use]
    pub fn with_signing(mut self, signing: SigningConfig) -> Self {
        self.signing = Some(signing);
        self
    }

    /// Installs the store used to resolve per-tenant DKIM keys by the `From`
    /// domain (ADR 0014). The configured file key (`with_signing`) remains the
    /// fallback for domains without a stored key.
    #[must_use]
    pub fn with_dkim_store(mut self, store: Arc<Store>) -> Self {
        self.dkim_store = Some(store);
        self
    }

    /// Whether the inbound gauntlet does anything: SPF/DKIM/DMARC (needs
    /// the resolver) or Rspamd scanning. The caller only runs `inbound`
    /// when this is true.
    pub fn is_active(&self) -> bool {
        self.resolver.is_some() || self.rspamd.is_some() || self.clamav.is_some()
    }

    /// Runs the inbound gauntlet — SPF/DKIM/DMARC when a resolver is
    /// available, then Rspamd when configured — and returns the headers
    /// to prepend plus the outcome. Rspamd runs **independent of the
    /// resolver**: a scanner outage fails closed (451) even if DNS is
    /// down, so a configured scanner never silently stops filtering.
    pub async fn inbound(
        &self,
        peer_ip: IpAddr,
        helo: &str,
        mail_from: Option<&str>,
        recipients: &[String],
        raw_message: &[u8],
    ) -> InboundResult {
        // Nothing configured → accept unchanged.
        if self.resolver.is_none() && self.rspamd.is_none() && self.clamav.is_none() {
            return InboundResult {
                headers: String::new(),
                outcome: InboundOutcome::Accept,
                stripped_body: None,
                dmarc_event: None,
                virus: None,
            };
        }

        let message = Message::parse(raw_message);
        let identity = mail_from.unwrap_or(helo);
        let identity_key = if mail_from.is_some() {
            "smtp.mailfrom"
        } else {
            "smtp.helo"
        };
        let mut headers = String::new();
        let mut ar = AuthenticationResults::new(&self.hostname);
        let mut dmarc_reject = false;
        let mut dmarc_event = None;

        // SPF + DKIM + DMARC — only when the resolver is available. When
        // it is not, these are skipped (mail still flows / gets scanned);
        // Authentication-Results then carries just the spam verdict.
        if let Some(resolver) = &self.resolver {
            let resolver = resolver.as_ref();
            let mail_from_box = mail_from
                .and_then(split_address)
                .map(|(local, domain)| Mailbox { local, domain });
            let spf_query = SpfQuery {
                ip: peer_ip,
                helo: helo.to_owned(),
                mail_from: mail_from_box,
            };
            let spf_verdict = spf::check_host(resolver, &spf_query).await;
            let dkim_verdicts = dkim::verify(resolver, &message).await;
            let from_domain = header_from_domain(&message).unwrap_or_default();
            let dmarc_verdict =
                dmarc::evaluate(resolver, &from_domain, &spf_verdict, &dkim_verdicts).await;

            // Received-SPF (RFC 7208 §9.1). The explanation is a
            // parenthesized comment (spaces allowed); envelope-from/helo
            // are `key=value` tokens — strip the structural chars (SP,
            // `;`, `=`) an attacker could use to forge extra pairs.
            headers.push_str(&format!(
                "Received-SPF: {} ({}) client-ip={}; envelope-from={}; helo={};\r\n",
                spf_verdict.result.as_str(),
                sanitize(&spf_verdict.explanation),
                peer_ip,
                sanitize_token(identity),
                sanitize_token(helo),
            ));

            ar = ar.spf(&spf_verdict, identity_key, identity);
            for verdict in &dkim_verdicts {
                ar = ar.dkim(verdict);
            }
            if dmarc_verdict.result != DmarcResult::None {
                ar = ar.dmarc(&dmarc_verdict);
            }

            // DMARC disposition: reject only on an explicit reject policy,
            // after applying the published `pct` sampling (§6.6.4) so a
            // sender mid-rollout (`p=reject; pct<100`) is not enforced at
            // 100%. The draw is a non-cryptographic sub-nanosecond sample.
            let roll = (jiff::Timestamp::now().subsec_nanosecond().unsigned_abs() % 100) as u8;
            let effective =
                dmarc::sample_disposition(dmarc_verdict.disposition, dmarc_verdict.pct, roll);
            dmarc_reject =
                dmarc_verdict.result == DmarcResult::Fail && effective == Disposition::Reject;
            if dmarc_reject {
                tracing::info!(from = %from_domain, "DMARC reject policy; refusing message");
            }

            // Aggregate-report row (RFC 7489 §7.2): only evaluations
            // where a policy was discovered are reportable — a pass
            // applied no disposition; a fail applied the sampled one.
            dmarc_event = match dmarc_verdict.result {
                DmarcResult::Pass => Some(DmarcEvent {
                    from_domain: dmarc_verdict.from_domain.clone(),
                    disposition: Disposition::None.as_str(),
                    dkim_aligned: dmarc_verdict.dkim_aligned,
                    spf_aligned: dmarc_verdict.spf_aligned,
                }),
                DmarcResult::Fail => Some(DmarcEvent {
                    from_domain: dmarc_verdict.from_domain.clone(),
                    disposition: effective.as_str(),
                    dkim_aligned: dmarc_verdict.dkim_aligned,
                    spf_aligned: dmarc_verdict.spf_aligned,
                }),
                _ => None,
            };
        }

        // Malware scan (ClamAV) — a FOUND rejects outright; a scanner
        // error/timeout fails closed (451 defer), never spooling an
        // unscanned message while scanning is configured.
        let mut virus: Option<String> = None;
        if let Some(clamav) = &self.clamav {
            match clamav.scan(raw_message).await {
                Ok(ClamVerdict::Clean) => {}
                Ok(ClamVerdict::Infected(signature)) => {
                    tracing::info!(%signature, "clamav match; refusing message");
                    virus = Some(signature);
                }
                Err(error) => {
                    tracing::error!(%error, "clamav unreachable; deferring message (fail-closed)");
                    return InboundResult {
                        headers: String::new(),
                        outcome: InboundOutcome::DeferSpam,
                        stripped_body: None,
                        dmarc_event: None,
                        virus: None,
                    };
                }
            }
        }
        if let Some(signature) = virus {
            // A 550 is final — the DMARC evaluation stays reportable.
            return InboundResult {
                headers: String::new(),
                outcome: InboundOutcome::RejectVirus,
                stripped_body: None,
                dmarc_event,
                virus: Some(signature),
            };
        }

        // Rspamd spam scoring (M4b) — runs whenever configured, whether or
        // not the resolver is present. A scanner error/timeout fails
        // closed (defer), never spooling an unscanned message.
        let spam_verdict = if let Some(rspamd) = &self.rspamd {
            let ip = peer_ip.to_string();
            let meta = RspamdMeta {
                ip: &ip,
                helo,
                mail_from,
                recipients,
                mta_name: &self.hostname,
            };
            match rspamd.check(&meta, raw_message).await {
                Ok(verdict) => Some(verdict),
                Err(error) => {
                    tracing::error!(%error, "rspamd unreachable; deferring message (fail-closed)");
                    // Deferred mail is retried and re-evaluated; recording
                    // an event here would double-count it in the report.
                    return InboundResult {
                        headers: String::new(),
                        outcome: InboundOutcome::DeferSpam,
                        stripped_body: None,
                        dmarc_event: None,
                        virus: None,
                    };
                }
            }
        } else {
            None
        };
        // Record the Rspamd verdict as the `x-spam` method (the visible
        // "why was this flagged" data downstream renders).
        if let Some(verdict) = &spam_verdict {
            ar = ar.spam(verdict.action.spam_token(), Some(verdict.score));
        }
        headers.push_str(&format!("Authentication-Results: {}\r\n", ar.render()));

        // Outcome: DMARC reject takes precedence, then the spam action.
        let outcome = if dmarc_reject {
            InboundOutcome::RejectDmarc
        } else {
            match spam_verdict.map(|v| v.action) {
                Some(RspamdAction::Reject) => {
                    tracing::info!("rspamd reject; refusing message");
                    InboundOutcome::RejectSpam
                }
                Some(RspamdAction::Greylist | RspamdAction::SoftReject) => {
                    InboundOutcome::DeferSpam
                }
                _ => InboundOutcome::Accept,
            }
        };

        // On any reject/defer the message is not spooled, so the stamped
        // headers are unused — drop them to keep the documented invariant.
        // A 550 is final and stays reportable; a 451 defer is retried and
        // re-evaluated, so its event is dropped (no double-counting).
        if outcome != InboundOutcome::Accept {
            return InboundResult {
                headers: String::new(),
                outcome,
                stripped_body: None,
                dmarc_event: if outcome == InboundOutcome::DeferSpam {
                    None
                } else {
                    dmarc_event
                },
                virus: None,
            };
        }

        // RFC 8601 §5: strip any pre-existing Authentication-Results
        // (bearing our authserv-id) or Received-SPF header a remote
        // sender may have forged, so downstream never trusts a
        // planted verdict.
        let stripped_body = strip_authserv_headers(raw_message, &self.hostname);

        InboundResult {
            headers,
            outcome,
            stripped_body,
            dmarc_event,
            virus: None,
        }
    }

    /// DKIM-signs an outbound (submission) message, returning the
    /// `DKIM-Signature:` header line (with CRLF) to prepend, or `None`
    /// when signing is not configured or the key is unavailable.
    pub async fn sign_outbound(&self, raw_message: &[u8]) -> Option<String> {
        let message = Message::parse(raw_message);

        // Per-tenant key (ADR 0014): sign with the stored key for the message's
        // `From` domain when one exists. `header_from_domain` refuses a From
        // with multiple domains, so `d=` is never attacker-chosen. On any miss
        // or failure we fall through to the configured file key below — the
        // single-tenant path is byte-identical to before.
        if let Some(store) = &self.dkim_store
            && let Some(domain) = header_from_domain(&message)
            && let Ok(materials) = store.active_dkim_materials(&domain).await
            && !materials.is_empty()
        {
            // One signature per active key - a domain may hold an RSA key and an
            // Ed25519 key at once, and a message carrying both verifies for
            // receivers that read only one of them. RFC 6376 allows any number of
            // signatures and a verifier takes the first it can check, so this is
            // additive: a single-key domain produces exactly the header it
            // produced before.
            let mut headers = String::new();
            for material in materials {
                let single = SingleKeyStore {
                    domain: domain.clone(),
                    selector: material.selector.clone(),
                    algorithm: material.algorithm.clone(),
                    seed: zeroize::Zeroizing::new(material.seed),
                };
                let params = SignParams::new(&domain, &material.selector);
                match dkim::sign(&single, &message, &params).await {
                    Ok(value) => headers.push_str(&format!("DKIM-Signature: {value}\r\n")),
                    Err(error) => {
                        // One algorithm failing must not cost the other its
                        // signature.
                        tracing::error!(
                            %error, %domain, selector = %material.selector,
                            "per-domain DKIM signing failed for one key"
                        );
                    }
                }
            }
            if !headers.is_empty() {
                return Some(headers);
            }
            tracing::error!(%domain, "no per-domain key signed; trying the configured key");
        }

        // Fallback: the configured deployment key(s) (the single-tenant
        // path). With a second key configured (RFC 8463 dual-signing) the
        // message carries one signature per key, RSA first — the wiring
        // ordered the selectors that way for verifiers that cannot read
        // Ed25519 yet.
        let signing = self.signing.as_ref()?;
        let mut headers = String::new();
        for selector in std::iter::once(&signing.selector).chain(&signing.second_selector) {
            let params = SignParams::new(&signing.domain, selector);
            match dkim::sign(signing.keys.as_ref(), &message, &params).await {
                Ok(value) => headers.push_str(&format!("DKIM-Signature: {value}\r\n")),
                Err(error) => {
                    // One key failing must not cost the other its
                    // signature — and a total failure must not lose the
                    // message (deliverability degrades, mail flows).
                    tracing::error!(
                        %error, %selector,
                        "DKIM signing failed for a configured key"
                    );
                }
            }
        }
        (!headers.is_empty()).then_some(headers)
    }

    /// ARC-seals a message about to be forwarded (Sieve `redirect`,
    /// RFC 8617 first hop), returning the three `ARC-*` header lines to
    /// prepend, or `None` when sealing is not possible — no
    /// `Authentication-Results` of ours on the message, no signing key,
    /// an existing ARC chain, or a signing failure. The caller always
    /// forwards regardless; an unsealed forward is degraded
    /// deliverability, never lost mail.
    ///
    /// `seal_domain` is the forwarding account's domain: the seal is
    /// made with that tenant's stored DKIM key (ADR 0014), falling back
    /// to the configured deployment key.
    pub async fn seal_arc(&self, raw_message: &[u8], seal_domain: &str) -> Option<String> {
        let message = Message::parse(raw_message);

        // The AAR must carry the verdicts WE computed at ingress: the
        // Authentication-Results whose authserv-id is our hostname
        // (forged ones were already stripped at DATA, RFC 8601 §5).
        let authres = message.headers.iter().find_map(|(name, value)| {
            if !name.eq_ignore_ascii_case("Authentication-Results") {
                return None;
            }
            let v = value.trim_start();
            let id_end = v.find([' ', '\t', ';', '\r']).unwrap_or(v.len());
            v[..id_end]
                .eq_ignore_ascii_case(&self.hostname)
                .then(|| (*value).to_owned())
        })?;

        // Per-tenant key for the forwarding domain (ADR 0014), then the
        // configured deployment key — the same order as `sign_outbound`.
        if let Some(store) = &self.dkim_store
            && let Ok(Some(material)) = store.active_dkim_material(seal_domain).await
        {
            // One seal, not one per algorithm: an ARC set is a chain with a
            // single `i=` per hop, so sealing twice would be two competing chains
            // rather than two readable signatures.
            let single = SingleKeyStore {
                domain: seal_domain.to_owned(),
                selector: material.selector.clone(),
                algorithm: material.algorithm.clone(),
                seed: zeroize::Zeroizing::new(material.seed),
            };
            let params = arc::SealParams::new(seal_domain, &material.selector, &authres);
            match arc::seal(&single, &message, &params).await {
                Ok(set) => return Some(set),
                Err(arc::SealError::ExistingChain) => {
                    tracing::debug!("message already carries an ARC chain; forwarding unsealed");
                    return None;
                }
                Err(error) => {
                    tracing::error!(%error, "per-tenant ARC sealing failed; trying the configured key");
                }
            }
        }
        let signing = self.signing.as_ref()?;
        let params = arc::SealParams::new(&signing.domain, &signing.selector, &authres);
        match arc::seal(signing.keys.as_ref(), &message, &params).await {
            Ok(set) => Some(set),
            Err(arc::SealError::ExistingChain) => {
                tracing::debug!("message already carries an ARC chain; forwarding unsealed");
                None
            }
            Err(error) => {
                tracing::error!(%error, "ARC sealing failed; forwarding unsealed");
                None
            }
        }
    }
}

/// Splits a `local@domain` address into its parts (lowercasing the
/// domain). Returns `None` when there is no `@`.
fn split_address(addr: &str) -> Option<(String, String)> {
    let addr = addr.trim().trim_start_matches('<').trim_end_matches('>');
    let (local, domain) = addr.rsplit_once('@')?;
    if local.is_empty() || domain.is_empty() {
        return None;
    }
    Some((local.to_owned(), domain.to_ascii_lowercase()))
}

/// Extracts the domain of the RFC 5322 `From` header for DMARC. Returns
/// `None` when there is no `From`, no parseable domain, or — per RFC
/// 7489 §6.6.1 — the header carries multiple addresses in *different*
/// domains (DMARC alignment then has no single domain to judge, so it
/// must not proceed on an attacker-chosen one).
fn header_from_domain(message: &Message<'_>) -> Option<String> {
    let (_, value) = message
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("From"))?;
    // A `From` may list several mailboxes (comma-separated). Collect the
    // distinct domains; DMARC requires exactly one.
    let mut domains: Vec<String> = Vec::new();
    for part in value.split(',') {
        let Some(at) = part.rfind('@') else { continue };
        let domain: String = part[at + 1..]
            .chars()
            .skip_while(|c| c.is_whitespace())
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
            .collect();
        let domain = domain.trim_end_matches('.').to_ascii_lowercase();
        if !domain.is_empty() && !domains.contains(&domain) {
            domains.push(domain);
        }
    }
    match domains.as_slice() {
        [only] => Some(only.clone()),
        _ => None,
    }
}

/// Removes any pre-existing `Authentication-Results` header whose
/// authserv-id equals our hostname, and any `Received-SPF` header, from
/// the received message (RFC 8601 §5). Returns `Some(rewritten)` when at
/// least one header was removed, else `None`. Parses over raw bytes and
/// preserves every other header (including legitimate upstream
/// `Authentication-Results` from a *different* authserv-id) byte-exact.
fn strip_authserv_headers(raw: &[u8], authserv_id: &str) -> Option<Vec<u8>> {
    // Split at the header/body separator; only a well-formed message
    // (with a blank line) is rewritten — otherwise leave it untouched.
    let sep = find_double_crlf(raw)?;
    // `hb` covers every header line including its terminating CRLF (the
    // last header's CRLF is the first half of the CRLFCRLF separator).
    let hb = &raw[..sep + 2];
    let tail = &raw[sep + 2..]; // blank line + body, verbatim
    let n = hb.len();
    let mut out: Vec<u8> = Vec::with_capacity(raw.len());
    let mut removed = false;
    let mut i = 0;
    while i < n {
        // A field spans its start line plus any WSP-led continuations.
        let mut end = crlf_after(hb, i);
        while end < n && (hb[end] == b' ' || hb[end] == b'\t') {
            end = crlf_after(hb, end);
        }
        let field = &hb[i..end];
        if field_is_forged(field, authserv_id) {
            removed = true;
        } else {
            out.extend_from_slice(field);
        }
        i = end;
    }
    if !removed {
        return None;
    }
    out.extend_from_slice(tail);
    Some(out)
}

/// Whether a full header field (name through trailing CRLF) is a
/// pre-existing `Received-SPF`, or an `Authentication-Results` whose
/// authserv-id is our own hostname.
fn field_is_forged(field: &[u8], authserv_id: &str) -> bool {
    let Some(colon) = field.iter().position(|&b| b == b':') else {
        return false;
    };
    let name = &field[..colon];
    if name.eq_ignore_ascii_case(b"Received-SPF") {
        return true;
    }
    if name.eq_ignore_ascii_case(b"Authentication-Results") {
        // The authserv-id is the first token of the value.
        let value = field[colon + 1..].trim_ascii_start();
        let id_end = value
            .iter()
            .position(|&b| matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b';'))
            .unwrap_or(value.len());
        return value[..id_end].eq_ignore_ascii_case(authserv_id.as_bytes());
    }
    false
}

/// Index just past the CRLF that ends the line starting at `from`
/// (or `hb.len()` if the line has no CRLF).
fn crlf_after(hb: &[u8], from: usize) -> usize {
    let mut j = from;
    while j + 1 < hb.len() {
        if hb[j] == b'\r' && hb[j + 1] == b'\n' {
            return j + 2;
        }
        j += 1;
    }
    hb.len()
}

fn find_double_crlf(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|w| w == b"\r\n\r\n")
}

/// Removes control characters (CR/LF) from a value placed in a header.
fn sanitize(value: &str) -> String {
    value.chars().filter(|c| !c.is_control()).collect()
}

/// Sanitizes a `key=value` token field (Received-SPF `envelope-from`,
/// `helo`): keeps only dot-atom / addr-spec characters, dropping SP,
/// `;`, `=`, and control chars an attacker could use to forge extra
/// key/value pairs in the header.
fn sanitize_token(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '@' | '+'))
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn split_address_parses_and_lowercases() {
        assert_eq!(
            split_address("Bob@Example.ORG"),
            Some(("Bob".to_owned(), "example.org".to_owned()))
        );
        assert_eq!(
            split_address("<a@b.test>"),
            Some(("a".to_owned(), "b.test".to_owned()))
        );
        assert_eq!(split_address("no-at-sign"), None);
        assert_eq!(split_address("@nolocal"), None);
    }

    #[test]
    fn from_domain_extraction() {
        let raw = b"From: Alice <alice@Example.com>\r\nSubject: x\r\n\r\nbody\r\n";
        let msg = Message::parse(raw);
        assert_eq!(header_from_domain(&msg), Some("example.com".to_owned()));
    }

    #[test]
    fn multi_from_with_differing_domains_is_none() {
        // RFC 7489 §6.6.1: two From domains → no single domain to align.
        let raw = b"From: a@one.example, b@two.example\r\nSubject: x\r\n\r\nbody\r\n";
        let msg = Message::parse(raw);
        assert_eq!(header_from_domain(&msg), None);
        // But two mailboxes in the SAME domain resolve to that domain.
        let raw2 = b"From: a@same.example, b@same.example\r\n\r\nbody\r\n";
        assert_eq!(
            header_from_domain(&Message::parse(raw2)),
            Some("same.example".to_owned())
        );
    }

    #[test]
    fn strips_forged_authserv_headers_only() {
        let raw = concat!(
            "Received-SPF: pass (forged) client-ip=1.2.3.4;\r\n",
            "Authentication-Results: mx.alo.test; dmarc=pass header.from=bank.com\r\n",
            "Authentication-Results: upstream.example; spf=pass\r\n",
            "From: alice@example.com\r\n",
            "Subject: hi\r\n",
            "\r\n",
            "body\r\n",
        )
        .as_bytes();
        let cleaned = strip_authserv_headers(raw, "mx.alo.test").expect("headers removed");
        let text = String::from_utf8(cleaned).unwrap();
        // Our own planted verdict and the Received-SPF are gone.
        assert!(!text.contains("dmarc=pass header.from=bank.com"));
        assert!(!text.contains("Received-SPF:"));
        // The legitimate upstream result (different authserv-id) survives.
        assert!(text.contains("Authentication-Results: upstream.example; spf=pass"));
        // The real message is intact.
        assert!(text.contains("From: alice@example.com"));
        assert!(text.contains("Subject: hi"));
        assert!(text.ends_with("\r\n\r\nbody\r\n"));
    }

    #[test]
    fn strip_returns_none_when_nothing_forged() {
        let raw = b"From: alice@example.com\r\nSubject: hi\r\n\r\nbody\r\n";
        assert!(strip_authserv_headers(raw, "mx.alo.test").is_none());
    }

    #[tokio::test]
    async fn disabled_context_stamps_nothing() {
        let auth = AuthMail::disabled("mx.alo.test");
        assert!(!auth.is_active());
        let result = auth
            .inbound(
                "192.0.2.1".parse().unwrap(),
                "helo",
                Some("a@b.test"),
                &["c@d.test".to_owned()],
                b"From: a@b.test\r\n\r\nx",
            )
            .await;
        assert!(result.headers.is_empty());
        assert_eq!(result.outcome, InboundOutcome::Accept);
        assert!(auth.sign_outbound(b"msg").await.is_none());
    }

    #[tokio::test]
    async fn clamav_configured_but_unreachable_fails_closed() {
        // A configured malware scanner that is down must defer (451),
        // never accept an unscanned message.
        let clam = crate::clamav::ClamavClient::from_addr(
            "127.0.0.1:1",
            std::time::Duration::from_millis(300),
        )
        .unwrap();
        let auth = AuthMail::disabled("mx.alo.test").with_clamav(Arc::new(clam));
        assert!(auth.is_active());
        let result = auth
            .inbound(
                "192.0.2.1".parse().unwrap(),
                "helo",
                Some("a@b.test"),
                &["c@d.test".to_owned()],
                b"From: a@b.test

x",
            )
            .await;
        assert_eq!(result.outcome, InboundOutcome::DeferSpam);
        assert!(result.headers.is_empty());
        assert!(result.virus.is_none());
    }

    /// A mock clamd answering `verdict` to one INSTREAM exchange.
    async fn mock_clamd(verdict: &'static [u8]) -> std::net::SocketAddr {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 65536];
            // Drain the command+chunks (the trailing zero-length chunk
            // ends the client's writes), then reply.
            loop {
                let n = sock.read(&mut buf).await.unwrap();
                if n == 0 || buf[..n].ends_with(&0u32.to_be_bytes()) {
                    break;
                }
            }
            sock.write_all(verdict).await.unwrap();
        });
        addr
    }

    #[tokio::test]
    async fn clamav_found_rejects_with_the_signature() {
        let addr = mock_clamd(b"stream: Eicar-Signature FOUND\0").await;
        let clam = crate::clamav::ClamavClient::from_addr(
            &addr.to_string(),
            std::time::Duration::from_secs(5),
        )
        .unwrap();
        let auth = AuthMail::disabled("mx.alo.test").with_clamav(Arc::new(clam));
        let result = auth
            .inbound(
                "192.0.2.1".parse().unwrap(),
                "helo",
                Some("a@b.test"),
                &["c@d.test".to_owned()],
                b"From: a@b.test

payload",
            )
            .await;
        assert_eq!(result.outcome, InboundOutcome::RejectVirus);
        assert_eq!(result.virus.as_deref(), Some("Eicar-Signature"));
        assert!(result.headers.is_empty(), "rejects stamp nothing");
    }

    #[tokio::test]
    async fn clamav_clean_accepts() {
        let addr = mock_clamd(b"stream: OK\0").await;
        let clam = crate::clamav::ClamavClient::from_addr(
            &addr.to_string(),
            std::time::Duration::from_secs(5),
        )
        .unwrap();
        let auth = AuthMail::disabled("mx.alo.test").with_clamav(Arc::new(clam));
        let result = auth
            .inbound(
                "192.0.2.1".parse().unwrap(),
                "helo",
                Some("a@b.test"),
                &["c@d.test".to_owned()],
                b"From: a@b.test

hello",
            )
            .await;
        assert_eq!(result.outcome, InboundOutcome::Accept);
        assert!(result.virus.is_none());
    }

    #[tokio::test]
    async fn rspamd_configured_without_resolver_fails_closed() {
        // Regression (cold-review BLOCKER): a scanner configured but
        // unreachable must defer (451) even when the DNS resolver is
        // absent — never silently accept an unscanned message.
        let rspamd =
            RspamdClient::from_url("http://127.0.0.1:1", std::time::Duration::from_millis(300))
                .unwrap();
        let auth = AuthMail::disabled("mx.alo.test").with_rspamd(Arc::new(rspamd));
        assert!(auth.is_active());
        let result = auth
            .inbound(
                "192.0.2.1".parse().unwrap(),
                "helo",
                Some("a@b.test"),
                &["c@d.test".to_owned()],
                b"From: a@b.test\r\n\r\nx",
            )
            .await;
        assert_eq!(result.outcome, InboundOutcome::DeferSpam);
        assert!(result.headers.is_empty());
    }

    #[tokio::test]
    async fn rspamd_runs_and_stamps_without_a_resolver() {
        // Happy path with no resolver: Rspamd still runs; a clean verdict
        // accepts and records the x-spam method (no SPF/DKIM/DMARC).
        let (addr, server) =
            crate::canned_http::serve_once(b"{\"action\":\"no action\",\"score\":0.1}").await;
        let rspamd =
            RspamdClient::from_url(&format!("http://{addr}"), std::time::Duration::from_secs(5))
                .unwrap();
        let auth = AuthMail::disabled("mx.alo.test").with_rspamd(Arc::new(rspamd));
        let result = auth
            .inbound(
                "192.0.2.1".parse().unwrap(),
                "helo",
                Some("a@b.test"),
                &["c@d.test".to_owned()],
                b"From: a@b.test\r\n\r\nhello",
            )
            .await;
        assert_eq!(result.outcome, InboundOutcome::Accept);
        assert!(result.headers.contains("x-spam=no"));
        // No resolver → no Received-SPF stamped.
        assert!(!result.headers.contains("Received-SPF"));
        server.await.unwrap();
    }
}
