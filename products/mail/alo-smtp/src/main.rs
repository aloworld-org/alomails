//! Thin binary entry point: parse config, initialize tracing, run the
//! server — nothing else lives here (new-component skill).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::process::ExitCode;

use alo_smtp::config::SmtpConfig;
use alo_smtp::{dkim_install, healthcheck, server};

#[tokio::main]
async fn main() -> ExitCode {
    let config = match SmtpConfig::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("alo-smtp: {error}");
            return ExitCode::FAILURE;
        }
    };

    // `--healthcheck` probes a running instance and exits; used by the
    // container HEALTHCHECK so the image needs no extra tooling.
    if std::env::args().nth(1).as_deref() == Some("--healthcheck") {
        let probe_addr = local_probe_addr(config.bind_addr);
        return match healthcheck::probe(probe_addr).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("alo-smtp: {error}");
                ExitCode::FAILURE
            }
        };
    }

    // `--install-dkim-key` installs a sending identity's signing key and exits,
    // printing the DNS record to publish. It runs here rather than as a separate
    // tool because this is the process that already has the key directory and
    // the database — the private half never has to travel to reach either.
    match dkim_install::from_args(std::env::args().skip(1)) {
        Ok(Some(request)) => return install_dkim_key(&config, &request).await,
        Ok(None) => {}
        Err(error) => {
            eprintln!("alo-smtp: {error}");
            return ExitCode::FAILURE;
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    match server::run(config).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "fatal");
            ExitCode::FAILURE
        }
    }
}

/// Runs the operator key install and prints the record to publish. Prints only
/// public material: the key itself never reaches stdout, a log or an error.
async fn install_dkim_key(config: &SmtpConfig, request: &dkim_install::InstallRequest) -> ExitCode {
    let Some(database_url) = &config.database_url else {
        eprintln!("alo-smtp: --install-dkim-key needs a database (ALO_SMTP_DATABASE_URL)");
        return ExitCode::FAILURE;
    };
    // The blob backend is unused here (this touches one small table), but the
    // store is one type: point it at the configured directory rather than
    // inventing a second construction path.
    let blobs = match alo_store::BlobStore::local(&config.blob_dir, 1) {
        Ok(blobs) => blobs,
        Err(error) => {
            eprintln!("alo-smtp: cannot open the blob store: {error}");
            return ExitCode::FAILURE;
        }
    };
    let store = match alo_store::Store::connect(database_url, blobs).await {
        Ok(store) => store,
        Err(error) => {
            eprintln!("alo-smtp: could not reach the database: {error}");
            return ExitCode::FAILURE;
        }
    };
    match dkim_install::run(&store, request).await {
        Ok(installed) => {
            println!(
                "Installed the {} key for {} (selector {}).",
                installed.algorithm, installed.domain, installed.selector
            );
            println!();
            println!("Publish this DNS record — the key signs nothing until it resolves:");
            println!();
            println!("  Host:  {}", installed.record_name);
            println!("  Type:  TXT");
            println!("  Value: {}", installed.record_value);
            println!();
            println!("(Long TXT values may need splitting into 255-char chunks at your DNS host.)");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("alo-smtp: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The healthcheck runs inside the same host/container as the server;
/// a wildcard bind address is probed via loopback.
fn local_probe_addr(bind: SocketAddr) -> SocketAddr {
    if bind.ip().is_unspecified() {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), bind.port())
    } else {
        bind
    }
}
