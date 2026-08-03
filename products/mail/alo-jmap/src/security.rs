//! Admin — Security & trust: live deliverability checks. Every result is a real
//! query made at request time — DNS for SPF / DMARC / DKIM / MX / reverse-DNS,
//! and an HTTPS fetch for the MTA-STS policy — not a display of stored config.
//! The server tells the operator exactly what is published and what to fix
//! (changing DNS is the operator's own action).

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use hickory_resolver::TokioResolver;
use hickory_resolver::proto::rr::RData;
use serde_json::{Value, json};

use crate::error::Problem;
use crate::state::{AppState, authenticate};

#[derive(Clone, Copy)]
enum Status {
    Pass,
    Warn,
    Fail,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }
}

fn item(key: &str, title: &str, status: Status, detail: String) -> Value {
    json!({ "key": key, "title": title, "status": status.as_str(), "detail": detail })
}

/// The email domain to check — the domain that appears in mailbox addresses,
/// which is where SPF / DMARC / MX must be published. This is the first
/// `ALO_SMTP_LOCAL_DOMAINS` entry (the same value alo-smtp treats as
/// local). `DOMAIN` is deliberately NOT used: it is the server FQDN
/// (`mail.example.com`), not the email domain (`example.com`). Falling back to
/// the JMAP base URL host with a leading `mail.` stripped keeps a sane default.
pub(crate) fn mail_domain(base_url: &str) -> String {
    if let Ok(v) = std::env::var("ALO_SMTP_LOCAL_DOMAINS")
        && let Some(first) = v.split(',').map(str::trim).find(|s| !s.is_empty())
    {
        return first.to_lowercase();
    }
    let host = base_url
        .split("://")
        .nth(1)
        .unwrap_or(base_url)
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("");
    host.strip_prefix("mail.").unwrap_or(host).to_lowercase()
}

/// Build a Tokio DNS resolver from the system config, or `None` if it can't be
/// constructed. Shared with the admin domain-verification path.
pub(crate) fn build_resolver() -> Option<TokioResolver> {
    TokioResolver::builder_tokio().and_then(|b| b.build()).ok()
}

/// Collect the TXT strings at `name` (each record's segments joined).
pub(crate) async fn txt_records(resolver: &TokioResolver, name: &str) -> Vec<String> {
    match resolver.txt_lookup(name).await {
        Ok(lookup) => lookup
            .answers()
            .iter()
            .filter_map(|r| match &r.data {
                RData::TXT(txt) => Some(
                    txt.txt_data
                        .iter()
                        .map(|seg| String::from_utf8_lossy(seg))
                        .collect::<String>(),
                ),
                _ => None,
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

async fn check_spf(resolver: &TokioResolver, domain: &str) -> Value {
    let records = txt_records(resolver, domain).await;
    match records
        .iter()
        .find(|r| r.to_lowercase().starts_with("v=spf1"))
    {
        Some(spf) => {
            let strict = spf.contains("-all");
            item(
                "spf",
                "SPF",
                if strict { Status::Pass } else { Status::Warn },
                if strict {
                    format!("Published: {spf}")
                } else {
                    format!("Published, but not strict (prefer `-all`): {spf}")
                },
            )
        }
        None => item(
            "spf",
            "SPF",
            Status::Fail,
            "No SPF record. Publish a TXT record starting `v=spf1` that authorizes this server."
                .to_owned(),
        ),
    }
}

async fn check_dmarc(resolver: &TokioResolver, domain: &str) -> Value {
    let records = txt_records(resolver, &format!("_dmarc.{domain}")).await;
    match records
        .iter()
        .find(|r| r.to_lowercase().starts_with("v=dmarc1"))
    {
        Some(rec) => {
            let policy = rec
                .split(';')
                .find_map(|p| p.trim().strip_prefix("p="))
                .unwrap_or("none")
                .trim()
                .to_lowercase();
            let status = if policy == "none" {
                Status::Warn
            } else {
                Status::Pass
            };
            item(
                "dmarc",
                "DMARC",
                status,
                format!("Published with policy p={policy}: {rec}"),
            )
        }
        None => item(
            "dmarc",
            "DMARC",
            Status::Fail,
            "No DMARC record. Publish TXT at `_dmarc` starting `v=DMARC1; p=...`.".to_owned(),
        ),
    }
}

async fn check_dkim(resolver: &TokioResolver, domain: &str) -> Value {
    let selector = std::env::var("ALO_SMTP_DKIM_SELECTOR")
        .ok()
        .filter(|s| !s.trim().is_empty());
    let dkim_domain = std::env::var("ALO_SMTP_DKIM_DOMAIN")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| domain.to_owned());
    let Some(selector) = selector else {
        return item(
            "dkim",
            "DKIM",
            Status::Warn,
            "DKIM is not configured on this server (set ALO_SMTP_DKIM_* and publish the key)."
                .to_owned(),
        );
    };
    let name = format!("{selector}._domainkey.{dkim_domain}");
    let records = txt_records(resolver, &name).await;
    let has_key = records
        .iter()
        .any(|r| r.contains("p=") && !r.trim_end().ends_with("p="));
    if has_key {
        item(
            "dkim",
            "DKIM",
            Status::Pass,
            format!("Key published at {name}."),
        )
    } else {
        item(
            "dkim",
            "DKIM",
            Status::Fail,
            format!("No usable DKIM key at {name}. Publish the selector's public key."),
        )
    }
}

async fn check_mx(resolver: &TokioResolver, domain: &str) -> Value {
    match resolver.mx_lookup(domain).await {
        Ok(lookup) => {
            let hosts: Vec<String> = lookup
                .answers()
                .iter()
                .filter_map(|r| match &r.data {
                    RData::MX(mx) => Some(mx.exchange.to_string().trim_end_matches('.').to_owned()),
                    _ => None,
                })
                .collect();
            if hosts.is_empty() {
                item("mx", "MX", Status::Fail, "No MX records.".to_owned())
            } else {
                item(
                    "mx",
                    "MX",
                    Status::Pass,
                    format!("Points to: {}", hosts.join(", ")),
                )
            }
        }
        Err(_) => item(
            "mx",
            "MX",
            Status::Fail,
            "No MX record. Mail cannot be routed to this server.".to_owned(),
        ),
    }
}

async fn check_ptr(resolver: &TokioResolver, domain: &str) -> Value {
    let ip = match resolver.lookup_ip(domain).await {
        Ok(lookup) => lookup.iter().next(),
        Err(_) => None,
    };
    let Some(ip) = ip else {
        return item(
            "ptr",
            "Reverse DNS (PTR)",
            Status::Warn,
            "Could not resolve the domain's address to check reverse DNS.".to_owned(),
        );
    };
    match resolver.reverse_lookup(ip).await {
        Ok(lookup) => {
            let names: Vec<String> = lookup
                .answers()
                .iter()
                .filter_map(|r| match &r.data {
                    RData::PTR(ptr) => Some(ptr.to_utf8().trim_end_matches('.').to_owned()),
                    _ => None,
                })
                .collect();
            if names.is_empty() {
                item(
                    "ptr",
                    "Reverse DNS (PTR)",
                    Status::Fail,
                    format!("{ip} has no PTR record — many providers reject unmatched senders."),
                )
            } else {
                item(
                    "ptr",
                    "Reverse DNS (PTR)",
                    Status::Pass,
                    format!("{ip} → {}", names.join(", ")),
                )
            }
        }
        Err(_) => item(
            "ptr",
            "Reverse DNS (PTR)",
            Status::Fail,
            format!("{ip} has no PTR record — set reverse DNS at your host to the mail hostname."),
        ),
    }
}

async fn check_mta_sts(resolver: &TokioResolver, domain: &str) -> Value {
    let txt = txt_records(resolver, &format!("_mta-sts.{domain}")).await;
    let has_txt = txt.iter().any(|r| r.to_lowercase().starts_with("v=stsv1"));
    let policy_url = format!("https://mta-sts.{domain}/.well-known/mta-sts.txt");
    let reachable = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()
        .map(|c| async move {
            matches!(c.get(&policy_url).send().await, Ok(r) if r.status().is_success())
        });
    let policy_ok = match reachable {
        Some(fut) => fut.await,
        None => false,
    };
    match (has_txt, policy_ok) {
        (true, true) => item(
            "mta_sts",
            "MTA-STS",
            Status::Pass,
            "TXT record and policy are published.".to_owned(),
        ),
        (true, false) => item(
            "mta_sts",
            "MTA-STS",
            Status::Warn,
            format!(
                "TXT record present but the policy at https://mta-sts.{domain}/.well-known/mta-sts.txt is unreachable."
            ),
        ),
        (false, _) => item(
            "mta_sts",
            "MTA-STS",
            Status::Warn,
            "Not published (optional, but improves inbound TLS enforcement).".to_owned(),
        ),
    }
}

/// `GET /admin/security/checks` — run the live deliverability checks.
pub async fn checks(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let domain = mail_domain(&state.base_url);

    let resolver = TokioResolver::builder_tokio()
        .and_then(|b| b.build())
        .map_err(|_| Problem::server_error())?;

    let out = vec![
        check_spf(&resolver, &domain).await,
        check_dkim(&resolver, &domain).await,
        check_dmarc(&resolver, &domain).await,
        check_mx(&resolver, &domain).await,
        check_ptr(&resolver, &domain).await,
        check_mta_sts(&resolver, &domain).await,
    ];

    Ok(Json(json!({ "domain": domain, "checks": out })))
}
