//! The queue scheduler: owns the periodic tick that drives
//! [`crate::queue::Queue::process_once`]. Kept separate from `queue`
//! (pure delivery logic) and `server` (inbound transport) so each has
//! one reason to change.

use std::sync::Arc;

use crate::config::OutboundConfig;
use crate::queue::{Queue, QueuePolicy, Route};
use crate::resolver::DnsResolver;
use crate::spool::Spool;

/// Spawns the background delivery loop. Failure to build the resolver
/// is logged and disables outbound rather than crashing the whole
/// server — inbound mail must keep flowing.
pub fn spawn(spool: Arc<Spool>, hostname: String, outbound: OutboundConfig) {
    let resolver = match DnsResolver::from_system() {
        Ok(resolver) => {
            // DANE (RFC 7672): TLSA over a DNSSEC-validating resolver;
            // hosts publishing secure TLSA get mandatory verified TLS.
            if outbound.dane {
                tracing::info!("DANE (TLSA) enforcement enabled for outbound delivery");
                Arc::new(resolver.with_dane())
            } else {
                tracing::warn!(
                    "DANE disabled ({}) — outbound TLS stays opportunistic everywhere",
                    crate::config::ENV_DANE
                );
                Arc::new(resolver)
            }
        }
        Err(error) => {
            tracing::error!(%error, "could not build DNS resolver; outbound disabled");
            return;
        }
    };

    let route = match outbound.smarthost {
        Some(addr) => Route::Smarthost(addr),
        None => Route::Mx,
    };
    if !outbound.egress.is_empty() {
        // An operator checking that the campaign identity really leaves by its
        // own address should not have to send a message to find out.
        tracing::info!(
            domains = ?outbound.egress.domains(),
            "per-domain egress addresses configured"
        );
    }
    let policy = QueuePolicy {
        hostname,
        route,
        retry_base: outbound.retry_base,
        retry_cap: outbound.retry_cap,
        max_attempts: outbound.max_attempts,
        rate_per_min: outbound.rate_per_min,
        rate_burst: outbound.rate_burst,
        egress: outbound.egress,
    };
    let interval = outbound.queue_interval;
    let queue = Queue::new(spool, resolver, policy);

    tokio::spawn(async move {
        tracing::info!(?interval, "outbound queue started");
        loop {
            match queue.process_once().await {
                Ok(report) if report.delivered + report.bounced + report.deferred > 0 => {
                    tracing::info!(
                        delivered = report.delivered,
                        bounced = report.bounced,
                        deferred = report.deferred,
                        "queue pass complete"
                    );
                }
                Ok(_) => {}
                Err(error) => tracing::error!(%error, "queue pass failed"),
            }
            tokio::time::sleep(interval).await;
        }
    });
}
