//! Liveness probe used by the container healthcheck
//! (`alo-smtp --healthcheck`): connects to the service and expects
//! a 220 greeting, so "healthy" means "speaks SMTP", not "process
//! exists".

use std::net::SocketAddr;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

use crate::error::SmtpError;

/// Probe budget: generous enough for a loaded host, short enough that
/// the container healthcheck interval stays meaningful.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Connects to `addr` and verifies a 220 service-ready greeting
/// (RFC 5321 §3.1) arrives.
///
/// # Errors
/// Returns [`SmtpError::Unhealthy`] describing what was observed
/// instead — connect failure, timeout, or a non-220 banner.
pub async fn probe(addr: SocketAddr) -> Result<(), SmtpError> {
    let result = tokio::time::timeout(PROBE_TIMEOUT, async {
        let mut stream = TcpStream::connect(addr)
            .await
            .map_err(|error| SmtpError::Unhealthy {
                addr,
                reason: format!("connect failed: {error}"),
            })?;
        let mut banner = [0_u8; 4];
        stream
            .read_exact(&mut banner)
            .await
            .map_err(|error| SmtpError::Unhealthy {
                addr,
                reason: format!("greeting read failed: {error}"),
            })?;
        if &banner == b"220 " {
            Ok(())
        } else {
            Err(SmtpError::Unhealthy {
                addr,
                reason: format!(
                    "expected 220 greeting, got {:?}",
                    String::from_utf8_lossy(&banner)
                ),
            })
        }
    })
    .await;

    match result {
        Ok(inner) => inner,
        Err(_elapsed) => Err(SmtpError::Unhealthy {
            addr,
            reason: format!("no greeting within {PROBE_TIMEOUT:?}"),
        }),
    }
}
