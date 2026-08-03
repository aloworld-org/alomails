//! Compact JWS (RFC 7515) for OIDC ID tokens, signed EdDSA (Ed25519). We
//! assemble the token envelope ourselves — the only cryptography is the
//! Ed25519 signature (via the audited `ed25519-dalek`); the rest is
//! base64url of a JSON header and payload. Any OIDC Relying Party verifies
//! it against our JWKS.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ed25519_dalek::Signer;
use serde::Serialize;
use time::OffsetDateTime;

use crate::keys::{ALG_EDDSA, ActiveKey};
use crate::{Identity, IdentityError, Result};

/// The registered ID-token claims we emit (OIDC Core §2). `sub` is the
/// opaque stable `UserId`, never the email.
#[derive(Serialize)]
struct IdClaims<'a> {
    iss: &'a str,
    sub: &'a str,
    aud: &'a str,
    exp: i64,
    iat: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    nonce: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    email_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preferred_username: Option<&'a str>,
}

/// The identity claims available to build an ID token / userinfo response.
pub struct Claims<'a> {
    /// The subject: the opaque stable user id.
    pub sub: &'a str,
    /// The audience: the client id the token is for.
    pub aud: &'a str,
    /// The OIDC nonce from the authorization request, if any.
    pub nonce: Option<&'a str>,
    /// The user's email, if the `email` scope was granted.
    pub email: Option<&'a str>,
    /// The user's preferred username, if the `profile` scope was granted.
    pub preferred_username: Option<&'a str>,
}

impl Identity {
    /// Signs an OIDC ID token (EdDSA compact JWS) for `claims`, valid for
    /// the access-token lifetime.
    ///
    /// # Errors
    /// [`IdentityError::NoSigningKey`] if no key is provisioned;
    /// [`IdentityError::Crypto`] on a serialization failure.
    pub async fn sign_id_token(&self, claims: &Claims<'_>) -> Result<String> {
        let key = self.active_key().await?;
        let now = OffsetDateTime::now_utc();
        let exp = (now + self.config().access_ttl).unix_timestamp();
        let payload = IdClaims {
            iss: &self.config().issuer,
            sub: claims.sub,
            aud: claims.aud,
            exp,
            iat: now.unix_timestamp(),
            nonce: claims.nonce,
            email: claims.email,
            email_verified: claims.email.map(|_| true),
            preferred_username: claims.preferred_username,
        };
        sign_jws(&key, &payload)
    }
}

/// Assembles and signs a compact JWS `header.payload.signature`.
fn sign_jws<T: Serialize>(key: &ActiveKey, payload: &T) -> Result<String> {
    let header = serde_json::json!({
        "alg": ALG_EDDSA,
        "typ": "JWT",
        "kid": key.kid,
    });
    let header_b64 =
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).map_err(|_| IdentityError::Crypto)?);
    let payload_b64 =
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(payload).map_err(|_| IdentityError::Crypto)?);
    let signing_input = format!("{header_b64}.{payload_b64}");
    let signature = key.signing_key.sign(signing_input.as_bytes());
    let sig_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());
    Ok(format!("{signing_input}.{sig_b64}"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use ed25519_dalek::{Signature, SigningKey, Verifier, VerifyingKey};

    #[test]
    fn jws_verifies_against_its_public_key() {
        let seed = [7u8; 32];
        let sk = SigningKey::from_bytes(&seed);
        let vk: VerifyingKey = sk.verifying_key();
        let key = ActiveKey {
            kid: "test-kid".to_owned(),
            signing_key: sk,
        };
        let jwt = sign_jws(&key, &serde_json::json!({"sub": "u1", "iss": "id.test"})).unwrap();

        // A relying party splits header.payload.signature, verifies the
        // signature over "header.payload", and reads the claims.
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
        let signing_input = format!("{}.{}", parts[0], parts[1]);
        let sig_bytes = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
        let sig = Signature::from_slice(&sig_bytes).unwrap();
        assert!(vk.verify(signing_input.as_bytes(), &sig).is_ok());

        // Tampered payload fails.
        let bad = format!("{}.{}.{}", parts[0], parts[1], parts[2]).replace(parts[1], "eyJ4Ijoxfq");
        let bad_parts: Vec<&str> = bad.split('.').collect();
        let bad_input = format!("{}.{}", bad_parts[0], bad_parts[1]);
        assert!(vk.verify(bad_input.as_bytes(), &sig).is_err());

        let header: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
        assert_eq!(header["alg"], "EdDSA");
        assert_eq!(header["kid"], "test-kid");
    }
}
