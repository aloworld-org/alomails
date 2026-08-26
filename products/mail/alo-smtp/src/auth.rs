//! SMTP authentication (RFC 4954) for the submission path: the SASL
//! PLAIN and LOGIN mechanisms and their wire decoding. Credential
//! **verification** is `alo-identity`'s job — the submission path calls
//! `Identity::authenticate_password`, which is constant-time and closes the
//! user-existence timing oracle. Passwords never appear in logs or errors;
//! only the resolved login name is surfaced (at debug).

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

/// A successfully authenticated identity. Carries the login name; the
/// resolved `(tenant, user)` lives in `alo-identity`'s `Principal`.
/// Binding the envelope sender to this identity (send-as) is deferred —
/// see `docs/design/tls-and-submission.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthIdentity {
    /// The authenticated login name (an address or bare username).
    pub username: String,
}

impl AuthIdentity {
    /// An identity for a verified login name.
    pub fn new(username: impl Into<String>) -> Self {
        Self {
            username: username.into(),
        }
    }
}

/// Decoded SASL credentials from a client.
///
/// `Debug` is hand-written to redact the password: even an accidental
/// `tracing::…(?credentials)` must never leak the secret (defense in
/// depth for a sovereignty product).
#[derive(Clone, PartialEq, Eq)]
pub struct Credentials {
    /// Login name.
    pub username: String,
    /// Secret — never logged, never stored.
    pub password: String,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

/// The SASL mechanism a client selected in `AUTH`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mechanism {
    /// `AUTH PLAIN` (RFC 4616): single `authzid\0authcid\0passwd` blob.
    Plain,
    /// `AUTH LOGIN`: username then password, each base64, via 334
    /// challenges (de-facto standard, not an RFC).
    Login,
    /// `AUTH XOAUTH2`: a `user=…^Aauth=Bearer …^A^A` blob carrying an
    /// OAuth bearer token (de-facto standard, not an RFC; see
    /// `docs/interop.md`). Parsing and verification live in
    /// `alo_identity::xoauth2`.
    XOAuth2,
}

impl Mechanism {
    /// Parses the mechanism token from an `AUTH` command (case-insensitive).
    pub fn parse(token: &str) -> Option<Self> {
        match token.to_ascii_uppercase().as_str() {
            "PLAIN" => Some(Self::Plain),
            "LOGIN" => Some(Self::Login),
            "XOAUTH2" => Some(Self::XOAuth2),
            _ => None,
        }
    }
}

/// Why a SASL exchange could not be decoded (maps to 501).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SaslError {
    /// The base64 payload did not decode.
    #[error("invalid base64 in SASL exchange")]
    BadBase64,
    /// The decoded payload was structurally wrong for the mechanism.
    #[error("malformed SASL credentials")]
    Malformed,
}

/// Decodes an `AUTH PLAIN` initial response: base64 of
/// `authzid NUL authcid NUL passwd` (RFC 4616 §2). The optional
/// authzid is ignored; authcid is the login name.
///
/// # Errors
/// [`SaslError`] on bad base64 or wrong field count.
pub fn decode_plain(b64: &str) -> Result<Credentials, SaslError> {
    let decoded = BASE64
        .decode(b64.trim())
        .map_err(|_| SaslError::BadBase64)?;
    let mut parts = decoded.split(|&b| b == 0);
    let _authzid = parts.next().ok_or(SaslError::Malformed)?;
    let authcid = parts.next().ok_or(SaslError::Malformed)?;
    let passwd = parts.next().ok_or(SaslError::Malformed)?;
    // Exactly three NUL-separated fields: nothing may follow.
    if parts.next().is_some() {
        return Err(SaslError::Malformed);
    }
    let username = String::from_utf8(authcid.to_vec()).map_err(|_| SaslError::Malformed)?;
    let password = String::from_utf8(passwd.to_vec()).map_err(|_| SaslError::Malformed)?;
    if !is_valid_username(&username) {
        return Err(SaslError::Malformed);
    }
    Ok(Credentials { username, password })
}

/// A login name must be non-empty and free of control characters: the
/// identity flows into audit logs today and into headers / store keys
/// in later milestones, so control chars (CR/LF/NUL/DEL) are rejected
/// at the boundary to prevent log/header injection and key confusion.
pub fn is_valid_username(username: &str) -> bool {
    !username.is_empty() && !username.chars().any(|c| c.is_control())
}

/// Decodes one base64 field of an `AUTH LOGIN` exchange (username or
/// password).
///
/// # Errors
/// [`SaslError::BadBase64`] when the field is not valid base64.
pub fn decode_login_field(b64: &str) -> Result<String, SaslError> {
    let decoded = BASE64
        .decode(b64.trim())
        .map_err(|_| SaslError::BadBase64)?;
    String::from_utf8(decoded).map_err(|_| SaslError::Malformed)
}

/// The base64 `334` challenges for `AUTH LOGIN` (the strings clients
/// expect: "Username:" and "Password:").
pub const LOGIN_USERNAME_CHALLENGE: &str = "VXNlcm5hbWU6";
pub const LOGIN_PASSWORD_CHALLENGE: &str = "UGFzc3dvcmQ6";

/// Decodes an `AUTH XOAUTH2` response: base64 of the
/// `user=…^Aauth=Bearer …^A^A` blob (parsed by `alo_identity::xoauth2`,
/// the one parser IMAP and SMTP share). The asserted login name obeys the
/// same boundary rule as PLAIN ([`is_valid_username`]).
///
/// # Errors
/// [`SaslError`] on bad base64 or a malformed/invalid blob.
pub fn decode_xoauth2(b64: &str) -> Result<alo_identity::xoauth2::XOAuth2Response, SaslError> {
    let decoded = BASE64
        .decode(b64.trim())
        .map_err(|_| SaslError::BadBase64)?;
    let response =
        alo_identity::xoauth2::parse_client_response(&decoded).ok_or(SaslError::Malformed)?;
    if !is_valid_username(&response.username) {
        return Err(SaslError::Malformed);
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn b64(s: &str) -> String {
        BASE64.encode(s)
    }

    #[test]
    fn mechanism_parsing_is_case_insensitive() {
        assert_eq!(Mechanism::parse("plain"), Some(Mechanism::Plain));
        assert_eq!(Mechanism::parse("LOGIN"), Some(Mechanism::Login));
        assert_eq!(Mechanism::parse("xoauth2"), Some(Mechanism::XOAuth2));
        assert_eq!(Mechanism::parse("CRAM-MD5"), None);
    }

    #[test]
    fn xoauth2_decodes_and_validates() {
        let payload = b64("user=alice@a.test\u{1}auth=Bearer tok123\u{1}\u{1}");
        let r = decode_xoauth2(&payload).unwrap();
        assert_eq!(r.username, "alice@a.test");
        assert_eq!(r.token, "tok123");
        assert_eq!(decode_xoauth2("!!!notb64"), Err(SaslError::BadBase64));
        assert_eq!(
            decode_xoauth2(&b64("auth=Bearer tok\u{1}\u{1}")),
            Err(SaslError::Malformed)
        );
        // A control character in the asserted login name is rejected at the
        // boundary, same as PLAIN.
        assert_eq!(
            decode_xoauth2(&b64("user=a\rb\u{1}auth=Bearer tok\u{1}\u{1}")),
            Err(SaslError::Malformed)
        );
    }

    #[test]
    fn plain_decodes_authcid_and_password() {
        // authzid empty, authcid "alice", passwd "s3cret"
        let payload = b64("\u{0}alice\u{0}s3cret");
        let creds = decode_plain(&payload).unwrap();
        assert_eq!(creds.username, "alice");
        assert_eq!(creds.password, "s3cret");
    }

    #[test]
    fn plain_with_authzid_ignores_it() {
        let payload = b64("admin\u{0}alice\u{0}s3cret");
        let creds = decode_plain(&payload).unwrap();
        assert_eq!(creds.username, "alice");
    }

    #[test]
    fn plain_rejects_bad_shape() {
        assert_eq!(decode_plain("!!!notb64"), Err(SaslError::BadBase64));
        assert_eq!(
            decode_plain(&b64("only\u{0}two")),
            Err(SaslError::Malformed)
        );
        assert_eq!(
            decode_plain(&b64("a\u{0}b\u{0}c\u{0}d")),
            Err(SaslError::Malformed)
        );
        assert_eq!(
            decode_plain(&b64("\u{0}\u{0}pw")),
            Err(SaslError::Malformed)
        );
    }

    #[test]
    fn login_field_round_trips() {
        assert_eq!(decode_login_field(&b64("alice")).unwrap(), "alice");
        assert_eq!(decode_login_field("!!!").unwrap_err(), SaslError::BadBase64);
    }
}
