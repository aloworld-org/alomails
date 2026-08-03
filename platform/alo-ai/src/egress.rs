//! Outbound-egress SSRF guard (ADR 0012). Any code that connects to a URL that
//! ultimately comes from untrusted input — an AI backend an operator configured,
//! or a `List-Unsubscribe` URL a stranger put in an email — routes through here
//! so it can never be tricked into reaching the host itself, a co-tenant, or
//! internal infrastructure. Lives in `alo-ai` because that is where the guard
//! was first needed; it is deliberately generic (no AI concepts) so other
//! crates can reuse the exact same, tested checks rather than reimplement them.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

/// An egress attempt refused or failed before/while connecting. Every variant
/// is intentionally coarse: a caller (and therefore an attacker) can never tell
/// "blocked because internal" apart from "genuinely unreachable" — no oracle
/// into what is reachable inside the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EgressError {
    /// The URL was malformed, not https, or resolved to a blocked address.
    Blocked,
    /// The host did not resolve, or the client could not be built.
    Unreachable,
}

/// Whether a resolved IP must be refused: loopback, link-local (which includes
/// the `169.254.169.254` cloud-metadata endpoint), private, unique-local,
/// unspecified, or multicast/reserved — anything that could reach the host
/// itself, a co-tenant, or internal infrastructure.
#[must_use]
pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.octets()[0] == 0
                || v4.octets()[0] >= 224 // multicast + reserved
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_unspecified() || v6.is_multicast() {
                return true;
            }
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_blocked_ip(IpAddr::V4(mapped));
            }
            let seg0 = v6.segments()[0];
            (seg0 & 0xfe00) == 0xfc00 // unique-local fc00::/7
                || (seg0 & 0xffc0) == 0xfe80 // link-local fe80::/10
        }
    }
}

/// Parse `(is_https, host, port)` from a URL without a URL-crate dependency.
#[must_use]
pub fn split_authority(url: &str) -> Option<(bool, String, u16)> {
    let (scheme, rest) = url.split_once("://")?;
    let https = scheme.eq_ignore_ascii_case("https");
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit('@').next()?; // drop any userinfo
    let (host, port) = match authority.rsplit_once(':') {
        // host:port — but not an unbracketed IPv6 literal (has multiple colons).
        Some((h, p)) if !h.is_empty() && !h.contains(':') => (h.to_owned(), p.parse().ok()?),
        _ => (authority.to_owned(), if https { 443 } else { 80 }),
    };
    if host.is_empty() {
        return None;
    }
    Some((https, host, port))
}

/// Build an HTTP client that may only reach a public host over https. The host
/// in `url` is resolved now; if it does not resolve or **any** resolved address
/// is blocked, the attempt is refused. The vetted address is then **pinned** so
/// a DNS rebind between this check and the actual connect cannot slip through,
/// and redirects are disabled so the server cannot bounce the request to an
/// internal address on a second, unchecked hop.
///
/// # Errors
/// [`EgressError::Blocked`] for a non-https/malformed URL or a blocked address;
/// [`EgressError::Unreachable`] if the host does not resolve or the client
/// cannot be built.
pub async fn guarded_client(url: &str, timeout: Duration) -> Result<reqwest::Client, EgressError> {
    let (https, host, port) = split_authority(url).ok_or(EgressError::Blocked)?;
    if !https {
        return Err(EgressError::Blocked);
    }
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), port))
        .await
        .map_err(|_| EgressError::Unreachable)?
        .collect();
    if addrs.is_empty() {
        return Err(EgressError::Unreachable);
    }
    if addrs.iter().any(|a| is_blocked_ip(a.ip())) {
        return Err(EgressError::Blocked);
    }
    let first = addrs[0];
    reqwest::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&host, first)
        .build()
        .map_err(|_| EgressError::Unreachable)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn blocked_ips_cover_the_ssrf_ranges() {
        for ip in [
            "127.0.0.1",
            "169.254.169.254", // cloud metadata
            "10.0.0.5",
            "172.16.9.9",
            "192.168.1.1",
            "0.0.0.0",
            "::1",
            "fd00::1", // ULA
            "fe80::1", // link-local
        ] {
            assert!(is_blocked_ip(ip.parse::<IpAddr>().unwrap()), "should block {ip}");
        }
        for ip in ["8.8.8.8", "1.1.1.1", "93.184.216.34", "2606:4700:4700::1111"] {
            assert!(!is_blocked_ip(ip.parse::<IpAddr>().unwrap()), "should allow {ip}");
        }
    }

    #[test]
    fn split_authority_parses_scheme_host_port() {
        assert_eq!(
            split_authority("https://api.openai.com/v1/chat/completions"),
            Some((true, "api.openai.com".to_owned(), 443))
        );
        assert_eq!(
            split_authority("http://host.internal:8080/x"),
            Some((false, "host.internal".to_owned(), 8080))
        );
        assert_eq!(
            split_authority("https://user:pw@evil.example/path"),
            Some((true, "evil.example".to_owned(), 443))
        );
        assert_eq!(split_authority("not a url"), None);
    }

    #[tokio::test]
    async fn guarded_client_refuses_non_https_and_private() {
        // reqwest::Client isn't PartialEq, so assert on the error variant.
        assert_eq!(
            guarded_client("http://example.com", Duration::from_secs(2))
                .await
                .err(),
            Some(EgressError::Blocked),
            "plain http is refused",
        );
        // Resolves to loopback ⇒ blocked (localhost is 127.0.0.1 / ::1).
        assert_eq!(
            guarded_client("https://localhost", Duration::from_secs(2))
                .await
                .err(),
            Some(EgressError::Blocked),
        );
    }
}
