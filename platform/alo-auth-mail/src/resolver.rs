//! The single DNS entry point for the trust stack. Every lookup —
//! SPF, DKIM key records, DMARC policy, MTA-STS — goes through here so
//! timeouts, record caps, and defensive parsing are enforced in one
//! place. DNS answers are attacker-influenced input (RFC 7208 §11.1,
//! RFC 6376 §8): treat them as hostile.

use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::pin::Pin;
use std::time::Duration;

use hickory_resolver::TokioResolver;
use hickory_resolver::net::NetError;
use hickory_resolver::proto::rr::RData;

/// Per-lookup timeout. DNS is on the critical path of accepting mail,
/// so a slow authority must not stall the SMTP transaction.
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(10);

/// Defensive cap on records returned for any single query. SPF/DKIM
/// need only a handful; a hostile authority returning thousands must
/// not blow up memory or CPU.
const MAX_RECORDS: usize = 64;

/// Defensive cap on a single TXT record's assembled length. Real SPF
/// (≤ 450-ish) and DKIM key records are well under this.
const MAX_TXT_LEN: usize = 4096;

/// Why a DNS lookup did not yield a usable answer. The split drives
/// SPF/DMARC result mapping: `NotFound` is a *void* lookup, `Temporary`
/// maps to `temperror`, `Malformed` to `permerror`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DnsError {
    /// NXDOMAIN or no records of the requested type (a "void" lookup).
    #[error("no records for {name} ({rtype})")]
    NotFound {
        /// Queried name.
        name: String,
        /// Record type queried.
        rtype: &'static str,
    },
    /// Timeout or SERVFAIL — retryable; SPF/DMARC → temperror.
    #[error("temporary DNS failure for {name} ({rtype}): {reason}")]
    Temporary {
        /// Queried name.
        name: String,
        /// Record type queried.
        rtype: &'static str,
        /// Cause.
        reason: String,
    },
}

/// Boxed future for the object-safe async trait methods below.
pub type DnsFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, DnsError>> + Send + 'a>>;

/// The trust stack's DNS surface. A trait so the evaluators are
/// testable against a fixture map with no network (see tests).
pub trait Resolver: Send + Sync {
    /// TXT records, each assembled from its character-strings and
    /// bounded by [`MAX_TXT_LEN`].
    fn txt<'a>(&'a self, name: &'a str) -> DnsFuture<'a, Vec<String>>;
    /// A records (IPv4).
    fn ipv4<'a>(&'a self, name: &'a str) -> DnsFuture<'a, Vec<Ipv4Addr>>;
    /// AAAA records (IPv6).
    fn ipv6<'a>(&'a self, name: &'a str) -> DnsFuture<'a, Vec<Ipv6Addr>>;
    /// MX exchange hostnames, sorted by preference (lowest first).
    fn mx<'a>(&'a self, name: &'a str) -> DnsFuture<'a, Vec<String>>;
    /// PTR names for an address (SPF `ptr`, discouraged — RFC 7208 §5.5).
    fn ptr(&self, ip: IpAddr) -> DnsFuture<'_, Vec<String>>;
}

/// Production resolver over the system DNS (hickory).
pub struct DnsResolver {
    inner: TokioResolver,
}

impl DnsResolver {
    /// Builds a resolver from the system configuration.
    ///
    /// # Errors
    /// Propagates hickory's configuration error.
    pub fn from_system() -> Result<Self, NetError> {
        Ok(Self {
            inner: TokioResolver::builder_tokio()?.build()?,
        })
    }
}

/// Classifies a hickory lookup error into our retry-semantic split.
fn classify(error: &NetError, name: &str, rtype: &'static str) -> DnsError {
    if error.is_no_records_found() || error.is_nx_domain() {
        DnsError::NotFound {
            name: name.to_owned(),
            rtype,
        }
    } else {
        DnsError::Temporary {
            name: name.to_owned(),
            rtype,
            reason: error.to_string(),
        }
    }
}

/// Wraps a lookup future with the per-lookup timeout, mapping elapse to
/// a `Temporary` failure.
async fn timed<T, F>(future: F, name: &str, rtype: &'static str) -> Result<T, DnsError>
where
    F: Future<Output = Result<T, DnsError>>,
{
    match tokio::time::timeout(LOOKUP_TIMEOUT, future).await {
        Ok(result) => result,
        Err(_elapsed) => Err(DnsError::Temporary {
            name: name.to_owned(),
            rtype,
            reason: "lookup timed out".to_owned(),
        }),
    }
}

impl Resolver for DnsResolver {
    fn txt<'a>(&'a self, name: &'a str) -> DnsFuture<'a, Vec<String>> {
        Box::pin(timed(
            async move {
                match self.inner.txt_lookup(name).await {
                    Ok(lookup) => Ok(lookup
                        .answers()
                        .iter()
                        .take(MAX_RECORDS)
                        .filter_map(|record| match &record.data {
                            // A TXT RR is a sequence of character-strings
                            // concatenated (RFC 7208 §3.3); bound length.
                            RData::TXT(txt) => {
                                let mut assembled = String::new();
                                for data in txt.txt_data.iter() {
                                    if assembled.len() >= MAX_TXT_LEN {
                                        break;
                                    }
                                    assembled.push_str(&String::from_utf8_lossy(data));
                                }
                                // Truncate to the last char boundary at or
                                // below the cap — `String::truncate` panics
                                // on a non-boundary, and the attacker fully
                                // controls these (multi-byte) bytes.
                                if assembled.len() > MAX_TXT_LEN {
                                    let mut end = MAX_TXT_LEN;
                                    while end > 0 && !assembled.is_char_boundary(end) {
                                        end -= 1;
                                    }
                                    assembled.truncate(end);
                                }
                                Some(assembled)
                            }
                            _ => None,
                        })
                        .collect()),
                    Err(error) => Err(classify(&error, name, "TXT")),
                }
            },
            name,
            "TXT",
        ))
    }

    fn ipv4<'a>(&'a self, name: &'a str) -> DnsFuture<'a, Vec<Ipv4Addr>> {
        Box::pin(timed(
            async move {
                match self.inner.ipv4_lookup(name).await {
                    Ok(lookup) => Ok(lookup
                        .answers()
                        .iter()
                        .take(MAX_RECORDS)
                        .filter_map(|record| match &record.data {
                            RData::A(a) => Some(a.0),
                            _ => None,
                        })
                        .collect()),
                    Err(error) => Err(classify(&error, name, "A")),
                }
            },
            name,
            "A",
        ))
    }

    fn ipv6<'a>(&'a self, name: &'a str) -> DnsFuture<'a, Vec<Ipv6Addr>> {
        Box::pin(timed(
            async move {
                match self.inner.ipv6_lookup(name).await {
                    Ok(lookup) => Ok(lookup
                        .answers()
                        .iter()
                        .take(MAX_RECORDS)
                        .filter_map(|record| match &record.data {
                            RData::AAAA(a) => Some(a.0),
                            _ => None,
                        })
                        .collect()),
                    Err(error) => Err(classify(&error, name, "AAAA")),
                }
            },
            name,
            "AAAA",
        ))
    }

    fn mx<'a>(&'a self, name: &'a str) -> DnsFuture<'a, Vec<String>> {
        Box::pin(timed(
            async move {
                match self.inner.mx_lookup(name).await {
                    Ok(lookup) => {
                        let mut records: Vec<(u16, String)> = lookup
                            .answers()
                            .iter()
                            .take(MAX_RECORDS)
                            .filter_map(|record| match &record.data {
                                RData::MX(mx) => Some((mx.preference, mx.exchange.to_utf8())),
                                _ => None,
                            })
                            .collect();
                        records.sort_by_key(|(pref, _)| *pref);
                        Ok(records
                            .into_iter()
                            .map(|(_, host)| host.trim_end_matches('.').to_owned())
                            .collect())
                    }
                    Err(error) => Err(classify(&error, name, "MX")),
                }
            },
            name,
            "MX",
        ))
    }

    fn ptr(&self, ip: IpAddr) -> DnsFuture<'_, Vec<String>> {
        Box::pin(async move {
            match tokio::time::timeout(LOOKUP_TIMEOUT, self.inner.reverse_lookup(ip)).await {
                Ok(Ok(lookup)) => Ok(lookup
                    .answers()
                    .iter()
                    .take(MAX_RECORDS)
                    .filter_map(|record| match &record.data {
                        RData::PTR(name) => Some(name.to_utf8().trim_end_matches('.').to_owned()),
                        _ => None,
                    })
                    .collect()),
                Ok(Err(error)) => Err(classify(&error, &ip.to_string(), "PTR")),
                Err(_elapsed) => Err(DnsError::Temporary {
                    name: ip.to_string(),
                    rtype: "PTR",
                    reason: "lookup timed out".to_owned(),
                }),
            }
        })
    }
}

/// A fixture resolver for tests: answers from in-memory maps, so the
/// SPF/DKIM/DMARC evaluators are exercised without a network.
#[cfg(any(test, feature = "test-fixtures"))]
pub mod fixture {
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    use super::{DnsError, DnsFuture, Resolver};

    /// In-memory DNS for tests. Absent names resolve to `NotFound`.
    #[derive(Default)]
    pub struct FixtureResolver {
        /// name → TXT strings.
        pub txt: HashMap<String, Vec<String>>,
        /// name → A addresses.
        pub a: HashMap<String, Vec<Ipv4Addr>>,
        /// name → AAAA addresses.
        pub aaaa: HashMap<String, Vec<Ipv6Addr>>,
        /// name → MX exchange hosts (already preference-sorted).
        pub mx: HashMap<String, Vec<String>>,
        /// ip → PTR names.
        pub ptr: HashMap<IpAddr, Vec<String>>,
    }

    impl FixtureResolver {
        /// Adds a TXT record set.
        pub fn with_txt(mut self, name: &str, values: &[&str]) -> Self {
            self.txt.insert(
                name.to_ascii_lowercase(),
                values.iter().map(|v| (*v).to_owned()).collect(),
            );
            self
        }

        /// Adds an A record set.
        pub fn with_a(mut self, name: &str, values: &[Ipv4Addr]) -> Self {
            self.a.insert(name.to_ascii_lowercase(), values.to_vec());
            self
        }

        /// Adds an MX record set (in preference order).
        pub fn with_mx(mut self, name: &str, exchanges: &[&str]) -> Self {
            self.mx.insert(
                name.to_ascii_lowercase(),
                exchanges.iter().map(|v| (*v).to_owned()).collect(),
            );
            self
        }

        /// Adds a PTR record set.
        pub fn with_ptr(mut self, ip: IpAddr, names: &[&str]) -> Self {
            self.ptr
                .insert(ip, names.iter().map(|v| (*v).to_owned()).collect());
            self
        }
    }

    fn found<T: Clone + Send + 'static>(
        map: &HashMap<String, Vec<T>>,
        name: &str,
        rtype: &'static str,
    ) -> Result<Vec<T>, DnsError> {
        map.get(&name.to_ascii_lowercase())
            .cloned()
            .ok_or_else(|| DnsError::NotFound {
                name: name.to_owned(),
                rtype,
            })
    }

    impl Resolver for FixtureResolver {
        fn txt<'a>(&'a self, name: &'a str) -> DnsFuture<'a, Vec<String>> {
            Box::pin(async move { found(&self.txt, name, "TXT") })
        }
        fn ipv4<'a>(&'a self, name: &'a str) -> DnsFuture<'a, Vec<Ipv4Addr>> {
            Box::pin(async move { found(&self.a, name, "A") })
        }
        fn ipv6<'a>(&'a self, name: &'a str) -> DnsFuture<'a, Vec<Ipv6Addr>> {
            Box::pin(async move { found(&self.aaaa, name, "AAAA") })
        }
        fn mx<'a>(&'a self, name: &'a str) -> DnsFuture<'a, Vec<String>> {
            Box::pin(async move { found(&self.mx, name, "MX") })
        }
        fn ptr(&self, ip: IpAddr) -> DnsFuture<'_, Vec<String>> {
            let result = self
                .ptr
                .get(&ip)
                .cloned()
                .ok_or_else(|| DnsError::NotFound {
                    name: ip.to_string(),
                    rtype: "PTR",
                });
            Box::pin(async move { result })
        }
    }
}
