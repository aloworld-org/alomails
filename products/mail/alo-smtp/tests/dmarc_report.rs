//! End-to-end proof of DMARC aggregate-report delivery (RFC 7489
//! §7.2): recorded evaluations for a domain are swept by the real
//! reporter (`dmarc_reporter::run_once`), and the outbound spool must
//! receive a well-formed report — correct envelope, §7.2.1.1 subject
//! and filename, and a gzip attachment whose XML carries the recorded
//! rows. The destination comes from the domain's published `rua=`
//! (fixture DNS), and external destinations without the §7.1
//! authorization record must be refused.
//!
//! Tenancy note: the event table is host-level operational data — no
//! tenant, no message content — so the mandatory wrong-tenant test
//! does not apply; the columns are exactly the fields the report
//! discloses to the domain owner by design.
//!
//! Needs the dev Postgres (compose, or a throwaway container) at
//! `DATABASE_URL` / the 5433 default. Skips itself if none.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use alo_auth_mail::resolver::fixture::FixtureResolver;
use alo_smtp::authmail::AuthMail;
use alo_smtp::dmarc_reporter::{self, ReporterConfig};
use alo_smtp::spool::Spool;
use alo_store::{BlobStore, DmarcEventRecord, Store};
use base64::Engine;

/// The database this suite runs against.
///
/// Delegates to `alo_test_db`, which refuses the database the product
/// runs on: suites create and drop their own, they never write into `alo`.
fn database_url() -> String {
    alo_test_db::url()
}

/// The sweep is host-global (it visits every domain in the shared dev
/// database), so concurrent tests would drop each other's windows —
/// each test holds this lock across record → sweep → assert.
static SWEEP_LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
fn sweep_lock() -> &'static tokio::sync::Mutex<()> {
    SWEEP_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

async fn test_store() -> Option<Arc<Store>> {
    let Ok(store) = Store::connect(&database_url(), BlobStore::in_memory(25 * 1024 * 1024)).await
    else {
        eprintln!("SKIP: no database at {}", database_url());
        return None;
    };
    store.migrate().await.unwrap();
    Some(Arc::new(store))
}

/// A globally-unique sender domain per test run, so parallel runs
/// never see each other's events.
fn unique_domain(tag: &str) -> String {
    let nanos = jiff::Timestamp::now().as_nanosecond();
    format!("{tag}{nanos}.test")
}

fn config() -> ReporterConfig {
    ReporterConfig {
        org_name: "mx.alo.test".to_owned(),
        report_from: "dmarc-reports@alo.test".to_owned(),
        // Everything already recorded is immediately reportable.
        min_age: Some(Duration::ZERO),
        tick: Duration::from_secs(3600),
    }
}

async fn record(store: &Store, domain: &str, ip: &str, disposition: &str, n: usize) {
    for _ in 0..n {
        store
            .record_dmarc_event(&DmarcEventRecord {
                from_domain: domain.to_owned(),
                source_ip: ip.to_owned(),
                disposition: disposition.to_owned(),
                dkim_aligned: disposition == "none",
                spf_aligned: false,
            })
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn sweep_enqueues_a_valid_report_and_consumes_the_window() {
    let Some(store) = test_store().await else {
        return;
    };
    let _guard = sweep_lock().lock().await;
    let domain = unique_domain("dr");
    record(&store, &domain, "192.0.2.7", "none", 3).await;
    record(&store, &domain, "198.51.100.9", "quarantine", 2).await;

    let dns = FixtureResolver::default().with_txt(
        &format!("_dmarc.{domain}"),
        &[&format!(
            "v=DMARC1; p=quarantine; rua=mailto:reports@{domain}"
        )],
    );
    let dir = tempfile::tempdir().unwrap();
    let spool = Spool::new(dir.path()).unwrap();
    let auth = AuthMail::disabled("mx.alo.test");

    // The cutoff is whole-second; let same-second inserts age past it.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    dmarc_reporter::run_once(&store, &spool, &dns, &auth, &config()).await;

    // Exactly one report, addressed per the published rua.
    let ids = spool.list().unwrap();
    assert_eq!(ids.len(), 1, "one report enqueued");
    let (envelope, body) = spool.read(&ids[0]).unwrap();
    assert_eq!(envelope.rcpt_to, vec![format!("reports@{domain}")]);
    assert_eq!(
        envelope.mail_from.as_deref(),
        Some("dmarc-reports@alo.test")
    );

    let text = String::from_utf8_lossy(&body);
    assert!(
        text.contains(&format!(
            "Subject: Report Domain: {domain} Submitter: mx.alo.test"
        )),
        "{text}"
    );
    assert!(text.contains(&format!("filename=\"mx.alo.test!{domain}!")));

    // The attachment gunzips back to XML carrying the recorded rows.
    let b64: String = text
        .split("Content-Transfer-Encoding: base64\r\n\r\n")
        .nth(1)
        .expect("attachment part")
        .split("\r\n--")
        .next()
        .unwrap()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let gz = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .unwrap();
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut flate2::read::GzDecoder::new(&gz[..]), &mut xml).unwrap();
    assert!(xml.contains("<source_ip>192.0.2.7</source_ip>"), "{xml}");
    assert!(xml.contains("<count>3</count>"), "{xml}");
    assert!(xml.contains("<source_ip>198.51.100.9</source_ip>"), "{xml}");
    assert!(
        xml.contains("<disposition>quarantine</disposition>"),
        "{xml}"
    );
    assert!(xml.contains(&format!("<domain>{domain}</domain>")), "{xml}");

    // The window is consumed — a second sweep sends nothing.
    dmarc_reporter::run_once(&store, &spool, &dns, &auth, &config()).await;
    assert_eq!(spool.list().unwrap().len(), 1, "no duplicate report");
}

#[tokio::test]
async fn unauthorized_external_rua_sends_nothing_and_drops_the_window() {
    let Some(store) = test_store().await else {
        return;
    };
    let _guard = sweep_lock().lock().await;
    let domain = unique_domain("drext");
    record(&store, &domain, "203.0.113.5", "reject", 1).await;

    // rua points at a third party that publishes NO §7.1 authorization.
    let dns = FixtureResolver::default().with_txt(
        &format!("_dmarc.{domain}"),
        &["v=DMARC1; p=reject; rua=mailto:collector@thirdparty.test"],
    );
    let dir = tempfile::tempdir().unwrap();
    let spool = Spool::new(dir.path()).unwrap();
    let auth = AuthMail::disabled("mx.alo.test");

    tokio::time::sleep(Duration::from_millis(1500)).await;
    dmarc_reporter::run_once(&store, &spool, &dns, &auth, &config()).await;

    assert!(
        spool.list().unwrap().is_empty(),
        "no report to an unauthorized third party"
    );
    // The unreportable window was dropped, not retried forever.
    let domains = store
        .dmarc_report_domains(jiff::Timestamp::now().as_second() + 10, 1000)
        .await
        .unwrap();
    assert!(!domains.contains(&domain), "window consumed");
}

#[tokio::test]
async fn dns_temperror_keeps_the_window_for_retry() {
    let Some(store) = test_store().await else {
        return;
    };
    let _guard = sweep_lock().lock().await;
    let domain = unique_domain("drtmp");
    record(&store, &domain, "203.0.113.9", "none", 1).await;

    // The fixture resolver answers NotFound for unknown names; a
    // TEMPERROR needs this stub, which fails every lookup transiently.
    struct TempFail;
    fn temperror<'a, T: Send + 'a>(
        name: &str,
        rtype: &'static str,
    ) -> alo_auth_mail::resolver::DnsFuture<'a, T> {
        let name = name.to_owned();
        Box::pin(async move {
            Err(alo_auth_mail::resolver::DnsError::Temporary {
                name,
                rtype,
                reason: "test".to_owned(),
            })
        })
    }
    impl alo_auth_mail::resolver::Resolver for TempFail {
        fn txt<'a>(&'a self, name: &'a str) -> alo_auth_mail::resolver::DnsFuture<'a, Vec<String>> {
            temperror(name, "TXT")
        }
        fn ipv4<'a>(
            &'a self,
            name: &'a str,
        ) -> alo_auth_mail::resolver::DnsFuture<'a, Vec<std::net::Ipv4Addr>> {
            temperror(name, "A")
        }
        fn ipv6<'a>(
            &'a self,
            name: &'a str,
        ) -> alo_auth_mail::resolver::DnsFuture<'a, Vec<std::net::Ipv6Addr>> {
            temperror(name, "AAAA")
        }
        fn mx<'a>(&'a self, name: &'a str) -> alo_auth_mail::resolver::DnsFuture<'a, Vec<String>> {
            temperror(name, "MX")
        }
        fn ptr(&self, ip: std::net::IpAddr) -> alo_auth_mail::resolver::DnsFuture<'_, Vec<String>> {
            temperror(&ip.to_string(), "PTR")
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let spool = Spool::new(dir.path()).unwrap();
    let auth = AuthMail::disabled("mx.alo.test");
    tokio::time::sleep(Duration::from_millis(1500)).await;
    dmarc_reporter::run_once(&store, &spool, &TempFail, &auth, &config()).await;

    assert!(
        spool.list().unwrap().is_empty(),
        "nothing sent on temperror"
    );
    let domains = store
        .dmarc_report_domains(jiff::Timestamp::now().as_second() + 10, 1000)
        .await
        .unwrap();
    assert!(domains.contains(&domain), "window kept for the next tick");
    // Cleanup so later runs do not re-sweep this synthetic domain.
    store
        .delete_dmarc_events(&domain, jiff::Timestamp::now().as_second() + 10)
        .await
        .unwrap();
}
