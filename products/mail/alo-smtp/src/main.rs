//! Thin binary entry point: parse config, initialize tracing, run the
//! server — nothing else lives here (new-component skill).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::process::ExitCode;

use alo_smtp::config::SmtpConfig;
use alo_smtp::{healthcheck, server};

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

/// The healthcheck runs inside the same host/container as the server;
/// a wildcard bind address is probed via loopback.
fn local_probe_addr(bind: SocketAddr) -> SocketAddr {
    if bind.ip().is_unspecified() {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), bind.port())
    } else {
        bind
    }
}
