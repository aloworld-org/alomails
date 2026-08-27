//! The Web Push wire crypto (mail M5.3): VAPID request signing (RFC 8292)
//! and `aes128gcm` payload encryption toward one browser (RFC 8291 over
//! RFC 8188). Pure functions over `ring` — no I/O, no store; the dispatcher
//! in `push_notify` owns *what* is sent and *to whom*, this module owns only
//! how bytes become a message a push service will relay and only the
//! subscribed browser can read.
//!
//! Everything a push service sees is either public (the VAPID public key,
//! the signed claims) or opaque ciphertext; the payload plaintext itself
//! carries ids and counts only (the dispatcher's contract), so even a
//! decryption is never message content.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::signature::KeyPair as _;
use ring::{aead, agreement, hkdf, rand, signature};

/// How long a VAPID token vouches for us: 12 hours, half the 24-hour
/// ceiling RFC 8292 §2 sets, so clock skew at the push service never turns
/// a fresh token into an expired one.
const VAPID_TOKEN_LIFETIME_SECS: i64 = 12 * 60 * 60;

/// The `rs` (record size) written into the `aes128gcm` header. One push
/// payload is always a single record — 4096 is the customary value and
/// far above any counts-and-ids payload.
const RECORD_SIZE: u32 = 4096;

/// A failure preparing Web Push material. Deliberately unstructured: every
/// case is either bad key material from config/storage or a crypto
/// primitive refusing garbage, and none of them carries anything a caller
/// could retry differently.
#[derive(Debug, thiserror::Error)]
#[error("web push crypto failure: {0}")]
pub struct WebPushError(&'static str);

/// The application server's VAPID identity: the ES256 key that signs
/// request tokens and the contact the tokens carry. Built once at startup
/// from config; the private key never leaves this struct.
pub struct VapidKeys {
    /// The ECDSA P-256 key, PKCS#8 — `ring`'s `EcdsaKeyPair` is rebuilt per
    /// signature (it is not `Clone`, and pushes are low-rate).
    pkcs8: Vec<u8>,
    /// The uncompressed public point, base64url — what browsers pass to
    /// `pushManager.subscribe` as `applicationServerKey`, verbatim.
    public_b64: String,
    /// The `sub` claim (RFC 8292 §2.1) — a `mailto:` or `https:` contact
    /// the push service can reach us at about misbehaving traffic.
    subject: String,
}

impl VapidKeys {
    /// Builds the identity from a base64url PKCS#8 key and a contact
    /// subject.
    ///
    /// # Errors
    /// [`WebPushError`] when the key does not decode to a P-256 signing key
    /// or the subject is not a `mailto:`/`https:` URI.
    pub fn new(key_b64: &str, subject: &str) -> Result<Self, WebPushError> {
        let subject = subject.trim();
        if !subject.starts_with("mailto:") && !subject.starts_with("https://") {
            return Err(WebPushError("VAPID subject must be mailto: or https:"));
        }
        let pkcs8 = URL_SAFE_NO_PAD
            .decode(key_b64.trim())
            .map_err(|_| WebPushError("VAPID key is not base64url"))?;
        let rng = rand::SystemRandom::new();
        let pair = signature::EcdsaKeyPair::from_pkcs8(
            &signature::ECDSA_P256_SHA256_FIXED_SIGNING,
            &pkcs8,
            &rng,
        )
        .map_err(|_| WebPushError("VAPID key is not a P-256 PKCS#8 key"))?;
        let public_b64 = URL_SAFE_NO_PAD.encode(pair.public_key().as_ref());
        Ok(Self {
            pkcs8,
            public_b64,
            subject: subject.to_owned(),
        })
    }

    /// Mints a fresh VAPID key as the base64url PKCS#8 text `ALO_VAPID_KEY`
    /// carries — the shape [`VapidKeys::new`] accepts.
    ///
    /// # Errors
    /// [`WebPushError`] only if the system RNG fails.
    pub fn generate_key_b64() -> Result<String, WebPushError> {
        let rng = rand::SystemRandom::new();
        let doc = signature::EcdsaKeyPair::generate_pkcs8(
            &signature::ECDSA_P256_SHA256_FIXED_SIGNING,
            &rng,
        )
        .map_err(|_| WebPushError("key generation failed"))?;
        Ok(URL_SAFE_NO_PAD.encode(doc.as_ref()))
    }

    /// The public key a browser subscribes with (`applicationServerKey`),
    /// base64url of the uncompressed P-256 point.
    #[must_use]
    pub fn public_key_b64(&self) -> &str {
        &self.public_b64
    }

    /// The `Authorization: vapid t=…, k=…` header value for one request to
    /// `endpoint_origin` (scheme://host[:port] — the token's `aud`), valid
    /// for the next twelve hours.
    ///
    /// # Errors
    /// [`WebPushError`] if signing fails (a corrupt key — never input).
    pub fn authorization(&self, endpoint_origin: &str) -> Result<String, WebPushError> {
        let header = URL_SAFE_NO_PAD.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
        let exp = time::OffsetDateTime::now_utc().unix_timestamp() + VAPID_TOKEN_LIFETIME_SECS;
        let claims = URL_SAFE_NO_PAD.encode(
            serde_json::json!({ "aud": endpoint_origin, "exp": exp, "sub": self.subject })
                .to_string(),
        );
        let signing_input = format!("{header}.{claims}");
        let rng = rand::SystemRandom::new();
        let pair = signature::EcdsaKeyPair::from_pkcs8(
            &signature::ECDSA_P256_SHA256_FIXED_SIGNING,
            &self.pkcs8,
            &rng,
        )
        .map_err(|_| WebPushError("VAPID key no longer parses"))?;
        let sig = pair
            .sign(&rng, signing_input.as_bytes())
            .map_err(|_| WebPushError("VAPID signing failed"))?;
        let jwt = format!("{signing_input}.{}", URL_SAFE_NO_PAD.encode(sig.as_ref()));
        Ok(format!("vapid t={jwt}, k={}", self.public_b64))
    }
}

/// Encrypts one payload for one subscription (RFC 8291): a fresh ephemeral
/// P-256 key and random salt per message, ECDH against the browser's
/// `p256dh` key, and the two-stage HKDF into a single `aes128gcm` record.
/// Returns the complete request body (header + ciphertext).
///
/// # Errors
/// [`WebPushError`] when the subscription's key material is malformed —
/// the caller treats that subscription as dead.
pub fn encrypt(
    ua_public_b64: &str,
    auth_b64: &str,
    plaintext: &[u8],
) -> Result<Vec<u8>, WebPushError> {
    let ua_public = URL_SAFE_NO_PAD
        .decode(ua_public_b64.trim().trim_end_matches('='))
        .map_err(|_| WebPushError("subscription p256dh is not base64url"))?;
    if ua_public.len() != 65 || ua_public[0] != 0x04 {
        return Err(WebPushError(
            "subscription p256dh is not an uncompressed P-256 point",
        ));
    }
    let auth = URL_SAFE_NO_PAD
        .decode(auth_b64.trim().trim_end_matches('='))
        .map_err(|_| WebPushError("subscription auth is not base64url"))?;
    if auth.len() != 16 {
        return Err(WebPushError("subscription auth secret is not 16 bytes"));
    }

    let rng = rand::SystemRandom::new();
    let ephemeral = agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng)
        .map_err(|_| WebPushError("ephemeral key generation failed"))?;
    let as_public = ephemeral
        .compute_public_key()
        .map_err(|_| WebPushError("ephemeral public key derivation failed"))?;
    let as_public = as_public.as_ref().to_vec();

    let mut salt = [0u8; 16];
    ring::rand::SecureRandom::fill(&rng, &mut salt)
        .map_err(|_| WebPushError("salt generation failed"))?;

    let peer = agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, ua_public.clone());
    agreement::agree_ephemeral(ephemeral, &peer, |ecdh_secret| {
        encrypt_content(ecdh_secret, &ua_public, &as_public, &auth, &salt, plaintext)
    })
    .map_err(|_| WebPushError("ECDH agreement failed — bad subscription key"))?
}

/// The deterministic half of [`encrypt`], from the ECDH secret down — split
/// out so RFC 8291's Appendix A vector (fixed keys, fixed salt) can pin
/// every derivation byte-for-byte in a test, which `ring`'s
/// ephemeral-only ECDH API cannot do through the full path.
fn encrypt_content(
    ecdh_secret: &[u8],
    ua_public: &[u8],
    as_public: &[u8],
    auth: &[u8],
    salt: &[u8; 16],
    plaintext: &[u8],
) -> Result<Vec<u8>, WebPushError> {
    // RFC 8291 §3.3–3.4: IKM = HKDF(salt=auth, ecdh_secret, "WebPush: info"
    // || 0x00 || ua_public || as_public, 32).
    let mut key_info = Vec::with_capacity(14 + 65 + 65);
    key_info.extend_from_slice(b"WebPush: info\x00");
    key_info.extend_from_slice(ua_public);
    key_info.extend_from_slice(as_public);
    let mut ikm = [0u8; 32];
    hkdf_derive(auth, ecdh_secret, &key_info, &mut ikm)?;

    // RFC 8188 §2.2–2.3: CEK and nonce from the message salt.
    let mut cek = [0u8; 16];
    hkdf_derive(salt, &ikm, b"Content-Encoding: aes128gcm\x00", &mut cek)?;
    let mut nonce = [0u8; 12];
    hkdf_derive(salt, &ikm, b"Content-Encoding: nonce\x00", &mut nonce)?;

    // One record: plaintext, the 0x02 last-record delimiter, then the tag.
    let key = aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::AES_128_GCM, &cek)
            .map_err(|_| WebPushError("CEK rejected"))?,
    );
    let mut record = Vec::with_capacity(plaintext.len() + 1 + 16);
    record.extend_from_slice(plaintext);
    record.push(0x02);
    key.seal_in_place_append_tag(
        aead::Nonce::assume_unique_for_key(nonce),
        aead::Aad::empty(),
        &mut record,
    )
    .map_err(|_| WebPushError("encryption failed"))?;

    // RFC 8188 §2.1 header: salt, record size, keyid = our ephemeral point.
    let mut body = Vec::with_capacity(16 + 4 + 1 + as_public.len() + record.len());
    body.extend_from_slice(salt);
    body.extend_from_slice(&RECORD_SIZE.to_be_bytes());
    body.push(u8::try_from(as_public.len()).map_err(|_| WebPushError("keyid too long"))?);
    body.extend_from_slice(as_public);
    body.extend_from_slice(&record);
    Ok(body)
}

/// One HKDF-SHA256 extract-and-expand (RFC 5869) into `out`.
fn hkdf_derive(salt: &[u8], ikm: &[u8], info: &[u8], out: &mut [u8]) -> Result<(), WebPushError> {
    struct Len(usize);
    impl hkdf::KeyType for Len {
        fn len(&self) -> usize {
            self.0
        }
    }
    hkdf::Salt::new(hkdf::HKDF_SHA256, salt)
        .extract(ikm)
        .expand(&[info], Len(out.len()))
        .map_err(|_| WebPushError("HKDF expand failed"))?
        .fill(out)
        .map_err(|_| WebPushError("HKDF fill failed"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn b64(s: &str) -> Vec<u8> {
        URL_SAFE_NO_PAD.decode(s).unwrap()
    }

    /// RFC 8291 Appendix A, byte for byte: fixed keys, fixed salt, the
    /// published ECDH secret in, the published push message out.
    #[test]
    fn rfc8291_appendix_a_vector() {
        let ua_public = b64(
            "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4",
        );
        let as_public = b64(
            "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8",
        );
        let ecdh_secret = b64("kyrL1jIIOHEzg3sM2ZWRHDRB62YACZhhSlknJ672kSs");
        let auth = b64("BTBZMqHH6r4Tts7J_aSIgg");
        let salt: [u8; 16] = b64("DGv6ra1nlYgDCS1FRnbzlw").try_into().unwrap();
        let plaintext = b"When I grow up, I want to be a watermelon";

        let body = encrypt_content(
            &ecdh_secret,
            &ua_public,
            &as_public,
            &auth,
            &salt,
            plaintext,
        )
        .unwrap();
        let expected = b64(
            "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlml\
             MoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPTpK4M\
             qgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN",
        );
        assert_eq!(body, expected);
    }

    /// The full path round-trips: a browser-side keypair decrypts what
    /// [`encrypt`] produced, and reads exactly the plaintext.
    #[test]
    fn encrypt_round_trips_against_a_client_keypair() {
        let rng = rand::SystemRandom::new();
        let ua_private =
            agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng).unwrap();
        let ua_public = ua_private.compute_public_key().unwrap().as_ref().to_vec();
        let auth: [u8; 16] = *b"0123456789abcdef";

        let plaintext = br#"{"@type":"StateChange"}"#;
        let body = encrypt(
            &URL_SAFE_NO_PAD.encode(&ua_public),
            &URL_SAFE_NO_PAD.encode(auth),
            plaintext,
        )
        .unwrap();

        // Parse the aes128gcm header.
        let salt: [u8; 16] = body[..16].try_into().unwrap();
        assert_eq!(u32::from_be_bytes(body[16..20].try_into().unwrap()), 4096);
        let keyid_len = usize::from(body[20]);
        assert_eq!(keyid_len, 65);
        let as_public = body[21..21 + keyid_len].to_vec();
        let ciphertext = &body[21 + keyid_len..];

        // Browser side: ECDH with our ephemeral public key, same KDF chain.
        let peer = agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, as_public.clone());
        let decrypted = agreement::agree_ephemeral(ua_private, &peer, |ecdh_secret| {
            let mut key_info = Vec::new();
            key_info.extend_from_slice(b"WebPush: info\x00");
            key_info.extend_from_slice(&ua_public);
            key_info.extend_from_slice(&as_public);
            let mut ikm = [0u8; 32];
            hkdf_derive(&auth, ecdh_secret, &key_info, &mut ikm).unwrap();
            let mut cek = [0u8; 16];
            hkdf_derive(&salt, &ikm, b"Content-Encoding: aes128gcm\x00", &mut cek).unwrap();
            let mut nonce = [0u8; 12];
            hkdf_derive(&salt, &ikm, b"Content-Encoding: nonce\x00", &mut nonce).unwrap();
            let key =
                aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_128_GCM, &cek).unwrap());
            let mut buf = ciphertext.to_vec();
            key.open_in_place(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::empty(),
                &mut buf,
            )
            .unwrap()
            .to_vec()
        })
        .unwrap();

        // The record is plaintext + the 0x02 delimiter.
        assert_eq!(&decrypted[..plaintext.len()], plaintext);
        assert_eq!(decrypted[plaintext.len()], 0x02);
    }

    /// Malformed subscription keys are a clean error, never a panic.
    #[test]
    fn bad_subscription_material_is_refused() {
        assert!(encrypt("not base64!!", "BTBZMqHH6r4Tts7J_aSIgg", b"x").is_err());
        // A compressed / truncated point.
        let short = URL_SAFE_NO_PAD.encode([0x02u8; 33]);
        assert!(encrypt(&short, "BTBZMqHH6r4Tts7J_aSIgg", b"x").is_err());
        // An auth secret of the wrong size.
        let rng = rand::SystemRandom::new();
        let ua = agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng).unwrap();
        let ua_pub = URL_SAFE_NO_PAD.encode(ua.compute_public_key().unwrap().as_ref());
        assert!(encrypt(&ua_pub, &URL_SAFE_NO_PAD.encode([0u8; 8]), b"x").is_err());
    }

    /// A generated key signs a token the matching public key verifies, with
    /// the claims RFC 8292 wants.
    #[test]
    fn vapid_token_is_verifiable_and_carries_the_claims() {
        let key = VapidKeys::generate_key_b64().unwrap();
        let keys = VapidKeys::new(&key, "mailto:owner@example.test").unwrap();
        let header_value = keys.authorization("https://push.example").unwrap();

        let token = header_value
            .strip_prefix("vapid t=")
            .unwrap()
            .split(", k=")
            .next()
            .unwrap();
        let k = header_value.split(", k=").nth(1).unwrap();
        assert_eq!(k, keys.public_key_b64());

        let mut parts = token.split('.');
        let (h, c, s) = (
            parts.next().unwrap(),
            parts.next().unwrap(),
            parts.next().unwrap(),
        );
        let claims: serde_json::Value = serde_json::from_slice(&b64(c)).unwrap();
        assert_eq!(claims["aud"], "https://push.example");
        assert_eq!(claims["sub"], "mailto:owner@example.test");
        let exp = claims["exp"].as_i64().unwrap();
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        assert!(exp > now && exp <= now + 24 * 60 * 60, "exp within RFC cap");

        let verifier = signature::UnparsedPublicKey::new(
            &signature::ECDSA_P256_SHA256_FIXED,
            b64(keys.public_key_b64()),
        );
        verifier
            .verify(format!("{h}.{c}").as_bytes(), &b64(s))
            .expect("ES256 signature verifies against the advertised key");
    }

    /// Config validation: a bad key or subject is refused at startup, not
    /// at first send.
    #[test]
    fn bad_config_is_refused() {
        let key = VapidKeys::generate_key_b64().unwrap();
        assert!(VapidKeys::new(&key, "owner@example.test").is_err());
        assert!(VapidKeys::new("bm90IGEga2V5", "mailto:x@y.test").is_err());
    }
}
