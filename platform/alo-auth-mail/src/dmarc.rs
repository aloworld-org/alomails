//! DMARC — RFC 7489. Discovers the policy for the RFC 5322 `From`
//! domain (falling back to the organizational domain via the public
//! suffix list), computes SPF and DKIM *alignment*, decides the
//! disposition, and produces aggregate-report XML (Appendix C).
//! Report *delivery* is a queue job (M2) — a follow-up.

use crate::dkim::{DkimResult, DkimVerdict};
use crate::resolver::{DnsError, Resolver};
use crate::spf::{SpfResult, SpfVerdict};

/// DMARC result (RFC 7489 §11.2 / RFC 8601).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmarcResult {
    /// At least one aligned, passing authentication method.
    Pass,
    /// Authenticated methods did not align (or failed).
    Fail,
    /// No DMARC record published.
    None,
    /// Transient DNS error during discovery.
    TempError,
    /// Malformed DMARC record.
    PermError,
}

impl DmarcResult {
    /// Token for Authentication-Results.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::None => "none",
            Self::TempError => "temperror",
            Self::PermError => "permerror",
        }
    }
}

/// The policy action to apply to a failing message (RFC 7489 §6.3 `p=`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// Take no action (monitor).
    None,
    /// Treat as suspicious (quarantine / spam folder).
    Quarantine,
    /// Reject the message.
    Reject,
}

impl Disposition {
    /// Parses a disposition token (`none`/`quarantine`/`reject`) — the
    /// inverse of [`Disposition::as_str`], used by the report job to
    /// rehydrate stored evaluations.
    pub fn parse(token: &str) -> Option<Self> {
        match token {
            "none" => Some(Self::None),
            "quarantine" => Some(Self::Quarantine),
            "reject" => Some(Self::Reject),
            _ => None,
        }
    }

    /// The token used in the aggregate report / Authentication-Results.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Quarantine => "quarantine",
            Self::Reject => "reject",
        }
    }
}

/// Alignment mode (`adkim=`/`aspf=`, §6.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Alignment {
    Relaxed,
    Strict,
}

impl Alignment {
    fn parse(token: &str) -> Self {
        match token {
            "s" => Self::Strict,
            _ => Self::Relaxed,
        }
    }
}

/// The full DMARC evaluation outcome.
#[derive(Debug, Clone)]
pub struct DmarcVerdict {
    /// Overall result.
    pub result: DmarcResult,
    /// The RFC 5322 From domain evaluated.
    pub from_domain: String,
    /// The disposition to apply (only meaningful when `result` is fail).
    pub disposition: Disposition,
    /// The published `pct=` (percent of failing mail the policy applies
    /// to, §6.6.4). The caller samples `disposition` against this before
    /// enforcing — see [`sample_disposition`].
    pub pct: u8,
    /// Whether SPF aligned with the From domain.
    pub spf_aligned: bool,
    /// Whether at least one passing DKIM signature aligned.
    pub dkim_aligned: bool,
}

/// Applies the DMARC `pct` sampling (RFC 7489 §6.6.4). For the
/// `100 - pct` percent of failing messages "sampled out", the next
/// lower policy applies: `reject` → `quarantine`, `quarantine` →
/// `none`. `roll` is a value in `0..100`; `roll < pct` enforces the
/// full policy. Kept pure (the random draw lives at the call site) so
/// the downgrade logic is deterministically testable.
pub fn sample_disposition(disposition: Disposition, pct: u8, roll: u8) -> Disposition {
    if roll < pct.min(100) {
        disposition
    } else {
        match disposition {
            Disposition::Reject => Disposition::Quarantine,
            Disposition::Quarantine | Disposition::None => Disposition::None,
        }
    }
}

/// A discovered, parsed DMARC record.
#[derive(Debug, Clone)]
pub struct DmarcPolicy {
    /// `p=` policy for the domain.
    pub policy: Disposition,
    /// `sp=` policy for subdomains (defaults to `p=`).
    pub subdomain_policy: Disposition,
    dkim_alignment: Alignment,
    spf_alignment: Alignment,
    /// `pct=` — percent of failing mail to which the policy is applied.
    pub pct: u8,
    /// `rua=` aggregate report URIs (mailto:…).
    pub rua: Vec<String>,
}

impl DmarcPolicy {
    /// Parses a DMARC TXT record value (requires `v=DMARC1` and `p=`,
    /// §6.3) — the public constructor for callers outside discovery
    /// (tests, tooling).
    pub fn from_txt(record: &str) -> Option<Self> {
        parse_policy(record)
    }
}

/// Evaluates DMARC for a message.
///
/// `from_domain` is the RFC 5322 `From` header domain. `spf` carries
/// the SPF result and the domain it authenticated (MAIL FROM or HELO).
/// `dkim` is the list of DKIM verdicts. Alignment is computed against
/// `from_domain`.
pub async fn evaluate<R: Resolver + ?Sized>(
    resolver: &R,
    from_domain: &str,
    spf: &SpfVerdict,
    dkim: &[DkimVerdict],
) -> DmarcVerdict {
    let from_domain = from_domain.trim_end_matches('.').to_ascii_lowercase();
    let fail = |result: DmarcResult| DmarcVerdict {
        result,
        from_domain: from_domain.clone(),
        disposition: Disposition::None,
        pct: 100,
        spf_aligned: false,
        dkim_aligned: false,
    };
    if from_domain.is_empty() {
        return fail(DmarcResult::None);
    }

    // Policy discovery (§6.6.3): _dmarc.<from-domain>, then the
    // organizational domain if the From domain has none.
    let (policy, policy_domain) = match discover(resolver, &from_domain).await {
        Ok(Some(found)) => found,
        Ok(None) => return fail(DmarcResult::None),
        Err(DnsError::Temporary { .. }) => return fail(DmarcResult::TempError),
        Err(DnsError::NotFound { .. }) => return fail(DmarcResult::None),
    };

    // Alignment (§3.1).
    let spf_aligned =
        spf.result == SpfResult::Pass && aligned(&spf.domain, &from_domain, policy.spf_alignment);
    let dkim_aligned = dkim.iter().any(|d| {
        d.result == DkimResult::Pass && aligned(&d.domain, &from_domain, policy.dkim_alignment)
    });

    let pass = spf_aligned || dkim_aligned;
    if pass {
        return DmarcVerdict {
            result: DmarcResult::Pass,
            from_domain,
            disposition: Disposition::None,
            pct: 100,
            spf_aligned,
            dkim_aligned,
        };
    }

    // Failing: pick the policy — subdomain policy applies when the From
    // domain is a subdomain of the policy (organizational) domain.
    let effective = if policy_domain != from_domain {
        policy.subdomain_policy
    } else {
        policy.policy
    };
    DmarcVerdict {
        result: DmarcResult::Fail,
        from_domain,
        disposition: effective,
        pct: policy.pct,
        spf_aligned,
        dkim_aligned,
    }
}

/// Whether `authenticated` aligns with `from` under the mode. Relaxed:
/// the organizational domains match. Strict: exact match. (§3.1)
fn aligned(authenticated: &str, from: &str, mode: Alignment) -> bool {
    let a = authenticated.trim_end_matches('.').to_ascii_lowercase();
    let f = from.trim_end_matches('.').to_ascii_lowercase();
    match mode {
        Alignment::Strict => a == f,
        Alignment::Relaxed => org_domain(&a) == org_domain(&f),
    }
}

/// The organizational (registrable) domain via the public suffix list
/// (§3.2). Falls back to the input when the PSL has no entry.
pub fn org_domain(domain: &str) -> String {
    match psl::domain_str(domain) {
        Some(org) => org.to_ascii_lowercase(),
        None => domain.to_ascii_lowercase(),
    }
}

/// Fetches and parses the DMARC record for `from_domain`, returning it
/// with the domain it was found at (the From domain or its
/// organizational domain). This is the same §6.6.3 discovery the
/// evaluator uses — public for the aggregate-report job, which needs
/// the published policy (and its `rua=`) at report time.
///
/// # Errors
/// [`DnsError::Temporary`] when DNS is transiently unavailable (the
/// caller should retry later rather than treat it as "no policy").
pub async fn discover_policy<R: Resolver + ?Sized>(
    resolver: &R,
    from_domain: &str,
) -> Result<Option<(DmarcPolicy, String)>, DnsError> {
    discover(resolver, from_domain).await
}

/// Fetches and parses the DMARC record, returning it with the domain
/// it was found at (the From domain or the org domain).
async fn discover<R: Resolver + ?Sized>(
    resolver: &R,
    from_domain: &str,
) -> Result<Option<(DmarcPolicy, String)>, DnsError> {
    // Try the From domain first.
    match fetch_policy(resolver, from_domain).await {
        Ok(Some(policy)) => return Ok(Some((policy, from_domain.to_owned()))),
        Ok(None) => {}
        Err(DnsError::Temporary { .. }) => {
            return Err(DnsError::Temporary {
                name: from_domain.to_owned(),
                rtype: "TXT",
                reason: "dmarc discovery".to_owned(),
            });
        }
        Err(DnsError::NotFound { .. }) => {}
    }
    // Fall back to the organizational domain (if different).
    let org = org_domain(from_domain);
    if org != from_domain
        && let Ok(Some(policy)) = fetch_policy(resolver, &org).await
    {
        return Ok(Some((policy, org)));
    }
    Ok(None)
}

async fn fetch_policy<R: Resolver + ?Sized>(
    resolver: &R,
    domain: &str,
) -> Result<Option<DmarcPolicy>, DnsError> {
    let name = format!("_dmarc.{domain}");
    let txts = resolver.txt(&name).await?;
    for txt in &txts {
        if let Some(policy) = parse_policy(txt) {
            return Ok(Some(policy));
        }
    }
    Ok(None)
}

/// Parses a DMARC TXT record. Requires `v=DMARC1` and `p=` (§6.3).
fn parse_policy(txt: &str) -> Option<DmarcPolicy> {
    let tags: Vec<(String, String)> = txt
        .split(';')
        .filter_map(|t| {
            let (k, v) = t.trim().split_once('=')?;
            Some((k.trim().to_ascii_lowercase(), v.trim().to_owned()))
        })
        .collect();
    let get = |k: &str| tags.iter().find(|(t, _)| t == k).map(|(_, v)| v.as_str());

    if !get("v").is_some_and(|v| v.eq_ignore_ascii_case("DMARC1")) {
        return None;
    }
    let policy = Disposition::parse(get("p")?)?;
    let subdomain_policy = get("sp").and_then(Disposition::parse).unwrap_or(policy);
    let pct = get("pct")
        .and_then(|p| p.parse::<u8>().ok())
        .filter(|p| *p <= 100)
        .unwrap_or(100);
    let rua = get("rua")
        .map(|r| r.split(',').map(|u| u.trim().to_owned()).collect())
        .unwrap_or_default();
    Some(DmarcPolicy {
        policy,
        subdomain_policy,
        dkim_alignment: get("adkim")
            .map(Alignment::parse)
            .unwrap_or(Alignment::Relaxed),
        spf_alignment: get("aspf")
            .map(Alignment::parse)
            .unwrap_or(Alignment::Relaxed),
        pct,
        rua,
    })
}

/// One row of an aggregate report (RFC 7489 Appendix C `<record>`).
#[derive(Debug, Clone)]
pub struct AggregateRow {
    /// Source IP of the messages.
    pub source_ip: String,
    /// Message count.
    pub count: u64,
    /// Applied disposition.
    pub disposition: Disposition,
    /// DKIM alignment result.
    pub dkim_aligned: bool,
    /// SPF alignment result.
    pub spf_aligned: bool,
    /// The header From domain.
    pub header_from: String,
}

/// Renders an aggregate report as XML (RFC 7489 Appendix C). The
/// report is produced here; sending it (gzip + email via the M2 queue)
/// is a separate job.
pub fn aggregate_report_xml(
    org_name: &str,
    report_id: &str,
    domain: &str,
    policy: &DmarcPolicy,
    begin: i64,
    end: i64,
    rows: &[AggregateRow],
) -> String {
    let esc = xml_escape;
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<feedback>\n");
    out.push_str("  <report_metadata>\n");
    out.push_str(&format!("    <org_name>{}</org_name>\n", esc(org_name)));
    out.push_str(&format!("    <report_id>{}</report_id>\n", esc(report_id)));
    out.push_str("    <date_range>\n");
    out.push_str(&format!("      <begin>{begin}</begin>\n"));
    out.push_str(&format!("      <end>{end}</end>\n"));
    out.push_str("    </date_range>\n  </report_metadata>\n");
    out.push_str("  <policy_published>\n");
    out.push_str(&format!("    <domain>{}</domain>\n", esc(domain)));
    out.push_str(&format!(
        "    <adkim>{}</adkim>\n    <aspf>{}</aspf>\n",
        alignment_char(policy.dkim_alignment),
        alignment_char(policy.spf_alignment)
    ));
    out.push_str(&format!(
        "    <p>{}</p>\n    <sp>{}</sp>\n    <pct>{}</pct>\n",
        policy.policy.as_str(),
        policy.subdomain_policy.as_str(),
        policy.pct
    ));
    out.push_str("  </policy_published>\n");
    for row in rows {
        out.push_str("  <record>\n    <row>\n");
        out.push_str(&format!(
            "      <source_ip>{}</source_ip>\n",
            esc(&row.source_ip)
        ));
        out.push_str(&format!("      <count>{}</count>\n", row.count));
        out.push_str("      <policy_evaluated>\n");
        out.push_str(&format!(
            "        <disposition>{}</disposition>\n",
            row.disposition.as_str()
        ));
        out.push_str(&format!(
            "        <dkim>{}</dkim>\n        <spf>{}</spf>\n",
            pass_fail(row.dkim_aligned),
            pass_fail(row.spf_aligned)
        ));
        out.push_str("      </policy_evaluated>\n    </row>\n");
        out.push_str("    <identifiers>\n");
        out.push_str(&format!(
            "      <header_from>{}</header_from>\n",
            esc(&row.header_from)
        ));
        out.push_str("    </identifiers>\n  </record>\n");
    }
    out.push_str("</feedback>\n");
    out
}

fn alignment_char(a: Alignment) -> char {
    match a {
        Alignment::Relaxed => 'r',
        Alignment::Strict => 's',
    }
}

fn pass_fail(aligned: bool) -> &'static str {
    if aligned { "pass" } else { "fail" }
}

fn xml_escape(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '&' => "&amp;".chars().collect::<Vec<_>>(),
            '<' => "&lt;".chars().collect(),
            '>' => "&gt;".chars().collect(),
            '"' => "&quot;".chars().collect(),
            c if c.is_control() => Vec::new(),
            c => vec![c],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::dkim::DkimResult;
    use crate::resolver::fixture::FixtureResolver;
    use crate::spf::SpfResult;

    fn spf(result: SpfResult, domain: &str) -> SpfVerdict {
        SpfVerdict {
            result,
            domain: domain.to_owned(),
            explanation: String::new(),
        }
    }

    fn dkim(result: DkimResult, domain: &str) -> DkimVerdict {
        DkimVerdict {
            result,
            domain: domain.to_owned(),
            selector: "s".to_owned(),
        }
    }

    #[tokio::test]
    async fn spf_aligned_passes() {
        let dns =
            FixtureResolver::default().with_txt("_dmarc.example.com", &["v=DMARC1; p=reject"]);
        let v = evaluate(
            &dns,
            "example.com",
            &spf(SpfResult::Pass, "example.com"),
            &[],
        )
        .await;
        assert_eq!(v.result, DmarcResult::Pass);
        assert!(v.spf_aligned);
    }

    #[tokio::test]
    async fn relaxed_dkim_alignment_via_org_domain() {
        // From = example.com, DKIM d = mail.example.com → relaxed align.
        let dns =
            FixtureResolver::default().with_txt("_dmarc.example.com", &["v=DMARC1; p=quarantine"]);
        let v = evaluate(
            &dns,
            "example.com",
            &spf(SpfResult::Fail, "other.test"),
            &[dkim(DkimResult::Pass, "mail.example.com")],
        )
        .await;
        assert_eq!(v.result, DmarcResult::Pass);
        assert!(v.dkim_aligned);
    }

    #[tokio::test]
    async fn unaligned_fail_yields_policy_disposition() {
        let dns = FixtureResolver::default().with_txt(
            "_dmarc.example.com",
            &["v=DMARC1; p=reject; aspf=s; adkim=s"],
        );
        // SPF passes for a different domain, DKIM for a different domain
        // → strict alignment fails both.
        let v = evaluate(
            &dns,
            "example.com",
            &spf(SpfResult::Pass, "bounces.example.com"),
            &[dkim(DkimResult::Pass, "other.test")],
        )
        .await;
        assert_eq!(v.result, DmarcResult::Fail);
        assert_eq!(v.disposition, Disposition::Reject);
    }

    #[tokio::test]
    async fn no_record_is_none() {
        let dns = FixtureResolver::default();
        let v = evaluate(
            &dns,
            "example.com",
            &spf(SpfResult::Pass, "example.com"),
            &[],
        )
        .await;
        assert_eq!(v.result, DmarcResult::None);
    }

    #[tokio::test]
    async fn subdomain_falls_back_to_org_policy() {
        // No _dmarc at the subdomain; org domain example.com has sp=reject.
        let dns = FixtureResolver::default()
            .with_txt("_dmarc.example.com", &["v=DMARC1; p=none; sp=reject"]);
        let v = evaluate(&dns, "sub.example.com", &spf(SpfResult::Fail, "x"), &[]).await;
        assert_eq!(v.result, DmarcResult::Fail);
        assert_eq!(v.disposition, Disposition::Reject);
    }

    #[test]
    fn aggregate_xml_is_well_formed() {
        let policy = DmarcPolicy {
            policy: Disposition::Reject,
            subdomain_policy: Disposition::Reject,
            dkim_alignment: Alignment::Relaxed,
            spf_alignment: Alignment::Relaxed,
            pct: 100,
            rua: vec!["mailto:dmarc@example.com".to_owned()],
        };
        let rows = [AggregateRow {
            source_ip: "192.0.2.1".to_owned(),
            count: 3,
            disposition: Disposition::Reject,
            dkim_aligned: false,
            spf_aligned: false,
            header_from: "example.com".to_owned(),
        }];
        let xml = aggregate_report_xml("alo", "report-1", "example.com", &policy, 100, 200, &rows);
        assert!(xml.contains("<org_name>alo</org_name>"));
        assert!(xml.contains("<source_ip>192.0.2.1</source_ip>"));
        assert!(xml.contains("<disposition>reject</disposition>"));
        assert!(xml.contains("<header_from>example.com</header_from>"));
    }

    #[test]
    fn org_domain_uses_psl() {
        assert_eq!(org_domain("mail.example.com"), "example.com");
        assert_eq!(org_domain("a.b.example.co.uk"), "example.co.uk");
    }

    #[tokio::test]
    async fn pct_is_carried_on_the_verdict() {
        let dns = FixtureResolver::default()
            .with_txt("_dmarc.example.com", &["v=DMARC1; p=reject; pct=0"]);
        let v = evaluate(&dns, "example.com", &spf(SpfResult::Fail, "x"), &[]).await;
        assert_eq!(v.result, DmarcResult::Fail);
        assert_eq!(v.disposition, Disposition::Reject);
        assert_eq!(v.pct, 0);
    }

    #[test]
    fn pct_sampling_downgrades_the_sampled_out_fraction() {
        // pct=100 always enforces; pct=0 always downgrades one level.
        assert_eq!(
            sample_disposition(Disposition::Reject, 100, 99),
            Disposition::Reject
        );
        assert_eq!(
            sample_disposition(Disposition::Reject, 0, 0),
            Disposition::Quarantine
        );
        assert_eq!(
            sample_disposition(Disposition::Reject, 0, 99),
            Disposition::Quarantine
        );
        assert_eq!(
            sample_disposition(Disposition::Quarantine, 0, 50),
            Disposition::None
        );
        // Boundary: roll == pct is "sampled out".
        assert_eq!(
            sample_disposition(Disposition::Reject, 50, 50),
            Disposition::Quarantine
        );
        assert_eq!(
            sample_disposition(Disposition::Reject, 50, 49),
            Disposition::Reject
        );
    }
}
