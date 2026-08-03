//! SPF verification — RFC 7208 `check_host()`.
//!
//! Implements the full mechanism set (`all`, `include`, `a`, `mx`,
//! `ptr`, `ip4`, `ip6`, `exists`), the `redirect`/`exp` modifiers,
//! macro expansion (§7), and the processing limits that make SPF safe
//! against amplification: at most 10 DNS-querying mechanisms
//! (§4.6.4) and at most 2 void lookups (§4.6.4) — exceeding either is
//! a hard `permerror`. DNS answers are hostile input, parsed
//! defensively; no input shape panics.

use std::net::IpAddr;

use crate::resolver::{DnsError, Resolver};

/// The result of an SPF check (RFC 7208 §2.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpfResult {
    /// The client is authorized.
    Pass,
    /// The client is explicitly not authorized (`-all`/`-mech`).
    Fail,
    /// Weak "probably not authorized" (`~all`).
    SoftFail,
    /// No assertion (`?all`, or default).
    Neutral,
    /// No SPF record published for the domain.
    None,
    /// Transient error (DNS timeout/SERVFAIL) — the caller may defer.
    TempError,
    /// The record or a limit was violated — permanent.
    PermError,
}

impl SpfResult {
    /// The token used in `Received-SPF` and `Authentication-Results`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::SoftFail => "softfail",
            Self::Neutral => "neutral",
            Self::None => "none",
            Self::TempError => "temperror",
            Self::PermError => "permerror",
        }
    }
}

/// The inputs to `check_host` (RFC 7208 §4.1): the connecting IP, the
/// HELO identity, and the MAIL FROM (its domain and full address).
#[derive(Debug, Clone)]
pub struct SpfQuery {
    /// Connecting client IP.
    pub ip: IpAddr,
    /// HELO/EHLO domain the client announced.
    pub helo: String,
    /// MAIL FROM sender; `None` for the null path `<>`, in which case
    /// the check uses the HELO identity (§2.4).
    pub mail_from: Option<Mailbox>,
}

/// A parsed sender mailbox: local part and domain.
#[derive(Debug, Clone)]
pub struct Mailbox {
    /// Local part (before `@`); `postmaster` is substituted when the
    /// reverse-path had no local part.
    pub local: String,
    /// Domain (after `@`).
    pub domain: String,
}

/// The evaluated verdict plus the domain that was checked (for the
/// `Received-SPF` and `Authentication-Results` headers).
#[derive(Debug, Clone)]
pub struct SpfVerdict {
    /// The result.
    pub result: SpfResult,
    /// The domain whose SPF policy produced it.
    pub domain: String,
    /// A short human explanation (for `Received-SPF` comment).
    pub explanation: String,
}

/// RFC 7208 §4.6.4: at most 10 mechanisms/modifiers that cause DNS
/// queries (`include`, `a`, `mx`, `ptr`, `exists`, `redirect`).
const MAX_DNS_MECHANISMS: u32 = 10;
/// RFC 7208 §4.6.4: at most 2 "void" lookups (NXDOMAIN / no records).
const MAX_VOID_LOOKUPS: u32 = 2;
/// RFC 7208 §4.6.4: `mx` may not resolve more than 10 exchanges, and
/// `ptr` more than 10 names — cap the per-mechanism work too.
const MAX_MX_HOSTS: usize = 10;
const MAX_PTR_NAMES: usize = 10;
/// Cap on `include`/`redirect` nesting depth as a belt-and-braces
/// guard beyond the mechanism budget.
const MAX_DEPTH: u32 = 20;

/// Runs SPF `check_host` for `query`, resolving through `resolver`.
/// Never fails the call: every error path maps to an [`SpfResult`].
pub async fn check_host<R: Resolver + ?Sized>(resolver: &R, query: &SpfQuery) -> SpfVerdict {
    // §2.4: with a null reverse-path, evaluate the HELO domain instead,
    // using `postmaster` as the local part.
    let (domain, sender_local) = match &query.mail_from {
        Some(mailbox) => (mailbox.domain.clone(), mailbox.local.clone()),
        None => (query.helo.clone(), "postmaster".to_owned()),
    };
    if domain.is_empty() {
        return SpfVerdict {
            result: SpfResult::None,
            domain,
            explanation: "no domain to check".to_owned(),
        };
    }

    let mut budget = Budget::default();
    let sender = format!("{sender_local}@{domain}");
    let result = check_host_inner(resolver, query, &domain, &sender, 0, &mut budget).await;
    let explanation = match result {
        SpfResult::Pass => format!("{} designates {} as permitted sender", domain, query.ip),
        SpfResult::Fail => format!(
            "{} does not designate {} as permitted sender",
            domain, query.ip
        ),
        other => format!("SPF {} for {}", other.as_str(), domain),
    };
    SpfVerdict {
        result,
        domain,
        explanation,
    }
}

/// The DNS-query and void-lookup budget shared across recursion
/// (RFC 7208 §4.6.4).
#[derive(Default)]
struct Budget {
    dns_mechanisms: u32,
    void_lookups: u32,
}

impl Budget {
    /// Charges one DNS-querying mechanism; returns false if over limit.
    fn charge_mechanism(&mut self) -> bool {
        self.dns_mechanisms += 1;
        self.dns_mechanisms <= MAX_DNS_MECHANISMS
    }

    /// Records the outcome of a lookup, charging a void lookup for a
    /// not-found answer; returns false if the void limit is exceeded.
    fn note_void<T>(&mut self, outcome: &Result<Vec<T>, DnsError>) -> bool {
        let is_void = match outcome {
            Ok(records) => records.is_empty(),
            Err(DnsError::NotFound { .. }) => true,
            Err(DnsError::Temporary { .. }) => false,
        };
        if is_void {
            self.void_lookups += 1;
        }
        self.void_lookups <= MAX_VOID_LOOKUPS
    }

    /// Charges one void lookup unconditionally (the TXT record fetch for
    /// the current/`include`/`redirect` domain came back NXDOMAIN or
    /// with no SPF record); returns false if the void limit is exceeded.
    fn charge_void(&mut self) -> bool {
        self.void_lookups += 1;
        self.void_lookups <= MAX_VOID_LOOKUPS
    }
}

/// The recursive core. `domain` is the current policy domain (changes
/// on `include`/`redirect`); `sender`/`query` stay fixed for macros.
async fn check_host_inner<R: Resolver + ?Sized>(
    resolver: &R,
    query: &SpfQuery,
    domain: &str,
    sender: &str,
    depth: u32,
    budget: &mut Budget,
) -> SpfResult {
    if depth > MAX_DEPTH {
        return SpfResult::PermError;
    }

    // Fetch and select the single SPF record (§4.5). A domain with no
    // usable SPF record is a void lookup (§4.6.4); charge it so a chain
    // of no-record targets cannot amplify beyond the void budget.
    let record = match fetch_spf_record(resolver, domain).await {
        Ok(Some(record)) => record,
        Ok(None) => {
            if !budget.charge_void() {
                return SpfResult::PermError;
            }
            return SpfResult::None;
        }
        Err(result) => return result,
    };

    let terms = match parse_terms(&record) {
        Ok(terms) => terms,
        Err(()) => return SpfResult::PermError,
    };

    let mut redirect: Option<String> = None;
    for term in &terms {
        match term {
            Term::Mechanism { qualifier, kind } => {
                match eval_mechanism(resolver, query, domain, sender, kind, depth, budget).await {
                    MechOutcome::Match => return qualifier.to_result(),
                    MechOutcome::NoMatch => {}
                    MechOutcome::Error(result) => return result,
                }
            }
            Term::Redirect(target) => redirect = Some(target.clone()),
            Term::Exp => {}     // exp only affects the explanation string
            Term::Unknown => {} // unknown modifiers are ignored (§6)
        }
    }

    // No mechanism matched. `redirect` (if present and there was no
    // `all`) transfers evaluation to the target domain (§6.1).
    if let Some(target) = redirect {
        let expanded = match expand_macro(&target, query, domain, sender) {
            Ok(expanded) => expanded,
            Err(()) => return SpfResult::PermError,
        };
        if !budget.charge_mechanism() {
            return SpfResult::PermError;
        }
        let result = Box::pin(check_host_inner(
            resolver,
            query,
            &expanded,
            sender,
            depth + 1,
            budget,
        ))
        .await;
        // §6.1: a redirect to a domain that publishes no (or a bad) SPF
        // record is a permerror, not `none`.
        return match result {
            SpfResult::None => SpfResult::PermError,
            other => other,
        };
    }

    // Default result when nothing matched and no `all` (§4.7).
    SpfResult::Neutral
}

/// Fetches the domain's SPF record, enforcing "exactly one" (§4.5).
/// `Ok(None)` = no SPF record (result `none`); `Err` = a mapped result.
async fn fetch_spf_record<R: Resolver + ?Sized>(
    resolver: &R,
    domain: &str,
) -> Result<Option<String>, SpfResult> {
    let txts = match resolver.txt(domain).await {
        Ok(txts) => txts,
        Err(DnsError::NotFound { .. }) => return Ok(None),
        Err(DnsError::Temporary { .. }) => return Err(SpfResult::TempError),
    };
    let mut spf: Vec<String> = txts.into_iter().filter(|txt| is_spf_record(txt)).collect();
    match spf.len() {
        0 => Ok(None),
        1 => Ok(Some(spf.remove(0))),
        // §4.5: more than one SPF record is a permerror.
        _ => Err(SpfResult::PermError),
    }
}

/// An SPF record starts with `v=spf1` followed by a space or end
/// (case-insensitive on the version tag), §4.5.
fn is_spf_record(txt: &str) -> bool {
    let lower = txt.to_ascii_lowercase();
    lower == "v=spf1" || lower.starts_with("v=spf1 ")
}

/// A parsed record term.
#[derive(Debug)]
enum Term {
    Mechanism {
        qualifier: Qualifier,
        kind: Mechanism,
    },
    Redirect(String),
    /// `exp=` only supplies an explanation string; we accept and ignore
    /// it (the verdict is unaffected), so no payload is retained.
    Exp,
    Unknown,
}

#[derive(Debug, Clone, Copy)]
enum Qualifier {
    Pass,
    Fail,
    SoftFail,
    Neutral,
}

impl Qualifier {
    fn to_result(self) -> SpfResult {
        match self {
            Self::Pass => SpfResult::Pass,
            Self::Fail => SpfResult::Fail,
            Self::SoftFail => SpfResult::SoftFail,
            Self::Neutral => SpfResult::Neutral,
        }
    }
}

#[derive(Debug)]
enum Mechanism {
    All,
    Include(String),
    A {
        domain: Option<String>,
        v4: u8,
        v6: u8,
    },
    Mx {
        domain: Option<String>,
        v4: u8,
        v6: u8,
    },
    Ptr(Option<String>),
    Ip4 {
        addr: std::net::Ipv4Addr,
        prefix: u8,
    },
    Ip6 {
        addr: std::net::Ipv6Addr,
        prefix: u8,
    },
    Exists(String),
}

/// Parses the record's terms after the `v=spf1` version (§4.6.1).
/// Returns `Err` on a syntactically invalid record (→ permerror).
fn parse_terms(record: &str) -> Result<Vec<Term>, ()> {
    let mut terms = Vec::new();
    // Skip the version token; split on runs of SP (§4.6.1 ABNF: terms
    // separated by one or more spaces).
    for token in record.split_whitespace().skip(1) {
        terms.push(parse_term(token)?);
    }
    Ok(terms)
}

fn parse_term(token: &str) -> Result<Term, ()> {
    // Modifiers are `name=value` (§6). `redirect` and `exp` are known;
    // other `name=macro` modifiers are ignored.
    if let Some((name, value)) = token.split_once('=')
        && !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        // Only treat as a modifier when the name is a pure keyword —
        // i.e. it is not itself a mechanism like `ip4:...`.
        && !name.contains(':')
    {
        return Ok(match name.to_ascii_lowercase().as_str() {
            "redirect" => Term::Redirect(value.to_owned()),
            "exp" => Term::Exp,
            _ => Term::Unknown,
        });
    }

    // Mechanism: optional qualifier, then name, then optional value.
    let (qualifier, rest) = match token.as_bytes().first() {
        Some(b'+') => (Qualifier::Pass, &token[1..]),
        Some(b'-') => (Qualifier::Fail, &token[1..]),
        Some(b'~') => (Qualifier::SoftFail, &token[1..]),
        Some(b'?') => (Qualifier::Neutral, &token[1..]),
        _ => (Qualifier::Pass, token),
    };
    let kind = parse_mechanism(rest)?;
    Ok(Term::Mechanism { qualifier, kind })
}

fn parse_mechanism(text: &str) -> Result<Mechanism, ()> {
    // `split_name` keeps `/` in the value (a/mx CIDR) but drops a
    // leading `:` (include/exists/ip4/ip6 argument).
    let (name, value) = split_name(text);
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "all" => Ok(Mechanism::All),
        "include" => Ok(Mechanism::Include(require_domain(value)?)),
        "exists" => Ok(Mechanism::Exists(require_domain(value)?)),
        "ptr" => Ok(Mechanism::Ptr(optional_domain(value))),
        "a" => {
            let (domain, v4, v6) = parse_dual_cidr_with_domain(value)?;
            Ok(Mechanism::A { domain, v4, v6 })
        }
        "mx" => {
            let (domain, v4, v6) = parse_dual_cidr_with_domain(value)?;
            Ok(Mechanism::Mx { domain, v4, v6 })
        }
        "ip4" => {
            let (addr, prefix) = parse_ip4(value.ok_or(())?)?;
            Ok(Mechanism::Ip4 { addr, prefix })
        }
        "ip6" => {
            let (addr, prefix) = parse_ip6(value.ok_or(())?)?;
            Ok(Mechanism::Ip6 { addr, prefix })
        }
        _ => Err(()),
    }
}

/// Splits a mechanism token into (name, remainder-including-delim-body).
/// The remainder is everything after the name's first `:` or `/`.
fn split_name(text: &str) -> (&str, Option<&str>) {
    match text.find([':', '/']) {
        Some(idx) => {
            // Keep `/` in the value for CIDR (a/mx), drop a leading `:`.
            if text.as_bytes()[idx] == b':' {
                (&text[..idx], Some(&text[idx + 1..]))
            } else {
                (&text[..idx], Some(&text[idx..]))
            }
        }
        None => (text, None),
    }
}

fn require_domain(value: Option<&str>) -> Result<String, ()> {
    match value {
        Some(v) if !v.is_empty() && !v.starts_with('/') => Ok(v.to_owned()),
        _ => Err(()),
    }
}

fn optional_domain(value: Option<&str>) -> Option<String> {
    value
        .filter(|v| !v.is_empty() && !v.starts_with('/'))
        .map(str::to_owned)
}

/// Parses the `a`/`mx` value: an optional domain then optional
/// `/v4cidr` and `//v6cidr` (§5.3, §5.4).
fn parse_dual_cidr_with_domain(value: Option<&str>) -> Result<(Option<String>, u8, u8), ()> {
    let Some(value) = value else {
        return Ok((None, 32, 128));
    };
    // Separate domain (before first `/`) from the CIDR part.
    let (domain_part, cidr_part) = match value.find('/') {
        Some(idx) => (&value[..idx], Some(&value[idx..])),
        None => (value, None),
    };
    let domain = if domain_part.is_empty() {
        None
    } else {
        Some(domain_part.to_owned())
    };
    let (v4, v6) = parse_dual_cidr(cidr_part)?;
    Ok((domain, v4, v6))
}

/// Parses `/v4` and/or `//v6` CIDR lengths (§5.6 dual-cidr-length).
fn parse_dual_cidr(cidr: Option<&str>) -> Result<(u8, u8), ()> {
    let Some(cidr) = cidr else {
        return Ok((32, 128));
    };
    // Forms: `/n`, `//m`, `/n//m`.
    if let Some(v6) = cidr.strip_prefix("//") {
        let v6 = parse_prefix(v6, 128)?;
        return Ok((32, v6));
    }
    let cidr = cidr.strip_prefix('/').ok_or(())?;
    match cidr.split_once("//") {
        Some((v4, v6)) => Ok((parse_prefix(v4, 32)?, parse_prefix(v6, 128)?)),
        None => Ok((parse_prefix(cidr, 32)?, 128)),
    }
}

fn parse_prefix(text: &str, max: u8) -> Result<u8, ()> {
    let n: u8 = text.parse().map_err(|_| ())?;
    if n <= max { Ok(n) } else { Err(()) }
}

fn parse_ip4(value: &str) -> Result<(std::net::Ipv4Addr, u8), ()> {
    let (addr, prefix) = match value.split_once('/') {
        Some((a, p)) => (a, parse_prefix(p, 32)?),
        None => (value, 32),
    };
    Ok((addr.parse().map_err(|_| ())?, prefix))
}

fn parse_ip6(value: &str) -> Result<(std::net::Ipv6Addr, u8), ()> {
    let (addr, prefix) = match value.split_once('/') {
        Some((a, p)) => (a, parse_prefix(p, 128)?),
        None => (value, 128),
    };
    Ok((addr.parse().map_err(|_| ())?, prefix))
}

enum MechOutcome {
    Match,
    NoMatch,
    Error(SpfResult),
}

#[allow(clippy::too_many_arguments)]
async fn eval_mechanism<R: Resolver + ?Sized>(
    resolver: &R,
    query: &SpfQuery,
    domain: &str,
    sender: &str,
    mechanism: &Mechanism,
    depth: u32,
    budget: &mut Budget,
) -> MechOutcome {
    match mechanism {
        Mechanism::All => MechOutcome::Match,

        Mechanism::Ip4 { addr, prefix } => match query.ip {
            IpAddr::V4(ip) if ipv4_in(ip, *addr, *prefix) => MechOutcome::Match,
            _ => MechOutcome::NoMatch,
        },
        Mechanism::Ip6 { addr, prefix } => match query.ip {
            IpAddr::V6(ip) if ipv6_in(ip, *addr, *prefix) => MechOutcome::Match,
            _ => MechOutcome::NoMatch,
        },

        Mechanism::Include(target) => {
            if !budget.charge_mechanism() {
                return MechOutcome::Error(SpfResult::PermError);
            }
            let expanded = match expand_macro(target, query, domain, sender) {
                Ok(e) => e,
                Err(()) => return MechOutcome::Error(SpfResult::PermError),
            };
            // §5.2: include's result maps — pass→match, fail/softfail/
            // neutral/none→no-match, temperror/permerror propagate.
            let sub = Box::pin(check_host_inner(
                resolver,
                query,
                &expanded,
                sender,
                depth + 1,
                budget,
            ))
            .await;
            match sub {
                SpfResult::Pass => MechOutcome::Match,
                SpfResult::Fail | SpfResult::SoftFail | SpfResult::Neutral => MechOutcome::NoMatch,
                SpfResult::None => MechOutcome::Error(SpfResult::PermError),
                SpfResult::TempError => MechOutcome::Error(SpfResult::TempError),
                SpfResult::PermError => MechOutcome::Error(SpfResult::PermError),
            }
        }

        Mechanism::Exists(target) => {
            if !budget.charge_mechanism() {
                return MechOutcome::Error(SpfResult::PermError);
            }
            let expanded = match expand_macro(target, query, domain, sender) {
                Ok(e) => e,
                Err(()) => return MechOutcome::Error(SpfResult::PermError),
            };
            let outcome = resolver.ipv4(&expanded).await;
            if !budget.note_void(&outcome) {
                return MechOutcome::Error(SpfResult::PermError);
            }
            match outcome {
                Ok(addrs) if !addrs.is_empty() => MechOutcome::Match,
                Ok(_) | Err(DnsError::NotFound { .. }) => MechOutcome::NoMatch,
                Err(DnsError::Temporary { .. }) => MechOutcome::Error(SpfResult::TempError),
            }
        }

        Mechanism::A { domain: d, v4, v6 } => {
            if !budget.charge_mechanism() {
                return MechOutcome::Error(SpfResult::PermError);
            }
            let target = match resolve_target(d, domain, query, sender) {
                Ok(t) => t,
                Err(()) => return MechOutcome::Error(SpfResult::PermError),
            };
            match_a(resolver, query.ip, &target, *v4, *v6, budget).await
        }

        Mechanism::Mx { domain: d, v4, v6 } => {
            if !budget.charge_mechanism() {
                return MechOutcome::Error(SpfResult::PermError);
            }
            let target = match resolve_target(d, domain, query, sender) {
                Ok(t) => t,
                Err(()) => return MechOutcome::Error(SpfResult::PermError),
            };
            let mx = resolver.mx(&target).await;
            if !budget.note_void(&mx) {
                return MechOutcome::Error(SpfResult::PermError);
            }
            let hosts = match mx {
                Ok(hosts) => hosts,
                Err(DnsError::NotFound { .. }) => return MechOutcome::NoMatch,
                Err(DnsError::Temporary { .. }) => return MechOutcome::Error(SpfResult::TempError),
            };
            // §4.6.4: cap MX hosts examined at 10.
            for host in hosts.into_iter().take(MAX_MX_HOSTS) {
                match match_a(resolver, query.ip, &host, *v4, *v6, budget).await {
                    MechOutcome::Match => return MechOutcome::Match,
                    MechOutcome::Error(e) => return MechOutcome::Error(e),
                    MechOutcome::NoMatch => {}
                }
            }
            MechOutcome::NoMatch
        }

        Mechanism::Ptr(d) => {
            // §5.5: ptr is discouraged (slow, unreliable) but must be
            // implemented. Validate that a PTR name for the client IP
            // resolves back to an address matching the client IP, and
            // ends in the target domain.
            if !budget.charge_mechanism() {
                return MechOutcome::Error(SpfResult::PermError);
            }
            let target = match resolve_target(d, domain, query, sender) {
                Ok(t) => t,
                Err(()) => return MechOutcome::Error(SpfResult::PermError),
            };
            let ptr = resolver.ptr(query.ip).await;
            if !budget.note_void(&ptr) {
                return MechOutcome::Error(SpfResult::PermError);
            }
            let names = match ptr {
                Ok(names) => names,
                Err(DnsError::NotFound { .. }) => return MechOutcome::NoMatch,
                Err(DnsError::Temporary { .. }) => return MechOutcome::Error(SpfResult::TempError),
            };
            for name in names.into_iter().take(MAX_PTR_NAMES) {
                // The PTR name must validate (forward-confirm) AND be
                // within the target domain.
                if name_in_domain(&name, &target)
                    && matches!(
                        match_a(resolver, query.ip, &name, 32, 128, budget).await,
                        MechOutcome::Match
                    )
                {
                    return MechOutcome::Match;
                }
            }
            MechOutcome::NoMatch
        }
    }
}

/// Resolves the domain a mechanism operates on: its explicit macro
/// argument if present, else the current policy domain (§5.3/§5.4).
fn resolve_target(
    explicit: &Option<String>,
    domain: &str,
    query: &SpfQuery,
    sender: &str,
) -> Result<String, ()> {
    match explicit {
        Some(arg) => expand_macro(arg, query, domain, sender),
        None => Ok(domain.to_owned()),
    }
}

/// Resolves A/AAAA for `target` and tests the client IP against them
/// under the given CIDR lengths.
async fn match_a<R: Resolver + ?Sized>(
    resolver: &R,
    ip: IpAddr,
    target: &str,
    v4: u8,
    v6: u8,
    budget: &mut Budget,
) -> MechOutcome {
    match ip {
        IpAddr::V4(client) => {
            let outcome = resolver.ipv4(target).await;
            if !budget.note_void(&outcome) {
                return MechOutcome::Error(SpfResult::PermError);
            }
            match outcome {
                Ok(addrs) => {
                    if addrs.iter().any(|a| ipv4_in(client, *a, v4)) {
                        MechOutcome::Match
                    } else {
                        MechOutcome::NoMatch
                    }
                }
                Err(DnsError::NotFound { .. }) => MechOutcome::NoMatch,
                Err(DnsError::Temporary { .. }) => MechOutcome::Error(SpfResult::TempError),
            }
        }
        IpAddr::V6(client) => {
            let outcome = resolver.ipv6(target).await;
            if !budget.note_void(&outcome) {
                return MechOutcome::Error(SpfResult::PermError);
            }
            match outcome {
                Ok(addrs) => {
                    if addrs.iter().any(|a| ipv6_in(client, *a, v6)) {
                        MechOutcome::Match
                    } else {
                        MechOutcome::NoMatch
                    }
                }
                Err(DnsError::NotFound { .. }) => MechOutcome::NoMatch,
                Err(DnsError::Temporary { .. }) => MechOutcome::Error(SpfResult::TempError),
            }
        }
    }
}

fn ipv4_in(ip: std::net::Ipv4Addr, net: std::net::Ipv4Addr, prefix: u8) -> bool {
    if prefix == 0 {
        return true;
    }
    if prefix > 32 {
        return false;
    }
    let mask = u32::MAX << (32 - prefix);
    (u32::from(ip) & mask) == (u32::from(net) & mask)
}

fn ipv6_in(ip: std::net::Ipv6Addr, net: std::net::Ipv6Addr, prefix: u8) -> bool {
    if prefix == 0 {
        return true;
    }
    if prefix > 128 {
        return false;
    }
    let mask = u128::MAX << (128 - prefix);
    (u128::from(ip) & mask) == (u128::from(net) & mask)
}

/// Whether `name` is within `domain` (equal, or a subdomain).
fn name_in_domain(name: &str, domain: &str) -> bool {
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    let domain = domain.trim_end_matches('.').to_ascii_lowercase();
    name == domain || name.ends_with(&format!(".{domain}"))
}

/// Expands SPF macros (§7). Supports the common macro letters
/// (`s`,`l`,`o`,`d`,`i`,`h`,`v`) with transformers (digits, `r`,
/// delimiters). Unknown macro letters are a permerror (`Err`).
fn expand_macro(input: &str, query: &SpfQuery, domain: &str, sender: &str) -> Result<String, ()> {
    if !input.contains('%') {
        return Ok(input.to_owned());
    }
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some('_') => out.push(' '),
            Some('-') => out.push_str("%20"),
            Some('{') => {
                let mut spec = String::new();
                for sc in chars.by_ref() {
                    if sc == '}' {
                        break;
                    }
                    spec.push(sc);
                }
                out.push_str(&expand_macro_spec(&spec, query, domain, sender)?);
            }
            // A lone `%` not starting a valid macro is illegal (§7.1).
            _ => return Err(()),
        }
    }
    Ok(out)
}

fn expand_macro_spec(
    spec: &str,
    query: &SpfQuery,
    domain: &str,
    sender: &str,
) -> Result<String, ()> {
    let mut chars = spec.chars();
    let letter = chars.next().ok_or(())?;
    let rest: String = chars.collect();

    let base = match letter.to_ascii_lowercase() {
        's' => sender.to_owned(),
        'l' => sender.split('@').next().unwrap_or("").to_owned(),
        'o' => sender.split('@').nth(1).unwrap_or("").to_owned(),
        'd' => domain.to_owned(),
        'i' => match query.ip {
            IpAddr::V4(v4) => v4.to_string(),
            // §7.1: IPv6 `i` is the dotted-nibble form.
            IpAddr::V6(v6) => v6
                .octets()
                .iter()
                .flat_map(|b| [b >> 4, b & 0x0f])
                .map(|n| format!("{n:x}"))
                .collect::<Vec<_>>()
                .join("."),
        },
        'h' => query.helo.clone(),
        'v' => match query.ip {
            IpAddr::V4(_) => "in-addr".to_owned(),
            IpAddr::V6(_) => "ip6".to_owned(),
        },
        _ => return Err(()),
    };

    // Transformer: optional digit count, optional `r` (reverse), and
    // custom delimiters (§7.1).
    let mut digits = String::new();
    let mut reverse = false;
    let mut delimiters = ".".to_owned();
    let mut delim_chars = String::new();
    for tc in rest.chars() {
        match tc {
            d if d.is_ascii_digit() => digits.push(d),
            'r' | 'R' => reverse = true,
            other => delim_chars.push(other),
        }
    }
    if !delim_chars.is_empty() {
        delimiters = delim_chars;
    }

    let mut parts: Vec<&str> = base.split(|c| delimiters.contains(c)).collect();
    if reverse {
        parts.reverse();
    }
    if !digits.is_empty() {
        let n: usize = digits.parse().map_err(|_| ())?;
        if n == 0 {
            return Err(());
        }
        if parts.len() > n {
            parts = parts.split_off(parts.len() - n);
        }
    }
    Ok(parts.join("."))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use std::net::Ipv4Addr;

    use crate::resolver::fixture::FixtureResolver;

    use super::*;

    fn query(ip: &str, from: &str) -> SpfQuery {
        let (local, domain) = from.split_once('@').unwrap_or(("postmaster", from));
        SpfQuery {
            ip: ip.parse().unwrap(),
            helo: "mail.example.com".to_owned(),
            mail_from: Some(Mailbox {
                local: local.to_owned(),
                domain: domain.to_owned(),
            }),
        }
    }

    #[tokio::test]
    async fn ip4_pass_and_fail() {
        let dns =
            FixtureResolver::default().with_txt("example.com", &["v=spf1 ip4:192.0.2.0/24 -all"]);
        let pass = check_host(&dns, &query("192.0.2.5", "a@example.com")).await;
        assert_eq!(pass.result, SpfResult::Pass);
        let fail = check_host(&dns, &query("198.51.100.9", "a@example.com")).await;
        assert_eq!(fail.result, SpfResult::Fail);
    }

    #[tokio::test]
    async fn softfail_and_neutral_and_none() {
        let dns = FixtureResolver::default()
            .with_txt("soft.example", &["v=spf1 ~all"])
            .with_txt("neutral.example", &["v=spf1 ?all"]);
        assert_eq!(
            check_host(&dns, &query("192.0.2.5", "a@soft.example"))
                .await
                .result,
            SpfResult::SoftFail
        );
        assert_eq!(
            check_host(&dns, &query("192.0.2.5", "a@neutral.example"))
                .await
                .result,
            SpfResult::Neutral
        );
        // No SPF record at all.
        assert_eq!(
            check_host(&dns, &query("192.0.2.5", "a@absent.example"))
                .await
                .result,
            SpfResult::None
        );
    }

    #[tokio::test]
    async fn a_and_mx_mechanisms() {
        let dns = FixtureResolver::default()
            .with_txt("example.com", &["v=spf1 a mx -all"])
            .with_a("example.com", &[Ipv4Addr::new(192, 0, 2, 1)])
            .with_mx("example.com", &["mail.example.com"])
            .with_a("mail.example.com", &[Ipv4Addr::new(192, 0, 2, 2)]);
        // Matches the A record.
        assert_eq!(
            check_host(&dns, &query("192.0.2.1", "a@example.com"))
                .await
                .result,
            SpfResult::Pass
        );
        // Matches an MX host's A record.
        assert_eq!(
            check_host(&dns, &query("192.0.2.2", "a@example.com"))
                .await
                .result,
            SpfResult::Pass
        );
        // Neither → fail.
        assert_eq!(
            check_host(&dns, &query("203.0.113.9", "a@example.com"))
                .await
                .result,
            SpfResult::Fail
        );
    }

    #[tokio::test]
    async fn include_pass_propagates() {
        let dns = FixtureResolver::default()
            .with_txt("example.com", &["v=spf1 include:_spf.provider.test -all"])
            .with_txt("_spf.provider.test", &["v=spf1 ip4:192.0.2.0/24 -all"]);
        assert_eq!(
            check_host(&dns, &query("192.0.2.7", "a@example.com"))
                .await
                .result,
            SpfResult::Pass
        );
        assert_eq!(
            check_host(&dns, &query("203.0.113.7", "a@example.com"))
                .await
                .result,
            SpfResult::Fail
        );
    }

    #[tokio::test]
    async fn redirect_modifier() {
        let dns = FixtureResolver::default()
            .with_txt("example.com", &["v=spf1 redirect=_spf.example.com"])
            .with_txt("_spf.example.com", &["v=spf1 ip4:192.0.2.0/24 -all"]);
        assert_eq!(
            check_host(&dns, &query("192.0.2.7", "a@example.com"))
                .await
                .result,
            SpfResult::Pass
        );
    }

    #[tokio::test]
    async fn redirect_to_domain_without_record_is_permerror() {
        // §6.1: redirect target with no SPF record → permerror (not none).
        let dns = FixtureResolver::default()
            .with_txt("example.com", &["v=spf1 redirect=missing.example"]);
        assert_eq!(
            check_host(&dns, &query("192.0.2.7", "a@example.com"))
                .await
                .result,
            SpfResult::PermError
        );
    }

    #[tokio::test]
    async fn ten_dns_mechanism_limit_is_permerror() {
        // A record that chains 11 includes exceeds the §4.6.4 limit.
        let mut dns = FixtureResolver::default();
        let chain: String = (0..11)
            .map(|i| format!("include:i{i}.example"))
            .collect::<Vec<_>>()
            .join(" ");
        dns = dns.with_txt("example.com", &[&format!("v=spf1 {chain} -all")]);
        for i in 0..11 {
            // each include points at a record that itself just says -all
            let name = format!("i{i}.example");
            dns.txt.insert(name, vec!["v=spf1 -all".to_owned()]);
        }
        assert_eq!(
            check_host(&dns, &query("192.0.2.7", "a@example.com"))
                .await
                .result,
            SpfResult::PermError
        );
    }

    #[tokio::test]
    async fn temperror_on_dns_failure() {
        // No record for the domain at all in a resolver that returns
        // Temporary — simulate by an empty fixture (NotFound = none),
        // so instead test the multi-record permerror path.
        let dns = FixtureResolver::default().with_txt(
            "example.com",
            &["v=spf1 ip4:192.0.2.0/24 -all", "v=spf1 -all"],
        );
        assert_eq!(
            check_host(&dns, &query("192.0.2.7", "a@example.com"))
                .await
                .result,
            SpfResult::PermError
        );
    }

    #[tokio::test]
    async fn malformed_record_is_permerror() {
        let dns =
            FixtureResolver::default().with_txt("example.com", &["v=spf1 ip4:not-an-ip -all"]);
        assert_eq!(
            check_host(&dns, &query("192.0.2.7", "a@example.com"))
                .await
                .result,
            SpfResult::PermError
        );
    }

    #[tokio::test]
    async fn null_sender_uses_helo() {
        let dns = FixtureResolver::default()
            .with_txt("mail.example.com", &["v=spf1 ip4:192.0.2.0/24 -all"]);
        let q = SpfQuery {
            ip: "192.0.2.5".parse().unwrap(),
            helo: "mail.example.com".to_owned(),
            mail_from: None,
        };
        let verdict = check_host(&dns, &q).await;
        assert_eq!(verdict.result, SpfResult::Pass);
        assert_eq!(verdict.domain, "mail.example.com");
    }

    #[test]
    fn macro_expansion() {
        let q = SpfQuery {
            ip: "192.0.2.3".parse().unwrap(),
            helo: "mail.example.com".to_owned(),
            mail_from: Some(Mailbox {
                local: "strong-bad".to_owned(),
                domain: "email.example.com".to_owned(),
            }),
        };
        // §7.4 examples.
        assert_eq!(
            expand_macro(
                "%{ir}.%{v}._spf.%{d2}",
                &q,
                "email.example.com",
                "strong-bad@email.example.com"
            )
            .unwrap(),
            "3.2.0.192.in-addr._spf.example.com"
        );
        assert_eq!(
            expand_macro(
                "%{lr-}.lp._spf.%{d}",
                &q,
                "email.example.com",
                "strong-bad@email.example.com"
            )
            .unwrap(),
            "bad.strong.lp._spf.email.example.com"
        );
        // Unknown macro letter → permerror.
        assert!(expand_macro("%{z}", &q, "d", "s@d").is_err());
    }
}
