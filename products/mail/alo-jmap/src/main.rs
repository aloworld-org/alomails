//! Thin binary entry point for the JMAP API + OpenID Connect provider:
//! read config from the environment, wire the store + identity, serve. All
//! logic lives in the library (`new-component` skill). The OIDC login
//! endpoints are mounted by [`alo_jmap::app`], so this one service is
//! both the native mail API and the identity provider.
//!
//! Environment:
//! - `DATABASE_URL` — the Postgres system of record (required),
//! - `ALO_BLOB_DIR` — the on-disk blob backend, shared with the SMTP and
//!   IMAP services (required),
//! - `ALO_IDENTITY_ISSUER` — the OIDC issuer URL, the public HTTPS origin
//!   this service is reached at, e.g. `https://mail.example` (required),
//! - `ALO_JMAP_ADDR` — the internal bind address (default `0.0.0.0:8080`;
//!   TLS is terminated by the front proxy),
//! - `ALO_JMAP_BASE_URL` — the external base URL for session resource
//!   links (defaults to the issuer).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use alo_identity::{Identity, IdentityConfig};
use alo_jmap::{app_state, serve};
use alo_store::{BlobStore, Store};

/// Per-object blob ceiling for content-addressed message blobs (attachments);
/// matches the SMTP + IMAP services. Large-file shares (alo Transfer) do not
/// go through this path — they stream to their own object key with no ceiling —
/// so this stays at the ordinary attachment size.
const BLOB_MAX_BYTES: usize = 50 * 1024 * 1024;
/// Default internal bind (the front proxy terminates TLS and forwards here).
const DEFAULT_ADDR: &str = "0.0.0.0:8080";

#[tokio::main]
async fn main() -> ExitCode {
    let addr = match bind_addr() {
        Ok(addr) => addr,
        Err(error) => {
            eprintln!("alo-jmap: {error}");
            return ExitCode::FAILURE;
        }
    };

    // `--healthcheck` TCP-probes the bind address over loopback and exits.
    if std::env::args().nth(1).as_deref() == Some("--healthcheck") {
        return match healthcheck(addr).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("alo-jmap: {error}");
                ExitCode::FAILURE
            }
        };
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    match run(addr).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "fatal");
            ExitCode::FAILURE
        }
    }
}

async fn run(addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    let database_url = require_env("DATABASE_URL")?;
    let blob_dir = PathBuf::from(require_env("ALO_BLOB_DIR")?);
    let issuer = require_env("ALO_IDENTITY_ISSUER")?;
    let base_url = std::env::var("ALO_JMAP_BASE_URL")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| issuer.clone());

    let blobs = BlobStore::local(&blob_dir, BLOB_MAX_BYTES)
        .map_err(|e| format!("cannot open blob directory {}: {e}", blob_dir.display()))?;
    let store = Arc::new(
        Store::connect(&database_url, blobs)
            .await
            .map_err(|_| "cannot connect to the database")?,
    );
    store
        .migrate()
        .await
        .map_err(|_| "database migration failed")?;

    // One-time backfill: compute `has_attachment` for messages ingested before
    // the column existed (migration 0022), in the background so startup is not
    // blocked. Stops when there is nothing left to compute.
    {
        let store = Arc::clone(&store);
        tokio::spawn(async move {
            loop {
                match store.backfill_has_attachment(200).await {
                    Ok(0) => break,
                    Ok(n) => tracing::info!(backfilled = n, "has_attachment backfill"),
                    Err(error) => {
                        tracing::warn!(%error, "has_attachment backfill failed");
                        break;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        });
    }

    // Background snooze sweeper (ADR-less; mirrors the vacation machinery):
    // return due snoozed messages to their owners' Inbox, unread.
    {
        let store = Arc::clone(&store);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                tick.tick().await;
                match store.sweep_snoozes().await {
                    Ok(n) if n > 0 => tracing::info!(woken = n, "snooze sweep"),
                    Ok(_) => {}
                    Err(error) => tracing::warn!(%error, "snooze sweep failed"),
                }
            }
        });
    }

    // Background site-form notifier (alo Sites, ADR 0036): deliver each new
    // contact-form submission to the site owner's inbox as an internal message.
    {
        let store = Arc::clone(&store);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                tick.tick().await;
                let delivered = alo_jmap::site_notify::run_due(&store).await;
                if delivered > 0 {
                    tracing::info!(delivered, "site-form notification sweep");
                }
            }
        });
    }

    // Background catalog-order notifier (alo Sites, ADR 0036): deliver each
    // new order request to the site owner's inbox as an internal message.
    {
        let store = Arc::clone(&store);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                tick.tick().await;
                let delivered = alo_jmap::site_order_notify::run_due(&store).await;
                if delivered > 0 {
                    tracing::info!(delivered, "catalog-order notification sweep");
                }
            }
        });
    }

    // Background booking notifier (alo Sites, ADR 0036): tell the site owner
    // about each new appointment taken on their website. The appointment is
    // already in their calendar; this is the second telling, in their inbox,
    // with the visitor reachable by one reply.
    {
        let store = Arc::clone(&store);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                tick.tick().await;
                let delivered = alo_jmap::site_booking_notify::run_due(&store).await;
                if delivered > 0 {
                    tracing::info!(delivered, "booking notification sweep");
                }
            }
        });
    }

    // Background assistant-ceiling notifier (alo Sites, ADR 0040 §3): tell
    // the site owner, once per site-month, that their assistant's spending
    // ceiling was hit and visitors are being offered the contact form.
    {
        let store = Arc::clone(&store);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                tick.tick().await;
                let delivered = alo_jmap::site_chat_notify::run_due(&store).await;
                if delivered > 0 {
                    tracing::info!(delivered, "assistant-ceiling notification sweep");
                }
            }
        });
    }

    // Background ticket-fulfilment sweeper (alo Sites, ADR 0041): make each
    // paid ticket sale good — mint the buyer's ticket, raise and settle the
    // invoice in Billing, hand the buyer to CRM — each through the owning
    // module's own door. Every 30 seconds: the buyer is watching the return
    // page their payment sent them back to.
    {
        let store = Arc::clone(&store);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                tick.tick().await;
                let fulfilled = alo_jmap::site_ticket_worker::run_due(&store).await;
                if fulfilled > 0 {
                    tracing::info!(fulfilled, "ticket fulfilment sweep");
                }
            }
        });
    }

    // Background stock-fulfilment sweeper (alo Sites, ADR 0041, S3.05a2):
    // put each paid web-shop sale on paper — the invoice in Billing, the
    // contact in CRM — through the owning module's own door. The goods moved
    // when the payment settled; this sweep owes the sale only its paper.
    {
        let store = Arc::clone(&store);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                tick.tick().await;
                let fulfilled = alo_jmap::site_stock_worker::run_due(&store).await;
                if fulfilled > 0 {
                    tracing::info!(fulfilled, "stock fulfilment sweep");
                }
            }
        });
    }

    // Background ticket-mail sweeper (alo Sites, ADR 0050): send each
    // fulfilled sale's buyer their ticket, from the deployment's own
    // transactional address through the trusted submission listener. Spawned
    // only when that address is configured — unset ALO_SITES_MAIL_FROM and
    // no mail leaves (the feature's off-switch is the same config).
    {
        let from = std::env::var("ALO_SITES_MAIL_FROM")
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| {
                !v.is_empty()
                    && v.chars().filter(|c| *c == '@').count() == 1
                    && v.chars().all(|c| !c.is_whitespace() && !c.is_control())
            });
        let submission_addr = std::env::var("ALO_JMAP_SUBMISSION_ADDR").ok();
        match (from, submission_addr) {
            (Some(from), Some(addr)) => {
                let store = Arc::clone(&store);
                tokio::spawn(async move {
                    let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
                    loop {
                        tick.tick().await;
                        let sent = alo_jmap::site_ticket_mail::run_due(&store, &addr, &from).await;
                        if sent > 0 {
                            tracing::info!(sent, "ticket mail sweep");
                        }
                    }
                });
            }
            _ => {
                tracing::info!(
                    "ticket mail sweep off: ALO_SITES_MAIL_FROM and the submission \
                     listener must both be configured (ADR 0050)"
                );
            }
        }
    }

    // Background scheduled-publish sweeper (alo Sites, ADR 0036): put each
    // website whose chosen moment has arrived on the internet, through the
    // scheduling user's own account door. Every 30 seconds, so "09:00" means
    // 09:00 to the person who chose it.
    {
        let store = Arc::clone(&store);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                tick.tick().await;
                let published = alo_jmap::site_publish_worker::run_due(&store).await;
                if published > 0 {
                    tracing::info!(published, "scheduled publish sweep");
                }
            }
        });
    }

    // Background domain-registration sweeper (alo Sites, ADR 0036): register
    // every paid domain purchase with the reseller and attach the name to its
    // website. Only in a deployment that sells domains at all — with no
    // nameservers configured nothing can be bought, so there is nothing to
    // register. Every 60 seconds: the money already moved, and a registry that
    // takes a minute to answer is normal.
    {
        let commerce = alo_jmap::sites_domain_purchases::SiteDomainCommerce::from_env();
        if commerce.sells_domains() {
            let store = Arc::clone(&store);
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
                loop {
                    tick.tick().await;
                    let live =
                        alo_jmap::site_domain_worker::run_due(&store, &commerce.registrar).await;
                    if live > 0 {
                        tracing::info!(live, "domain registration sweep");
                    }
                }
            });
        }
    }

    // Background share-expiry sweeper (alo Transfer): drop expired share links
    // and reclaim any blob no live share still holds.
    {
        let store = Arc::clone(&store);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(300));
            loop {
                tick.tick().await;
                match store.sweep_expired_shares().await {
                    Ok(n) if n > 0 => tracing::info!(expired = n, "share sweep"),
                    Ok(_) => {}
                    Err(error) => tracing::warn!(%error, "share sweep failed"),
                }
            }
        });
    }

    // Background recurring-invoice run (alo Billing B2.11): raise the DRAFT
    // every standing arrangement has come due for. Hourly rather than by the
    // minute — an arrangement bills on a day, not at an hour, so an hour's
    // latency is invisible and a run that finds nothing is the common case. It
    // raises drafts only; nothing here ever issues a document.
    {
        let store = Arc::clone(&store);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(3600));
            loop {
                tick.tick().await;
                // The server's own date, passed in rather than read inside the
                // store: the same call a tenant's own "run now" makes.
                let today = time::OffsetDateTime::now_utc().date();
                match store.sweep_billing_schedules(today).await {
                    Ok(n) if n > 0 => tracing::info!(drafted = n, "recurring invoice run"),
                    Ok(_) => {}
                    Err(error) => tracing::warn!(%error, "recurring invoice run failed"),
                }
            }
        });
    }

    // Start from the issuer default, then layer any env overrides (notably the
    // resource-server introspection secret) so a standalone product — Drive —
    // can exchange a bearer token for its principal over the wire (RFC 7662).
    let mut identity_config = IdentityConfig::new(issuer);
    if let Ok(secret) = std::env::var(alo_identity::config::ENV_INTROSPECT_SECRET) {
        let secret = secret.trim();
        if !secret.is_empty() {
            identity_config.introspect_secret = Some(alo_identity::secret::Secret::new(secret));
        }
    }
    let identity = Identity::new(Arc::clone(&store), identity_config)
        .map_err(|_| "could not initialise the credential authority")?;

    let state = app_state(store, identity, base_url);

    // Background scheduled-send sweeper (send later): submit drafts whose chosen
    // time has arrived, through the same outbound path as an interactive send.
    // Needs the full app state (submission listener address), so it starts here.
    {
        let state = state.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                tick.tick().await;
                alo_jmap::submission::run_due_scheduled(&state).await;
            }
        });
    }

    tracing::info!(%addr, "alo-jmap (API + OIDC provider) starting");
    // `serve` provisions the OIDC signing key at startup (fail-fast).
    serve(addr, state).await?;
    Ok(())
}

async fn healthcheck(addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    let probe = loopback(addr);
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::net::TcpStream::connect(probe),
    )
    .await
    .map_err(|_| "healthcheck: connection timed out")?
    .map_err(|e| format!("healthcheck: {e}"))?;
    Ok(())
}

fn bind_addr() -> Result<SocketAddr, String> {
    let raw = std::env::var("ALO_JMAP_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_owned());
    raw.parse()
        .map_err(|e| format!("ALO_JMAP_ADDR: invalid socket address {raw:?}: {e}"))
}

fn loopback(bind: SocketAddr) -> SocketAddr {
    if bind.ip().is_unspecified() {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), bind.port())
    } else {
        bind
    }
}

fn require_env(key: &str) -> Result<String, String> {
    match std::env::var(key) {
        Ok(v) if !v.is_empty() => Ok(v),
        _ => Err(format!("{key} is required")),
    }
}
