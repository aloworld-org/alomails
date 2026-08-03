//! Thin binary entry point for the IMAP/POP3 server: read config from the
//! environment, wire the store + identity, run the listeners. All logic
//! lives in the library (`new-component` skill).
//!
//! Beyond the `ALO_IMAP_*`/`ALO_POP3_*` listener config
//! ([`Config::from_env`]), this reads:
//! - `DATABASE_URL` — the Postgres system of record (required),
//! - `ALO_BLOB_DIR` — the on-disk blob backend, shared with the SMTP
//!   service so IMAP serves the bodies SMTP delivered (required),
//! - `ALO_IDENTITY_ISSUER` — the OIDC issuer, for the credential
//!   authority (required; identity backs `LOGIN`).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use alo_identity::{Identity, IdentityConfig};
use alo_imap::Config;
use alo_store::{BlobStore, Store};

/// Per-object blob ceiling; matches the SMTP service so the shared backend
/// is consistent.
const BLOB_MAX_BYTES: usize = 50 * 1024 * 1024;

#[tokio::main]
async fn main() -> ExitCode {
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("alo-imap: {error}");
            return ExitCode::FAILURE;
        }
    };

    // `--healthcheck` probes a configured listener over loopback and exits;
    // used by the container HEALTHCHECK so the image needs no shell tooling.
    if std::env::args().nth(1).as_deref() == Some("--healthcheck") {
        return match healthcheck(&config).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("alo-imap: {error}");
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

    match run(config).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "fatal");
            ExitCode::FAILURE
        }
    }
}

/// Wires the store + identity from the environment and serves the
/// listeners. Every fatal misconfiguration is reported once, in plain text,
/// with no secret in the message.
async fn run(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let database_url = require_env("DATABASE_URL")?;
    let blob_dir = PathBuf::from(require_env("ALO_BLOB_DIR")?);
    let issuer = require_env("ALO_IDENTITY_ISSUER")?;

    let blobs = BlobStore::local(&blob_dir, BLOB_MAX_BYTES)
        .map_err(|e| format!("cannot open blob directory {}: {e}", blob_dir.display()))?;
    let store = Arc::new(
        Store::connect(&database_url, blobs)
            .await
            .map_err(|_| "cannot connect to the database")?,
    );
    // Migrations are guarded by a Postgres advisory lock, so it is safe for
    // whichever service starts first to run them.
    store
        .migrate()
        .await
        .map_err(|_| "database migration failed")?;

    let identity = Identity::new(Arc::clone(&store), IdentityConfig::new(issuer))
        .map_err(|_| "could not initialise the credential authority")?;

    tracing::info!(
        imaps = ?config.imaps_addr,
        imap = ?config.imap_addr,
        pop3s = ?config.pop3s_addr,
        "alo-imap starting"
    );
    alo_imap::serve(config, store, identity).await?;
    Ok(())
}

/// Liveness probe: succeeds if a configured listener accepts a TCP
/// connection over loopback. A TCP connect (not a full TLS/IMAP dialog)
/// keeps the probe dependency-free in a minimal image while still proving
/// the process bound its port.
async fn healthcheck(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let addr = config
        .imaps_addr
        .or(config.imap_addr)
        .or(config.pop3s_addr)
        .ok_or("no listener configured to probe")?;
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

/// A wildcard bind address is probed via loopback.
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
