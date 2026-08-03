//! Destination resolution for outbound delivery, RFC 5321 §5.1, plus
//! the DANE half of RFC 7672.
//!
//! Rules implemented: MX records sorted by preference; when no MX
//! exists but the domain does, the implicit MX is the domain itself;
//! a null MX (single MX with preference 0 and target ".", RFC 7505)
//! means the domain never accepts mail — permanent failure; NXDOMAIN
//! is permanent; DNS timeouts/server failures are transient.
//!
//! DANE: each MX host's `_25._tcp.<host>` TLSA RRset is fetched over a
//! separate **DNSSEC-validating** resolver (hickory validates the
//! chain itself; the upstream is only transport). A `Secure` usable
//! set makes TLS mandatory + authenticated for that host; a `Secure`
//! but unusable set makes TLS mandatory (RFC 7672 §2.2); an insecure
//! or absent set leaves delivery opportunistic; and a lookup *failure*
//! (SERVFAIL — which is also how a bogus, possibly attacker-stripped
//! chain presents) skips the host entirely rather than risk an
//! unauthenticated session that policy forbids.
//!
//! Deviation (recorded): RFC 7672 §2.2.1 only applies DANE when the MX
//! RRset itself was DNSSEC-secure; we enforce whenever the TLSA RRset
//! is secure, regardless of MX security — strictly stronger, never
//! weaker (an attacker who can forge our MX answers today gets
//! opportunistic TLS either way; this closes the on-path STARTTLS
//! strip for every DANE MX).
//!
//! The trait exists so the queue can be tested without the network;
//! [`DnsResolver`] is the production implementation (hickory).

use std::net::IpAddr;

use alo_smtp_client::client::TlsRequirement;
use alo_smtp_client::dane::TlsaRecord;
use hickory_resolver::TokioResolver;
use hickory_resolver::net::NetError;
use hickory_resolver::proto::dnssec::Proof;
use hickory_resolver::proto::rr::{RData, RecordType};

/// Why resolution failed, split by retry semantics.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResolveFailure {
    /// The domain does not exist or explicitly refuses mail
    /// (NXDOMAIN, or RFC 7505 null MX) — generate a DSN, never retry.
    #[error("permanent resolution failure: {reason}")]
    Permanent {
        /// Human-readable cause for the DSN diagnostic.
        reason: String,
    },
    /// DNS infrastructure trouble (timeout, SERVFAIL) — retry later.
    #[error("transient resolution failure: {reason}")]
    Transient {
        /// Human-readable cause for logs/state.
        reason: String,
    },
}

/// An MX target ready to try, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailHost {
    /// Hostname to connect to (display/logging; connection uses `ips`).
    pub host: String,
    /// Resolved addresses for the host, in try order.
    pub ips: Vec<IpAddr>,
    /// The TLS policy for this host, from its (DNSSEC-validated) TLSA
    /// RRset — `Opportunistic` when DANE is disabled or absent.
    pub tls: TlsRequirement,
}

impl MailHost {
    /// A host with no TLSA-derived policy (tests, non-DANE paths).
    pub fn opportunistic(host: impl Into<String>, ips: Vec<IpAddr>) -> Self {
        Self {
            host: host.into(),
            ips,
            tls: TlsRequirement::Opportunistic,
        }
    }
}

/// Boxed future alias for the object-safe async trait method below
/// (hand-desugared so we don't pull the async-trait crate for one
/// trait).
pub type ResolveFuture<'a> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<Vec<MailHost>, ResolveFailure>> + Send + 'a>,
>;

/// Resolves a mail domain to an ordered list of hosts to attempt.
pub trait MxResolve: Send + Sync {
    /// Ordered delivery targets for `domain` per RFC 5321 §5.1.
    ///
    /// # Errors
    /// [`ResolveFailure`] with retry semantics encoded.
    fn resolve<'a>(&'a self, domain: &'a str) -> ResolveFuture<'a>;
}

/// Production resolver over the system's configured DNS, with an
/// optional DNSSEC-validating side channel for TLSA (DANE).
pub struct DnsResolver {
    inner: TokioResolver,
    /// The validating resolver for `_25._tcp.<mx>` TLSA lookups.
    /// `None` disables DANE (every host is `Opportunistic`).
    dane: Option<TokioResolver>,
}

impl DnsResolver {
    /// Builds a resolver from the system configuration.
    ///
    /// # Errors
    /// Returns the underlying error when the system DNS configuration
    /// cannot be read.
    pub fn from_system() -> Result<Self, NetError> {
        let inner = TokioResolver::builder_tokio()?.build()?;
        Ok(Self { inner, dane: None })
    }

    /// Enables DANE: TLSA lookups run over a DNSSEC-validating resolver
    /// pointed at well-known public servers (hickory validates the
    /// chain itself, so the upstream is untrusted transport — and
    /// bypassing the container's embedded DNS avoids its EDNS quirks).
    #[must_use]
    pub fn with_dane(mut self) -> Self {
        use hickory_resolver::Resolver;
        use hickory_resolver::config::{CLOUDFLARE, QUAD9, ResolverConfig};
        use hickory_resolver::net::runtime::TokioRuntimeProvider;

        let mut config = ResolverConfig::udp_and_tcp(&CLOUDFLARE);
        for server in ResolverConfig::udp_and_tcp(&QUAD9).name_servers() {
            config.add_name_server(server.clone());
        }
        let mut builder = Resolver::builder_with_config(config, TokioRuntimeProvider::default());
        builder.options_mut().validate = true;
        match builder.build() {
            Ok(resolver) => self.dane = Some(resolver),
            Err(error) => {
                // Fail open to today's behavior (opportunistic TLS),
                // loudly — DANE absent is degraded, not broken.
                tracing::error!(%error, "DANE resolver unavailable; outbound TLS stays opportunistic");
            }
        }
        self
    }

    async fn lookup_ips(&self, host: &str) -> Result<Vec<IpAddr>, ResolveFailure> {
        match self.inner.lookup_ip(host).await {
            Ok(lookup) => Ok(lookup.iter().collect()),
            Err(error) => Err(classify(&error, host)),
        }
    }

    /// The TLS requirement for one MX host, from its TLSA RRset
    /// (RFC 7672 §2.2). `Err(())` means the lookup itself failed
    /// (SERVFAIL / timeout / bogus) — the caller must skip the host,
    /// never downgrade it to opportunistic.
    async fn tlsa_requirement(&self, host: &str) -> Result<TlsRequirement, ()> {
        let Some(dane) = &self.dane else {
            return Ok(TlsRequirement::Opportunistic);
        };
        let name = format!("_25._tcp.{host}.");
        match dane.lookup(&name, RecordType::TLSA).await {
            Ok(lookup) => {
                let secure: Vec<TlsaRecord> = lookup
                    .answers()
                    .iter()
                    .filter(|record| matches!(record.proof, Proof::Secure))
                    .filter_map(|record| match &record.data {
                        RData::TLSA(tlsa) => Some(TlsaRecord {
                            usage: u8::from(tlsa.cert_usage),
                            selector: u8::from(tlsa.selector),
                            matching: u8::from(tlsa.matching),
                            data: tlsa.cert_data.clone(),
                        }),
                        _ => None,
                    })
                    .collect();
                Ok(classify_tlsa(secure))
            }
            // Proven absence (NXDOMAIN / no records) → no DANE policy.
            Err(error) if error.is_nx_domain() || error.is_no_records_found() => {
                Ok(TlsRequirement::Opportunistic)
            }
            // SERVFAIL/timeout — indistinguishable from an attacker
            // suppressing a bogus chain; skip the host (RFC 7672 §2.1.1).
            Err(error) => {
                tracing::warn!(%host, %error, "TLSA lookup failed; skipping this MX host");
                Err(())
            }
        }
    }
}

/// Maps a **secure** TLSA record set to the session policy
/// (RFC 7672 §2.2): usable DANE-EE records authenticate; a non-empty
/// but unusable set still mandates encryption; an empty set (nothing
/// secure) stays opportunistic.
fn classify_tlsa(secure: Vec<TlsaRecord>) -> TlsRequirement {
    if secure.is_empty() {
        return TlsRequirement::Opportunistic;
    }
    let usable: Vec<TlsaRecord> = secure.into_iter().filter(TlsaRecord::is_usable).collect();
    if usable.is_empty() {
        TlsRequirement::Required
    } else {
        TlsRequirement::DaneEe(usable)
    }
}

impl MxResolve for DnsResolver {
    fn resolve<'a>(&'a self, domain: &'a str) -> ResolveFuture<'a> {
        Box::pin(async move {
            match self.inner.mx_lookup(domain).await {
                Ok(lookup) => {
                    // Extract MX rdata from the answer records (§5.1).
                    let mut records: Vec<(u16, String)> = lookup
                        .answers()
                        .iter()
                        .filter_map(|record| match &record.data {
                            RData::MX(mx) => Some((mx.preference, mx.exchange.to_utf8())),
                            _ => None,
                        })
                        .collect();
                    if let [(preference, exchange)] = records.as_slice()
                        && *preference == 0
                        && (exchange == "." || exchange.is_empty())
                    {
                        // RFC 7505 null MX: the domain refuses mail.
                        return Err(ResolveFailure::Permanent {
                            reason: format!("domain {domain} declares a null MX (RFC 7505)"),
                        });
                    }
                    if records.is_empty() {
                        return Err(ResolveFailure::Transient {
                            reason: format!("{domain}: empty MX answer"),
                        });
                    }
                    // §5.1: sort by preference, lower first.
                    records.sort_by_key(|(preference, _)| *preference);
                    let mut hosts = Vec::new();
                    for (_preference, exchange) in records {
                        let name = exchange.trim_end_matches('.').to_owned();
                        match self.lookup_ips(&name).await {
                            Ok(ips) if !ips.is_empty() => {
                                // DANE (RFC 7672): a failed TLSA lookup
                                // skips the host — connecting without
                                // knowing its policy could violate it.
                                let Ok(tls) = self.tlsa_requirement(&name).await else {
                                    continue;
                                };
                                if tls != TlsRequirement::Opportunistic {
                                    tracing::info!(mx = %name, policy = ?discriminant_name(&tls), "TLSA present; TLS is mandatory for this host");
                                }
                                hosts.push(MailHost {
                                    host: name,
                                    ips,
                                    tls,
                                });
                            }
                            // An MX whose target has no address is
                            // skipped; others may still work (§5.1).
                            Ok(_) | Err(_) => {
                                tracing::debug!(mx = %name, "MX target did not resolve; skipping");
                            }
                        }
                    }
                    if hosts.is_empty() {
                        return Err(ResolveFailure::Transient {
                            reason: format!("no MX target for {domain} currently resolves"),
                        });
                    }
                    Ok(hosts)
                }
                Err(error) if is_no_records(&error) => {
                    // Implicit MX (§5.1): no MX RRset, use the domain
                    // itself if it has an address. The implicit MX host
                    // gets the same TLSA treatment (RFC 7672 §2.2.2).
                    let ips = self.lookup_ips(domain).await?;
                    if ips.is_empty() {
                        return Err(ResolveFailure::Permanent {
                            reason: format!("{domain} has neither MX nor address records"),
                        });
                    }
                    let Ok(tls) = self.tlsa_requirement(domain).await else {
                        return Err(ResolveFailure::Transient {
                            reason: format!("{domain}: TLSA lookup failed for the implicit MX"),
                        });
                    };
                    Ok(vec![MailHost {
                        host: domain.to_owned(),
                        ips,
                        tls,
                    }])
                }
                Err(error) => Err(classify(&error, domain)),
            }
        })
    }
}

/// Maps a hickory error to retry semantics: NXDOMAIN/no-records are
/// permanent, everything else (timeouts, SERVFAIL, I/O) transient.
fn classify(error: &NetError, subject: &str) -> ResolveFailure {
    if error.is_nx_domain() {
        ResolveFailure::Permanent {
            reason: format!("{subject}: domain does not exist (NXDOMAIN)"),
        }
    } else if error.is_no_records_found() {
        ResolveFailure::Permanent {
            reason: format!("{subject}: no address records"),
        }
    } else {
        ResolveFailure::Transient {
            reason: format!("{subject}: DNS lookup failed: {error}"),
        }
    }
}

fn is_no_records(error: &NetError) -> bool {
    error.is_no_records_found()
}

/// Short policy label for logs.
fn discriminant_name(tls: &TlsRequirement) -> &'static str {
    match tls {
        TlsRequirement::Opportunistic => "opportunistic",
        TlsRequirement::Required => "required-unauthenticated",
        TlsRequirement::DaneEe(_) => "dane-ee",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(usage: u8, selector: u8, matching: u8) -> TlsaRecord {
        TlsaRecord {
            usage,
            selector,
            matching,
            data: vec![0u8; 32],
        }
    }

    #[test]
    fn no_secure_records_is_opportunistic() {
        assert_eq!(classify_tlsa(Vec::new()), TlsRequirement::Opportunistic);
    }

    #[test]
    fn usable_dane_ee_records_authenticate() {
        let out = classify_tlsa(vec![rec(2, 1, 1), rec(3, 1, 1), rec(3, 0, 2)]);
        // Only the usable EE records are kept for matching.
        match out {
            TlsRequirement::DaneEe(records) => {
                assert_eq!(records.len(), 2);
                assert!(records.iter().all(TlsaRecord::is_usable));
            }
            other => panic!("expected DaneEe, got {other:?}"),
        }
    }

    #[test]
    fn only_unusable_records_still_require_tls() {
        // DANE-TA(2) and PKIX usages: unusable by this client, but the
        // set's existence forbids cleartext (RFC 7672 §2.2).
        assert_eq!(
            classify_tlsa(vec![rec(2, 1, 1), rec(0, 0, 1), rec(3, 9, 1)]),
            TlsRequirement::Required
        );
    }
}
