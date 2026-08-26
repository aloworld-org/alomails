//! SASL `XOAUTH2` for the legacy mail protocols: the client-response
//! parsing and the bearer-token verification IMAP `AUTHENTICATE` and SMTP
//! `AUTH` share, so a client that can do OAuth never needs an app
//! password. The mechanism is the de-facto standard shipped by real MUAs
//! (Thunderbird, mobile mail apps) — not an RFC; the wire shape is
//! recorded in `docs/interop.md`. Verification goes through
//! [`Identity::resolve_access_token`], the exact seam the RFC 7662
//! introspection endpoint wraps (ADR 0025), so revocation and expiry are
//! honoured immediately.
//!
//! 2FA note: an access token is only ever issued *after* the full login —
//! password and, when enrolled, the second factor — so accepting one here
//! does not weaken the fail-closed rule for 2FA accounts on legacy
//! protocols; it is the sanctioned way around it.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

use crate::{Identity, Principal, Result};

/// A parsed XOAUTH2 client response: the asserted login name and the
/// bearer token. `Debug` is hand-written to redact the token.
#[derive(Clone, PartialEq, Eq)]
pub struct XOAuth2Response {
    /// The login name the client asserts (`user=` field).
    pub username: String,
    /// The OAuth bearer token — never logged, never stored.
    pub token: String,
}

impl std::fmt::Debug for XOAuth2Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XOAuth2Response")
            .field("username", &self.username)
            .field("token", &"<redacted>")
            .finish()
    }
}

/// Parses a **base64-decoded** XOAUTH2 client response:
/// `user=<name>^Aauth=Bearer <token>^A^A` (`^A` = 0x01). Tolerant in
/// what it accepts: extra `key=value` fields (some clients add `host=`
/// and `port=`) are ignored, the `Bearer` scheme is matched
/// case-insensitively, and the trailing `^A^A` may be absent. `None`
/// when either required field is missing or empty.
pub fn parse_client_response(decoded: &[u8]) -> Option<XOAuth2Response> {
    let mut username = None;
    let mut token = None;
    for field in decoded.split(|&b| b == 0x01).filter(|f| !f.is_empty()) {
        let field = std::str::from_utf8(field).ok()?;
        let (key, value) = field.split_once('=')?;
        match key {
            "user" => username = Some(value.to_owned()),
            "auth" => {
                let (scheme, tok) = value.split_once(' ')?;
                if !scheme.eq_ignore_ascii_case("bearer") {
                    return None;
                }
                token = Some(tok.trim().to_owned());
            }
            // host=/port= and future fields: ignored, per the published
            // examples clients are written against.
            _ => {}
        }
    }
    match (username, token) {
        (Some(username), Some(token)) if !username.is_empty() && !token.is_empty() => {
            Some(XOAuth2Response { username, token })
        }
        _ => None,
    }
}

/// The base64 error status a server sends in the failure continuation of
/// an XOAUTH2 exchange (the client acknowledges it with an empty line,
/// then the protocol-level rejection follows). The JSON shape is the one
/// real clients were written against; they act on `status` only.
pub fn error_status_b64() -> String {
    BASE64.encode(r#"{"status":"401","schemes":"bearer","scope":""}"#)
}

impl Identity {
    /// Verifies an XOAUTH2 login: resolves the bearer token (unknown,
    /// expired, and revoked all fail identically) and requires the
    /// asserted username to resolve to **exactly the token's**
    /// `(tenant, user)` — a token can never log in as anyone but its own
    /// principal, across users or across tenants. Returns a scope-less
    /// [`Principal`] on success, `None` on any failure.
    ///
    /// No per-username backoff and no dummy hash here, deliberately: both
    /// paths are single indexed lookups of a 256-bit random token's
    /// SHA-256 (nothing guessable, no argon2 timing to equalize), and the
    /// common failure is an expired token that a well-behaved client
    /// refreshes and retries — backoff would punish exactly that. The
    /// per-connection failure caps in the protocols still apply.
    ///
    /// # Errors
    /// [`IdentityError::Store`](crate::IdentityError::Store) on a
    /// persistence failure.
    pub async fn authenticate_xoauth2(
        &self,
        username: &str,
        token: &str,
    ) -> Result<Option<Principal>> {
        let Some(principal) = self.resolve_access_token(token).await? else {
            return Ok(None);
        };
        let Some(cred) = self.store().credentials_by_username(username).await? else {
            return Ok(None);
        };
        if cred.tenant != principal.tenant || cred.user != principal.user {
            return Ok(None);
        }
        Ok(Some(Principal::protocol(principal.tenant, principal.user)))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn blob(s: &str) -> Vec<u8> {
        s.replace("^A", "\u{1}").into_bytes()
    }

    #[test]
    fn parses_the_canonical_shape() {
        let r = parse_client_response(&blob("user=a@b.test^Aauth=Bearer tok123^A^A")).unwrap();
        assert_eq!(r.username, "a@b.test");
        assert_eq!(r.token, "tok123");
    }

    #[test]
    fn tolerates_extra_fields_missing_trailer_and_scheme_case() {
        let r = parse_client_response(&blob(
            "user=a@b.test^Ahost=mail.test^Aport=993^Aauth=bearer tok123",
        ))
        .unwrap();
        assert_eq!(r.token, "tok123");
    }

    #[test]
    fn rejects_missing_or_empty_fields_and_wrong_scheme() {
        assert!(parse_client_response(&blob("auth=Bearer tok^A^A")).is_none());
        assert!(parse_client_response(&blob("user=a@b.test^A^A")).is_none());
        assert!(parse_client_response(&blob("user=^Aauth=Bearer tok^A^A")).is_none());
        assert!(parse_client_response(&blob("user=a@b.test^Aauth=Bearer ^A^A")).is_none());
        assert!(parse_client_response(&blob("user=a@b.test^Aauth=Basic tok^A^A")).is_none());
        assert!(parse_client_response(&blob("no-equals-sign")).is_none());
        assert!(parse_client_response(&[0xFF, 0x01]).is_none());
    }

    #[test]
    fn error_status_is_stable_base64_json() {
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(error_status_b64())
            .unwrap();
        assert_eq!(
            String::from_utf8(decoded).unwrap(),
            r#"{"status":"401","schemes":"bearer","scope":""}"#
        );
    }

    #[test]
    fn debug_redacts_the_token() {
        let r = XOAuth2Response {
            username: "a@b.test".into(),
            token: "super-secret".into(),
        };
        let dbg = format!("{r:?}");
        assert!(!dbg.contains("super-secret"), "{dbg}");
    }
}
