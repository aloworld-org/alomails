//! ID-token signing keys and the JWKS. Keys are Ed25519 (EdDSA, RFC 8037 —
//! see ADR 0008 for why not RS256), stored deployment-global. The newest
//! non-retired key signs; every non-retired key's public half is published
//! in the JWKS with a stable `kid`, so rotation is publish-new →
//! sign-with-new → retire-old after a grace window.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

use crate::secret;
use crate::{Identity, IdentityError, Result};

/// The EdDSA algorithm identifier used across discovery, JWKS, and JWT.
pub const ALG_EDDSA: &str = "EdDSA";

/// A loaded active signing key: its published `kid` and the Ed25519 key.
pub struct ActiveKey {
    /// The key id (JWT `kid` header and JWKS entry).
    pub kid: String,
    /// The Ed25519 signing key.
    pub signing_key: SigningKey,
}

impl Identity {
    /// Ensures at least one signing key exists, generating one if the
    /// deployment has none (idempotent bootstrap).
    ///
    /// # Errors
    /// [`IdentityError::Crypto`] on RNG failure; [`IdentityError::Store`] on
    /// a persistence failure.
    pub async fn ensure_signing_key(&self) -> Result<()> {
        if self.store().signing_keys().await?.is_empty() {
            self.rotate_signing_key().await?;
        }
        Ok(())
    }

    /// Generates a new Ed25519 signing key and inserts it as the active key
    /// (rotation: new key signs from now; retire old keys separately after
    /// a grace window with [`Identity::retire_signing_key`]).
    ///
    /// # Errors
    /// [`IdentityError::Crypto`] on RNG failure; [`IdentityError::Store`] on
    /// a persistence failure.
    pub async fn rotate_signing_key(&self) -> Result<String> {
        let mut seed = [0u8; 32];
        secret::random_bytes(&mut seed).map_err(|_| IdentityError::Crypto)?;
        let signing_key = SigningKey::from_bytes(&seed);
        let public = signing_key.verifying_key().to_bytes();
        let kid = kid_for(&public);
        self.store()
            .insert_signing_key(&kid, ALG_EDDSA, &seed, &public)
            .await?;
        // The seed now lives in the DB row; wipe our in-memory copy.
        seed.zeroize();
        Ok(kid)
    }

    /// Retires a signing key (drops it from the JWKS and from signing).
    ///
    /// # Errors
    /// [`IdentityError::Store`] on a persistence failure.
    pub async fn retire_signing_key(&self, kid: &str) -> Result<()> {
        self.store().retire_signing_key(kid).await?;
        Ok(())
    }

    /// Loads the active (newest non-retired) signing key.
    ///
    /// # Errors
    /// [`IdentityError::NoSigningKey`] if none is provisioned;
    /// [`IdentityError::Store`] on a persistence failure.
    pub async fn active_key(&self) -> Result<ActiveKey> {
        let keys = self.store().signing_keys().await?;
        let mut newest = keys.into_iter().next().ok_or(IdentityError::NoSigningKey)?;
        let mut seed: [u8; 32] = newest
            .private_key
            .as_slice()
            .try_into()
            .map_err(|_| IdentityError::Crypto)?;
        let signing_key = SigningKey::from_bytes(&seed);
        // Wipe the transient seed copies; the live key lives in `signing_key`
        // (ed25519-dalek zeroizes its own material on drop).
        seed.zeroize();
        newest.private_key.zeroize();
        Ok(ActiveKey {
            kid: newest.kid,
            signing_key,
        })
    }

    /// Renders the JWKS document (all non-retired public keys). Loads only
    /// public key material — never the private seed (L2 hardening).
    ///
    /// # Errors
    /// [`IdentityError::Store`] on a persistence failure.
    pub async fn jwks(&self) -> Result<serde_json::Value> {
        let keys = self.store().public_signing_keys().await?;
        let jwks: Vec<serde_json::Value> = keys
            .into_iter()
            .map(|k| {
                serde_json::json!({
                    "kty": "OKP",
                    "crv": "Ed25519",
                    "alg": ALG_EDDSA,
                    "use": "sig",
                    "kid": k.kid,
                    "x": URL_SAFE_NO_PAD.encode(k.public_key),
                })
            })
            .collect();
        Ok(serde_json::json!({ "keys": jwks }))
    }
}

/// A stable key id: the first 128 bits of SHA-256(public key), base64url.
fn kid_for(public: &[u8]) -> String {
    let digest = Sha256::digest(public);
    URL_SAFE_NO_PAD.encode(&digest[..16])
}
