//! DMARC aggregate-report delivery (RFC 7489 §7.2) — the job that
//! turns recorded evaluations into the daily `rua=` emails senders
//! rely on to monitor their own deliverability.
//!
//! The MX records one event per evaluated inbound message
//! (`dmarc_report_events`); this job runs on a timer and, for each
//! From-domain whose events have aged past the report window, fetches
//! the domain's *current* policy, validates every `rua=` destination
//! (external-destination verification, §7.1 — never mail an unrelated
//! third party on an attacker-published URI), renders the Appendix C
//! XML, gzips it, wraps it in the §7.2.1.1 report message, DKIM-signs
//! it, and enqueues it on the outbound spool. Events are deleted only
//! after the report is durably enqueued, so a crash re-sends rather
//! than loses; a transient DNS failure leaves the window untouched for
//! the next tick.

use std::sync::Arc;
use std::time::Duration;

use alo_auth_mail::dmarc::{self, AggregateRow, Disposition, DmarcPolicy};
use alo_auth_mail::resolver::{DnsError, Resolver};
use alo_store::Store;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use flate2::Compression;
use flate2::write::GzEncoder;
use std::io::Write as _;

use crate::authmail::AuthMail;
use crate::envelope::Envelope;
use crate::spool::Spool;

/// At most this many domains are reported per tick; the rest wait for
/// the next tick (oldest-first, so a backlog drains fairly).
const MAX_DOMAINS_PER_TICK: i64 = 100;
/// At most this many `<record>` rows per report (distinct source/
/// outcome groups) — a bound, not a target; overflow is logged.
const MAX_ROWS_PER_REPORT: i64 = 1000;
/// At most this many report recipients per domain (`rua=` URIs).
const MAX_RECIPIENTS: usize = 2;

/// Reporter settings, resolved from config at startup.
#[derive(Debug, Clone)]
pub struct ReporterConfig {
    /// Our organization name in the report metadata (the hostname).
    pub org_name: String,
    /// The report sender mailbox (`dmarc-reports@<domain>`).
    pub report_from: String,
    /// Events younger than this never report. `None` means "the
    /// current UTC day" — the standard daily cadence. An explicit
    /// value is the operational override (testing, catch-up).
    pub min_age: Option<Duration>,
    /// How often the job wakes to look for reportable windows.
    pub tick: Duration,
}

/// Spawns the reporter as a background task.
pub fn spawn(
    store: Arc<Store>,
    spool: Arc<Spool>,
    resolver: Arc<dyn Resolver>,
    auth: Arc<AuthMail>,
    config: ReporterConfig,
) {
    tokio::spawn(async move {
        tracing::info!(
            from = %config.report_from,
            tick_secs = config.tick.as_secs(),
            "DMARC aggregate reporting enabled"
        );
        // First pass shortly after startup (catch-up after downtime),
        // then on the configured cadence.
        tokio::time::sleep(Duration::from_secs(60)).await;
        loop {
            run_once(&store, &spool, resolver.as_ref(), &auth, &config).await;
            tokio::time::sleep(config.tick).await;
        }
    });
}

/// One sweep: report every domain whose window has closed.
pub async fn run_once(
    store: &Store,
    spool: &Spool,
    resolver: &dyn Resolver,
    auth: &AuthMail,
    config: &ReporterConfig,
) {
    let cutoff = match config.min_age {
        Some(age) => jiff::Timestamp::now().as_second() - age.as_secs() as i64,
        None => start_of_utc_day(),
    };
    let domains = match store
        .dmarc_report_domains(cutoff, MAX_DOMAINS_PER_TICK)
        .await
    {
        Ok(domains) => domains,
        Err(error) => {
            tracing::error!(%error, "dmarc reporter: domain sweep failed");
            return;
        }
    };
    for domain in domains {
        if let Err(error) =
            report_domain(store, spool, resolver, auth, config, &domain, cutoff).await
        {
            // Transient (DNS/store/spool) — the window stays recorded
            // and the next tick retries it.
            tracing::warn!(%domain, %error, "dmarc report deferred to the next tick");
        }
    }
}

/// Builds and enqueues one domain's report, or drops the window when
/// the domain publishes no usable destination. Errors are transient.
async fn report_domain(
    store: &Store,
    spool: &Spool,
    resolver: &dyn Resolver,
    auth: &AuthMail,
    config: &ReporterConfig,
    domain: &str,
    cutoff: i64,
) -> Result<(), String> {
    // The *current* policy governs where (and whether) to report.
    let discovered = match dmarc::discover_policy(resolver, domain).await {
        Ok(found) => found,
        Err(DnsError::Temporary { .. }) => return Err("policy lookup temperror".to_owned()),
        Err(DnsError::NotFound { .. }) => None,
    };
    let Some((policy, policy_domain)) = discovered else {
        // The record is gone — nothing to report to, drop the window.
        drop_window(store, domain, cutoff, "no DMARC record").await;
        return Ok(());
    };
    let recipients = match validated_recipients(resolver, domain, &policy).await {
        Ok(recipients) => recipients,
        Err(reason) => return Err(reason),
    };
    if recipients.is_empty() {
        drop_window(store, domain, cutoff, "no usable rua destination").await;
        return Ok(());
    }

    let (rows, begin) = store
        .dmarc_report_rows(domain, cutoff, MAX_ROWS_PER_REPORT)
        .await
        .map_err(|e| format!("row aggregation failed: {e}"))?;
    let Some(begin) = begin else {
        return Ok(()); // raced with a concurrent delete — nothing left
    };
    if rows.len() as i64 == MAX_ROWS_PER_REPORT {
        tracing::warn!(%domain, "dmarc report truncated at {MAX_ROWS_PER_REPORT} rows");
    }
    let report_rows: Vec<AggregateRow> = rows
        .iter()
        .map(|row| AggregateRow {
            source_ip: row.source_ip.clone(),
            count: row.count.max(0) as u64,
            disposition: Disposition::parse(&row.disposition).unwrap_or(Disposition::None),
            dkim_aligned: row.dkim_aligned,
            spf_aligned: row.spf_aligned,
            header_from: domain.to_owned(),
        })
        .collect();

    let report_id = format!("{cutoff}.{}", spool.next_id());
    let xml = dmarc::aggregate_report_xml(
        &config.org_name,
        &report_id,
        &policy_domain,
        &policy,
        begin,
        cutoff,
        &report_rows,
    );
    let gz = gzip(xml.as_bytes()).map_err(|e| format!("gzip failed: {e}"))?;
    let mut message =
        build_report_message(config, domain, &recipients, &report_id, begin, cutoff, &gz);

    // DKIM-sign the report (the outbound queue relays spooled bytes
    // as-is) — an unsigned report is still sent; many receivers accept
    // reports on SPF alone.
    if let Some(signature) = auth.sign_outbound(&message).await {
        let mut signed = Vec::with_capacity(signature.len() + message.len());
        signed.extend_from_slice(signature.as_bytes());
        signed.extend_from_slice(&message);
        message = signed;
    }

    let envelope = Envelope {
        helo: config.org_name.clone(),
        peer: "dmarc-reporter".to_owned(),
        mail_from: Some(config.report_from.clone()),
        rcpt_to: recipients.clone(),
        received_at: jiff::Timestamp::now().to_string(),
    };
    let id = spool.next_id();
    spool
        .store(&id, &envelope, &message)
        .map_err(|e| format!("spool enqueue failed: {e}"))?;

    // The report is durably enqueued — only now is the window consumed.
    match store.delete_dmarc_events(domain, cutoff).await {
        Ok(events) => {
            tracing::info!(
                %domain,
                events,
                rows = report_rows.len(),
                recipients = recipients.len(),
                "DMARC aggregate report enqueued"
            );
        }
        Err(error) => {
            // The report went out but the window survived — the next
            // tick re-sends a duplicate (informational, never lost mail).
            tracing::error!(%domain, %error, "dmarc events not deleted; a duplicate report may follow");
        }
    }
    Ok(())
}

/// Resolves the policy's `rua=` list to verified recipient addresses
/// (RFC 7489 §7.1). Internal destinations (same organizational domain
/// as the policy's From domain) are accepted as-is; external ones must
/// publish `<from>._report._dmarc.<dest>` with `v=DMARC1`, else they
/// are dropped. A temporary DNS failure defers the whole report.
async fn validated_recipients(
    resolver: &dyn Resolver,
    from_domain: &str,
    policy: &DmarcPolicy,
) -> Result<Vec<String>, String> {
    let mut recipients = Vec::new();
    for uri in &policy.rua {
        if recipients.len() >= MAX_RECIPIENTS {
            break;
        }
        let Some(address) = parse_mailto(uri) else {
            continue;
        };
        let Some((_, dest_domain)) = address.rsplit_once('@') else {
            continue;
        };
        let internal = dmarc::org_domain(dest_domain) == dmarc::org_domain(from_domain);
        if !internal {
            let name = format!("{from_domain}._report._dmarc.{dest_domain}");
            match resolver.txt(&name).await {
                Ok(txts)
                    if txts
                        .iter()
                        .any(|t| t.trim_start().to_ascii_uppercase().starts_with("V=DMARC1")) => {}
                Ok(_) | Err(DnsError::NotFound { .. }) => {
                    tracing::debug!(%from_domain, dest = %dest_domain, "external rua not authorized; dropping");
                    continue;
                }
                Err(DnsError::Temporary { .. }) => {
                    return Err("external destination verification temperror".to_owned());
                }
            }
        }
        if !recipients.contains(&address) {
            recipients.push(address);
        }
    }
    Ok(recipients)
}

/// Extracts the address from a `mailto:` report URI, dropping any
/// `!size` suffix and query part, and refusing anything that could not
/// be a plain address (control characters, missing `@`, oversized).
fn parse_mailto(uri: &str) -> Option<String> {
    let rest = uri.trim().strip_prefix("mailto:")?;
    let rest = rest.split(['?', '!']).next().unwrap_or_default();
    let address = rest.trim().to_ascii_lowercase();
    if address.len() > 320
        || !address.contains('@')
        || address.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return None;
    }
    Some(address)
}

/// Drops a window that can never be reported, so it does not pin the
/// sweep forever.
async fn drop_window(store: &Store, domain: &str, cutoff: i64, reason: &str) {
    match store.delete_dmarc_events(domain, cutoff).await {
        Ok(events) => tracing::debug!(%domain, events, reason, "dmarc window dropped"),
        Err(error) => tracing::error!(%domain, %error, "dmarc window drop failed"),
    }
}

/// The RFC 7489 §7.2.1.1 report message: a human-readable part plus
/// the gzipped XML attachment, named
/// `<receiver>!<policy-domain>!<begin>!<end>.xml.gz`.
fn build_report_message(
    config: &ReporterConfig,
    domain: &str,
    recipients: &[String],
    report_id: &str,
    begin: i64,
    end: i64,
    gz: &[u8],
) -> Vec<u8> {
    let boundary = format!("dmarc-{report_id}");
    let filename = format!("{}!{domain}!{begin}!{end}.xml.gz", config.org_name);
    let date = jiff::Zoned::now().strftime("%a, %d %b %Y %H:%M:%S %z");
    let to = recipients.join(", ");
    let b64 = wrap_base64(&BASE64.encode(gz));
    format!(
        "From: {from}\r\n\
         To: {to}\r\n\
         Subject: Report Domain: {domain} Submitter: {org} Report-ID: <{report_id}>\r\n\
         Date: {date}\r\n\
         Message-ID: <{report_id}@{org}>\r\n\
         Auto-Submitted: auto-generated\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: multipart/mixed; boundary=\"{boundary}\"\r\n\
         \r\n\
         --{boundary}\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         \r\n\
         This is a DMARC aggregate report for {domain}, generated by {org}.\r\n\
         The attachment covers {begin} to {end} (Unix seconds, UTC).\r\n\
         \r\n\
         --{boundary}\r\n\
         Content-Type: application/gzip\r\n\
         Content-Disposition: attachment; filename=\"{filename}\"\r\n\
         Content-Transfer-Encoding: base64\r\n\
         \r\n\
         {b64}\r\n\
         --{boundary}--\r\n",
        from = config.report_from,
        org = config.org_name,
    )
    .into_bytes()
}

/// Wraps base64 at 76 columns (RFC 2045 §6.8).
fn wrap_base64(b64: &str) -> String {
    let mut out = String::with_capacity(b64.len() + b64.len() / 76 * 2 + 2);
    let bytes = b64.as_bytes();
    for chunk in bytes.chunks(76) {
        if !out.is_empty() {
            out.push_str("\r\n");
        }
        // Base64 output is always ASCII; the chunk is valid UTF-8.
        out.push_str(std::str::from_utf8(chunk).unwrap_or_default());
    }
    out
}

fn gzip(data: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data)?;
    encoder.finish()
}

/// The epoch second at 00:00:00 UTC of the current day.
fn start_of_utc_day() -> i64 {
    let now = jiff::Timestamp::now().as_second();
    now - now.rem_euclid(86_400)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use alo_auth_mail::resolver::fixture::FixtureResolver;

    #[test]
    fn mailto_parsing_is_strict() {
        assert_eq!(
            parse_mailto("mailto:dmarc@example.com"),
            Some("dmarc@example.com".to_owned())
        );
        assert_eq!(
            parse_mailto(" mailto:Reports@Example.COM!10m "),
            Some("reports@example.com".to_owned())
        );
        assert_eq!(parse_mailto("https://evil.test/collect"), None);
        assert_eq!(parse_mailto("mailto:no-at-sign"), None);
        assert_eq!(parse_mailto("mailto:a@b\r\nBcc: x@y"), None);
    }

    #[tokio::test]
    async fn external_rua_requires_authorization_record() {
        let policy = DmarcPolicy::from_txt(
            "v=DMARC1; p=none; \
             rua=mailto:internal@sender.test,mailto:collector@thirdparty.test",
        )
        .unwrap();
        // No authorization TXT published → the external URI is dropped,
        // the internal one stays.
        let dns = FixtureResolver::default();
        let recipients = validated_recipients(&dns, "sender.test", &policy)
            .await
            .unwrap();
        assert_eq!(recipients, vec!["internal@sender.test".to_owned()]);

        // With the §7.1 record, the external destination is accepted.
        let dns = FixtureResolver::default()
            .with_txt("sender.test._report._dmarc.thirdparty.test", &["v=DMARC1"]);
        let recipients = validated_recipients(&dns, "sender.test", &policy)
            .await
            .unwrap();
        assert_eq!(recipients.len(), 2);
    }

    #[test]
    fn report_message_is_well_formed_and_gzip_roundtrips() {
        let config = ReporterConfig {
            org_name: "mx.alo.test".to_owned(),
            report_from: "dmarc-reports@alo.test".to_owned(),
            min_age: None,
            tick: Duration::from_secs(3600),
        };
        let xml = "<feedback>ok</feedback>";
        let gz = gzip(xml.as_bytes()).unwrap();
        let msg = build_report_message(
            &config,
            "sender.test",
            &["dmarc@sender.test".to_owned()],
            "100.42",
            50,
            100,
            &gz,
        );
        let text = String::from_utf8(msg).unwrap();
        assert!(text.contains("Subject: Report Domain: sender.test Submitter: mx.alo.test"));
        assert!(text.contains("filename=\"mx.alo.test!sender.test!50!100.xml.gz\""));
        assert!(text.contains("Auto-Submitted: auto-generated"));
        // The attachment decodes and decompresses back to the XML.
        let b64: String = text
            .split("Content-Transfer-Encoding: base64\r\n\r\n")
            .nth(1)
            .unwrap()
            .split("\r\n--")
            .next()
            .unwrap()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let compressed = BASE64.decode(b64).unwrap();
        let mut decoder = flate2::read::GzDecoder::new(&compressed[..]);
        let mut decompressed = String::new();
        std::io::Read::read_to_string(&mut decoder, &mut decompressed).unwrap();
        assert_eq!(decompressed, xml);
    }

    #[test]
    fn utc_day_start_is_midnight() {
        let start = start_of_utc_day();
        assert_eq!(start % 86_400, 0);
        assert!(start <= jiff::Timestamp::now().as_second());
    }
}
