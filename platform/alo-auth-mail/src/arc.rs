//! ARC — RFC 8617 Authenticated Received Chain: sealing and chain
//! validation.
//!
//! When we forward a message (Sieve `redirect`), SPF breaks at the next
//! hop and DKIM may too, so downstream DMARC fails. ARC lets us attest
//! the authentication results we computed at ingress: one **ARC set**
//! per hop — `ARC-Authentication-Results` (AAR, our RFC 8601 verdicts),
//! `ARC-Message-Signature` (AMS, a DKIM-style signature over the
//! message), and `ARC-Seal` (AS, a signature over the ARC header chain
//! itself).
//!
//! [`seal`] produces the **first-hop** set (`i=1; cv=none`) — sealing
//! onto a message that already carries an ARC chain requires validating
//! that chain first (RFC 8617 §5.1 step 1), so such messages are
//! refused ([`SealError::ExistingChain`]) and forwarded unsealed.
//! [`verify`] validates a full chain (§5.2); it backs the sealing
//! round-trip tests today and inbound `arc=` evaluation later.
//!
//! Signing reuses the DKIM machinery ([`crate::dkim`]): canonicalization
//! is RFC 6376 relaxed, keys are ordinary DKIM keys published at
//! `selector._domainkey.domain` (§4.1.2), and malformed input yields a
//! *fail verdict*, never a panic — these bytes come from the open
//! internet.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use sha2::{Digest, Sha256};

use crate::dkim::canon::{self, Canon};
use crate::dkim::keystore::{KeyAlgorithm, KeyStore, SigningKey};
use crate::dkim::{self, Message, SignError};
use crate::resolver::Resolver;

/// The highest ARC instance number a chain may carry (RFC 8617 §4.2.1).
pub const MAX_INSTANCE: u32 = 50;

const AAR_NAME: &str = "ARC-Authentication-Results";
const AMS_NAME: &str = "ARC-Message-Signature";
const SEAL_NAME: &str = "ARC-Seal";

/// Parameters for sealing a message as the first ARC hop.
pub struct SealParams {
    /// Signing domain (`d=`) — the forwarding ADMD's domain.
    pub domain: String,
    /// Selector (`s=`).
    pub selector: String,
    /// The `Authentication-Results` **value** we stamped at ingress
    /// (starting with our authserv-id); it becomes the AAR payload.
    pub authres: String,
    /// Header names the AMS signs (`h=`), in order. Must never include
    /// ARC header fields (§4.1.2).
    pub signed_headers: Vec<String>,
}

impl SealParams {
    /// Defaults: sign the same header set as our DKIM signer.
    pub fn new(
        domain: impl Into<String>,
        selector: impl Into<String>,
        authres: impl Into<String>,
    ) -> Self {
        Self {
            domain: domain.into(),
            selector: selector.into(),
            authres: authres.into(),
            signed_headers: ["From", "To", "Subject", "Date", "Message-ID"]
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
        }
    }
}

/// Why sealing was refused or failed.
#[derive(Debug, thiserror::Error)]
pub enum SealError {
    /// The message already carries ARC headers; only the first hop
    /// (`i=1`) is implemented, and sealing over an unvalidated chain
    /// would corrupt it (§5.1). The caller forwards unsealed.
    #[error("message already carries an ARC chain; refusing to seal i=1 over it")]
    ExistingChain,
    /// Key lookup or the signing operation failed.
    #[error(transparent)]
    Sign(#[from] SignError),
}

/// Seals `message` as the first ARC hop (`i=1; cv=none`), returning the
/// three header lines — `ARC-Seal`, `ARC-Message-Signature`,
/// `ARC-Authentication-Results`, each CRLF-terminated — ready to
/// prepend to the message, in that order.
///
/// # Errors
/// [`SealError::ExistingChain`] when the message already carries any
/// ARC header; [`SealError::Sign`] when the key is unavailable or
/// signing fails. Never panics on message content.
pub async fn seal<K: KeyStore + ?Sized>(
    keys: &K,
    message: &Message<'_>,
    params: &SealParams,
) -> Result<String, SealError> {
    if message.headers.iter().any(|(name, _)| is_arc_header(name)) {
        return Err(SealError::ExistingChain);
    }
    let key = keys
        .get(&params.domain, &params.selector)
        .await
        .map_err(SignError::Key)?;
    let a = key.algorithm.tag();
    let t = jiff::Timestamp::now().as_second();

    // AAR (§4.1.1): `i=1;` then the Authentication-Results value as
    // stamped (folds included — relaxed canonicalization unfolds them
    // identically on both ends).
    let aar_value = format!(" i=1; {}", params.authres.trim_start());

    // AMS (§4.1.2): a DKIM-style signature — `i=` instead of `v=`,
    // relaxed/relaxed, never covering ARC header fields.
    let body_canon = canon::body(Canon::Relaxed, message.body);
    let bh = BASE64.encode(Sha256::digest(&body_canon));
    let h = params.signed_headers.join(":");
    let ams_no_b = format!(
        " i=1; a={a}; c=relaxed/relaxed; d={}; s={}; t={t}; h={h}; bh={bh}; b=",
        params.domain, params.selector
    );
    // Header-hash input: signed headers bottom-up with consumption
    // (RFC 6376 §5.4.2), then the AMS itself (b= empty, no CRLF).
    let mut input = Vec::new();
    let mut consumed = vec![false; message.headers.len()];
    for signed_name in &params.signed_headers {
        for i in (0..message.headers.len()).rev() {
            if consumed[i] {
                continue;
            }
            if message.headers[i].0.eq_ignore_ascii_case(signed_name) {
                consumed[i] = true;
                let (name, value) = message.headers[i];
                input.extend_from_slice(&canon::header(Canon::Relaxed, name, value));
                break;
            }
        }
    }
    append_self_no_crlf(&mut input, AMS_NAME, &ams_no_b);
    let b = BASE64.encode(sign_with(&key, &input)?);
    let ams_value = format!("{ams_no_b}{b}");

    // AS (§4.1.3, §5.1.1): relaxed-canonicalized ARC set headers in
    // instance order — AAR, AMS, then this seal with `b=` empty and no
    // trailing CRLF. First hop, so `cv=none`.
    let seal_no_b = format!(
        " i=1; a={a}; t={t}; cv=none; d={}; s={}; b=",
        params.domain, params.selector
    );
    let mut seal_input = Vec::new();
    seal_input.extend_from_slice(&canon::header(Canon::Relaxed, AAR_NAME, &aar_value));
    seal_input.extend_from_slice(&canon::header(Canon::Relaxed, AMS_NAME, &ams_value));
    append_self_no_crlf(&mut seal_input, SEAL_NAME, &seal_no_b);
    let b = BASE64.encode(sign_with(&key, &seal_input)?);
    let seal_value = format!("{seal_no_b}{b}");

    Ok(format!(
        "{SEAL_NAME}:{seal_value}\r\n{AMS_NAME}:{ams_value}\r\n{AAR_NAME}:{aar_value}\r\n"
    ))
}

/// Appends the signature's own header, relaxed-canonicalized, without
/// the trailing CRLF (the self-exclusion form both AMS and AS hash).
fn append_self_no_crlf(input: &mut Vec<u8>, name: &str, value: &str) {
    let mut own = canon::header(Canon::Relaxed, name, value);
    if own.ends_with(b"\r\n") {
        own.truncate(own.len() - 2);
    }
    input.extend_from_slice(&own);
}

/// Signs `input` per the key's algorithm: RSA over the data (ring
/// hashes internally), Ed25519 over its SHA-256 (RFC 8463).
fn sign_with(key: &SigningKey, input: &[u8]) -> Result<Vec<u8>, SignError> {
    match key.algorithm {
        KeyAlgorithm::RsaSha256 => dkim::sign_rsa(key, input),
        KeyAlgorithm::Ed25519Sha256 => dkim::sign_ed25519(key, &Sha256::digest(input)),
    }
}

fn is_arc_header(name: &str) -> bool {
    name.eq_ignore_ascii_case(AAR_NAME)
        || name.eq_ignore_ascii_case(AMS_NAME)
        || name.eq_ignore_ascii_case(SEAL_NAME)
}

/// The outcome of validating a message's ARC chain (RFC 8617 §2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainValidation {
    /// No ARC sets are present.
    None,
    /// Every seal and the newest AMS verified.
    Pass,
    /// The chain is structurally invalid or a signature failed. DNS
    /// outages also land here — the 3-state `cv` model has no
    /// temperror, and a chain we cannot validate must not be trusted.
    Fail,
}

impl ChainValidation {
    /// Token for `Authentication-Results` / the `cv=` tag.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Pass => "pass",
            Self::Fail => "fail",
        }
    }
}

/// One instance's three headers: `(header_index, raw_value)` each.
#[derive(Default)]
struct ArcSet<'a> {
    aar: Option<(usize, &'a str)>,
    ams: Option<(usize, &'a str)>,
    seal: Option<(usize, &'a str)>,
}

/// Validates the ARC chain on `message` (RFC 8617 §5.2): structural
/// checks (contiguous instances, one of each header per set, `cv`
/// semantics), every `ARC-Seal` signature, and the newest
/// `ARC-Message-Signature`. A message with no ARC headers returns
/// [`ChainValidation::None`].
pub async fn verify<R: Resolver + ?Sized>(resolver: &R, message: &Message<'_>) -> ChainValidation {
    // Collect the sets by instance number.
    let mut sets: std::collections::BTreeMap<u32, ArcSet<'_>> = std::collections::BTreeMap::new();
    let mut any = false;
    for (idx, (name, value)) in message.headers.iter().enumerate() {
        if !is_arc_header(name) {
            continue;
        }
        any = true;
        let tags = dkim::parse_tag_list(value);
        let instance = tags
            .iter()
            .find(|(t, _)| t == "i")
            .and_then(|(_, v)| v.parse::<u32>().ok());
        let Some(i) = instance else {
            return ChainValidation::Fail;
        };
        if i == 0 || i > MAX_INSTANCE {
            return ChainValidation::Fail;
        }
        let set = sets.entry(i).or_default();
        let slot = if name.eq_ignore_ascii_case(AAR_NAME) {
            &mut set.aar
        } else if name.eq_ignore_ascii_case(AMS_NAME) {
            &mut set.ams
        } else {
            &mut set.seal
        };
        if slot.is_some() {
            return ChainValidation::Fail; // duplicate within an instance (§4.2.1)
        }
        *slot = Some((idx, value));
    }
    if !any {
        return ChainValidation::None;
    }

    // Structure: instances 1..=n, each complete.
    let n = match sets.keys().next_back() {
        Some(&n) => n,
        None => return ChainValidation::None,
    };
    if sets.len() != n as usize {
        return ChainValidation::Fail; // a gap in the instance sequence
    }
    for set in sets.values() {
        if set.aar.is_none() || set.ams.is_none() || set.seal.is_none() {
            return ChainValidation::Fail;
        }
    }

    // cv semantics (§5.1.2): the oldest seal says `none`, every later
    // one says `pass` (a recorded `fail` dead-ends the chain).
    for (&i, set) in &sets {
        let Some((_, seal_value)) = set.seal else {
            return ChainValidation::Fail;
        };
        let tags = dkim::parse_tag_list(seal_value);
        let cv = tags
            .iter()
            .find(|(t, _)| t == "cv")
            .map(|(_, v)| v.as_str());
        let expected = if i == 1 { "none" } else { "pass" };
        if cv != Some(expected) {
            return ChainValidation::Fail;
        }
    }

    // Every seal must verify (§5.2 step 4)...
    for i in 1..=n {
        if !verify_seal(resolver, &sets, i).await {
            return ChainValidation::Fail;
        }
    }
    // ...and the most recent AMS (§5.2 step 3 — earlier AMS values are
    // expected to break as later hops modify the message).
    let Some((ams_idx, ams_value)) = sets.get(&n).and_then(|s| s.ams) else {
        return ChainValidation::Fail;
    };
    if !verify_ams(resolver, message, ams_idx, ams_value).await {
        return ChainValidation::Fail;
    }
    ChainValidation::Pass
}

/// Verifies the `ARC-Seal` of instance `i` over the chain prefix
/// 1..=i (§5.1.1): relaxed-canonicalized AAR, AMS, AS per instance in
/// ascending order, with instance `i`'s own seal `b=`-stripped and
/// unterminated.
async fn verify_seal<R: Resolver + ?Sized>(
    resolver: &R,
    sets: &std::collections::BTreeMap<u32, ArcSet<'_>>,
    i: u32,
) -> bool {
    let Some(set) = sets.get(&i) else {
        return false;
    };
    let Some((_, seal_value)) = set.seal else {
        return false;
    };
    let tags = dkim::parse_tag_list(seal_value);
    let get = |k: &str| tags.iter().find(|(t, _)| t == k).map(|(_, v)| v.as_str());
    let Some(algorithm) = get("a").and_then(parse_algorithm) else {
        return false;
    };
    let (Some(domain), Some(selector)) = (get("d"), get("s")) else {
        return false;
    };
    let Some(signature) = get("b").and_then(dkim::decode_b64_ws) else {
        return false;
    };

    let mut input = Vec::new();
    for j in 1..=i {
        let Some(prior) = sets.get(&j) else {
            return false;
        };
        let (Some((_, aar)), Some((_, ams)), Some((_, seal))) = (prior.aar, prior.ams, prior.seal)
        else {
            return false;
        };
        input.extend_from_slice(&canon::header(Canon::Relaxed, AAR_NAME, aar));
        input.extend_from_slice(&canon::header(Canon::Relaxed, AMS_NAME, ams));
        if j < i {
            input.extend_from_slice(&canon::header(Canon::Relaxed, SEAL_NAME, seal));
        } else {
            append_self_no_crlf(&mut input, SEAL_NAME, &dkim::strip_b_tag(seal));
        }
    }
    verify_signature(resolver, algorithm, domain, selector, &input, &signature).await
}

/// Verifies the newest `ARC-Message-Signature` like a DKIM signature
/// (§4.1.2): body hash (`bh=`), then the signed-header hash with the
/// AMS itself `b=`-stripped. `c=` defaults to relaxed/relaxed.
async fn verify_ams<R: Resolver + ?Sized>(
    resolver: &R,
    message: &Message<'_>,
    ams_index: usize,
    ams_value: &str,
) -> bool {
    let tags = dkim::parse_tag_list(ams_value);
    let get = |k: &str| tags.iter().find(|(t, _)| t == k).map(|(_, v)| v.as_str());
    let Some(algorithm) = get("a").and_then(parse_algorithm) else {
        return false;
    };
    let (Some(domain), Some(selector)) = (get("d"), get("s")) else {
        return false;
    };
    let Some((header_canon, body_canon)) = dkim::parse_canon(get("c").unwrap_or("relaxed/relaxed"))
    else {
        return false;
    };
    let signed_headers: Vec<&str> = match get("h") {
        Some(h) => h
            .split(':')
            .map(str::trim)
            .filter(|h| !h.is_empty())
            .collect(),
        None => return false,
    };
    // §4.1.2: the AMS must not cover ARC header fields; a signature
    // that claims to is malformed.
    if signed_headers.iter().any(|h| is_arc_header(h)) {
        return false;
    }
    let (Some(body_hash), Some(signature)) = (
        get("bh").and_then(dkim::decode_b64_ws),
        get("b").and_then(dkim::decode_b64_ws),
    ) else {
        return false;
    };

    // Body hash, honoring an optional l= (canonicalize, then truncate).
    let mut body = canon::body(body_canon, message.body);
    if let Some(l) = get("l").and_then(|l| l.parse::<usize>().ok())
        && l <= body.len()
    {
        body.truncate(l);
    }
    if Sha256::digest(&body).as_slice() != body_hash.as_slice() {
        return false;
    }

    // Header hash: h= names bottom-up with consumption, excluding the
    // AMS being verified, then the AMS itself (b= empty, no CRLF).
    let mut input = Vec::new();
    let mut consumed = vec![false; message.headers.len()];
    for signed_name in &signed_headers {
        for i in (0..message.headers.len()).rev() {
            if i == ams_index || consumed[i] {
                continue;
            }
            if message.headers[i].0.eq_ignore_ascii_case(signed_name) {
                consumed[i] = true;
                let (name, value) = message.headers[i];
                input.extend_from_slice(&canon::header(header_canon, name, value));
                break;
            }
        }
    }
    append_self_no_crlf(&mut input, AMS_NAME, &dkim::strip_b_tag(ams_value));
    verify_signature(resolver, algorithm, domain, selector, &input, &signature).await
}

/// Fetches the public key and verifies `signature` over `input`.
async fn verify_signature<R: Resolver + ?Sized>(
    resolver: &R,
    algorithm: KeyAlgorithm,
    domain: &str,
    selector: &str,
    input: &[u8],
    signature: &[u8],
) -> bool {
    let key = match dkim::fetch_key(resolver, selector, domain).await {
        Ok(Some(key)) => key,
        _ => return false,
    };
    if key.algorithm != algorithm {
        return false;
    }
    match algorithm {
        KeyAlgorithm::RsaSha256 => dkim::verify_rsa(&key.public_key, input, signature),
        KeyAlgorithm::Ed25519Sha256 => {
            dkim::verify_ed25519(&key.public_key, &Sha256::digest(input), signature)
        }
    }
}

fn parse_algorithm(a: &str) -> Option<KeyAlgorithm> {
    match a {
        "rsa-sha256" => Some(KeyAlgorithm::RsaSha256),
        "ed25519-sha256" => Some(KeyAlgorithm::Ed25519Sha256),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::dkim::keystore::FileKeyStore;
    use crate::resolver::fixture::FixtureResolver;

    const RAW_MSG: &[u8] = b"Authentication-Results: mx.alo.test;\r\n\tspf=pass smtp.mailfrom=alice@origin.test;\r\n\tdkim=pass header.d=origin.test;\r\n\tdmarc=pass header.from=origin.test\r\nFrom: alice@origin.test\r\nTo: bob@alo.test\r\nSubject: forward me\r\nDate: Mon, 27 Jul 2026 00:00:00 +0000\r\nMessage-ID: <fwd-1@origin.test>\r\n\r\nplease forward this\r\n";

    const AUTHRES: &str = "mx.alo.test;\r\n\tspf=pass smtp.mailfrom=alice@origin.test;\r\n\tdkim=pass header.d=origin.test;\r\n\tdmarc=pass header.from=origin.test";

    /// An Ed25519 test key on disk plus its DNS TXT record.
    fn ed25519_fixture(dir: &std::path::Path, seed: u8) -> (FileKeyStore, FixtureResolver) {
        use base64::Engine;
        use ed25519_dalek::SigningKey as EdSigningKey;
        use ed25519_dalek::pkcs8::EncodePrivateKey;
        let signing = EdSigningKey::from_bytes(&[seed; 32]);
        let pkcs8 = signing.to_pkcs8_der().unwrap();
        let b64 = BASE64.encode(pkcs8.as_bytes());
        let pem = format!("-----BEGIN PRIVATE KEY-----\n{b64}\n-----END PRIVATE KEY-----\n");
        let path = dir.join("arc.pem");
        std::fs::write(&path, pem).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let keys =
            FileKeyStore::new().with_key("alo.test", "arcsel", path, KeyAlgorithm::Ed25519Sha256);
        let public_b64 = BASE64.encode(signing.verifying_key().to_bytes());
        let dns = FixtureResolver::default().with_txt(
            "arcsel._domainkey.alo.test",
            &[&format!("v=DKIM1; k=ed25519; p={public_b64}")],
        );
        (keys, dns)
    }

    async fn seal_fixture(dir: &std::path::Path) -> (Vec<u8>, FixtureResolver) {
        let (keys, dns) = ed25519_fixture(dir, 11);
        let msg = Message::parse(RAW_MSG);
        let params = SealParams::new("alo.test", "arcsel", AUTHRES);
        let set = seal(&keys, &msg, &params).await.expect("seal");
        let mut sealed = set.into_bytes();
        sealed.extend_from_slice(RAW_MSG);
        (sealed, dns)
    }

    #[tokio::test]
    async fn seal_then_verify_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let (sealed, dns) = seal_fixture(dir.path()).await;
        let text = String::from_utf8(sealed.clone()).unwrap();
        assert!(text.starts_with("ARC-Seal: i=1; a=ed25519-sha256;"));
        assert!(text.contains("ARC-Message-Signature: i=1;"));
        assert!(text.contains("ARC-Authentication-Results: i=1; mx.alo.test;"));
        assert!(text.contains("cv=none"));
        let verdict = verify(&dns, &Message::parse(&sealed)).await;
        assert_eq!(verdict, ChainValidation::Pass);
    }

    #[tokio::test]
    async fn tampered_body_fails() {
        let dir = tempfile::tempdir().unwrap();
        let (mut sealed, dns) = seal_fixture(dir.path()).await;
        let idx = sealed.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        sealed[idx] = b'X';
        assert_eq!(
            verify(&dns, &Message::parse(&sealed)).await,
            ChainValidation::Fail
        );
    }

    #[tokio::test]
    async fn tampered_aar_breaks_the_seal() {
        // The AS covers the AAR — altering the attested verdicts (the
        // whole point of ARC) must fail the chain.
        let dir = tempfile::tempdir().unwrap();
        let (sealed, dns) = seal_fixture(dir.path()).await;
        let text = String::from_utf8(sealed).unwrap();
        let tampered = text.replace("dmarc=pass", "dmarc=fail");
        assert_ne!(text, tampered, "the replace must have bitten");
        assert_eq!(
            verify(&dns, &Message::parse(tampered.as_bytes())).await,
            ChainValidation::Fail
        );
    }

    #[tokio::test]
    async fn existing_chain_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let (sealed, _) = seal_fixture(dir.path()).await;
        let (keys, _) = ed25519_fixture(dir.path(), 12);
        let params = SealParams::new("alo.test", "arcsel", AUTHRES);
        let again = seal(&keys, &Message::parse(&sealed), &params).await;
        assert!(matches!(again, Err(SealError::ExistingChain)));
    }

    // Cross-implementation known-answer vector: a message ARC-signed by
    // **dkimpy** (an independent ARC implementation) with the repo's
    // RSA-2048 test key (`kat._domainkey.alo.test`), base64 of the raw
    // signed bytes. Our validator must return Pass — this pins the
    // §5.1.1 seal-scope and §4.1.2 AMS construction to a foreign
    // signer rather than only round-tripping our own seal. dkimpy's
    // AMS folds `h=` with spaces and lists `from` twice (over-signing),
    // exercising tolerant tag parsing and header consumption.
    const KAT_SEALED_B64: &str = "QVJDLVNlYWw6IGk9MTsgY3Y9bm9uZTsgYT1yc2Etc2hhMjU2OyBkPWFsby50ZXN0OyBzPWthdDsgdD0xNzg1MTM4OTA0Ow0KIGI9Y2tMSkFYazQ3NThZdVNoV1Fvay9TeU1nUXNHT1JxM2d5QVY2UGswVG1ndjg5YnZQY2lVaHp6N2IwL3g4MnRWNWZTWCswDQogZlFLRkxZNnJaNVBuQ0V2MFY0bGVHdnhXdXJyOUU0cEY0bjBhNlhORU5LU3Z6elYzUVg1R3c3Z0ZCVC8veVNHa2RBTXBkalUNCiBjYWZ4UlMrcnhFZTliWnNTd2ZaR3grWXZSV01PK1liNjZoZmpGRzhaSmladmRYbUhkYitQUm5hamlITG1UY0FEQW0wR3NTWA0KIHNVMm52RVVQd0NHWURzR290QWRVRm9qY1hvcWdQZXJWQ05JdFYrbU5ldm9LWnp0TS9UZm1zSENZQ0JjNlRXNk5HdGt0VkdHDQogWm9vTHYyU1ZZemdOL2syVTY1NFl4YzhraitzSG8wMmFydUhPMFVJZHA5aVkxOVFwUkcvU1hxb0hGS2dnPT0NCkFSQy1NZXNzYWdlLVNpZ25hdHVyZTogaT0xOyBhPXJzYS1zaGEyNTY7IGM9cmVsYXhlZC9yZWxheGVkOyBkPWFsby50ZXN0Ow0KIHM9a2F0OyB0PTE3ODUxMzg5MDQ7IGg9ZnJvbSA6IHRvIDogc3ViamVjdCA6IGRhdGUgOiBtZXNzYWdlLWlkIDogZnJvbTsNCiBiaD1yeWZ4TU9SSkUvNHIwUkllNEhTejJVZzJzZlJmYlFJZ2VMR1hpc09NM253PTsNCiBiPW1WWHhrUERGaGl3clVNVjVqZUQ1aXphRDB4UU0zVWpBcmtITFZPaEJZc243WE50UG43R0ZobllvQWg4VXNPV0FZa1F6ZA0KIFYwTEVVck1XY040M1o5SkVvZmNjcjcxNkE1MU0xbGtzaU10SXhDaWpWQ1M4bmtuMWJKcGRicTEwYnhQazkyY1daOTlWZjNZDQogQmNvSVFGSGM4WXlPUXRsVndEcTBIVFhaWVdHYWxKVUd6NFlpZkwwdXp3YWdEbmx3UzRIcHRnNktTd1Yrc3haWVJYamt3K2gNCiBTbUttcWVPTC9VU09lZzNYdXk3a3ArRklrSURiZW4ybENqWEVxU3k3KzBQaFFYc09Ddkc0aFBtTHV0TURLUFlUMDNieWN6RQ0KIHN6MVUzc1dCeUJucml0ajR1eGNWRTc5NVlpSHU3Sms2dmdpRTFxc0daeHEyWTk5b1ZWYVFIa0x4WHFkZz09DQpBUkMtQXV0aGVudGljYXRpb24tUmVzdWx0czogaT0xOyBteC5hbG8udGVzdDsNCiBzcGY9cGFzcyBzbXRwLm1haWxmcm9tPWFsaWNlQG9yaWdpbi50ZXN0Ow0KIGRraW09cGFzcyBoZWFkZXIuZD1vcmlnaW4udGVzdDsNCiBkbWFyYz1wYXNzIGhlYWRlci5mcm9tPW9yaWdpbi50ZXN0DQpBdXRoZW50aWNhdGlvbi1SZXN1bHRzOiBteC5hbG8udGVzdDsgc3BmPXBhc3Mgc210cC5tYWlsZnJvbT1hbGljZUBvcmlnaW4udGVzdDsgZGtpbT1wYXNzIGhlYWRlci5kPW9yaWdpbi50ZXN0OyBkbWFyYz1wYXNzIGhlYWRlci5mcm9tPW9yaWdpbi50ZXN0DQpGcm9tOiBhbGljZUBvcmlnaW4udGVzdA0KVG86IGJvYkBhbG8udGVzdA0KU3ViamVjdDogQVJDIEtBVA0KRGF0ZTogTW9uLCAyNyBKdWwgMjAyNiAwMDowMDowMCArMDAwMA0KTWVzc2FnZS1JRDogPGFyYy1rYXQtMUBvcmlnaW4udGVzdD4NCg0KYXJjIGtub3duIGFuc3dlciB0ZXN0IGJvZHkNCg==";
    const KAT_SPKI_B64: &str = "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEApDyfpTZ1bArZV6IRum1dSKmdyzJkLxt6d6ws5n9wX4MiwXmStWO762hjiMDslKqmtotodLNOUd0+JIXZKkTz0waccxF5x3qjDC38MWfaSc5wDEXc0z+YdEm55E6kk6xHvc0ZPTyIsC36BR9hbx9HT9WDnKCaHaO5yvCXYqfik3qjG7sgz5ZDsBYl8zQYbIowfnwa4qACYyYGd1CVoIGFaBDKo7wETu54m2CTLn4dqvB029f5YRCk9++3Y2nMNOwgbiSnqfHI/nevOYEvtMjuGhTgjYALmAyGnhIzbN/gSMLZXlOu0x/TvFgkEMBAyW5rz0Lx+nrHAbLR6R9UadkaowIDAQAB";

    #[tokio::test]
    async fn verifies_dkimpy_sealed_message_known_answer() {
        let sealed = BASE64.decode(KAT_SEALED_B64).unwrap();
        let dns = FixtureResolver::default().with_txt(
            "kat._domainkey.alo.test",
            &[&format!("v=DKIM1; k=rsa; p={KAT_SPKI_B64}")],
        );
        assert_eq!(
            verify(&dns, &Message::parse(&sealed)).await,
            ChainValidation::Pass
        );
    }

    #[tokio::test]
    async fn message_without_arc_is_none() {
        let dns = FixtureResolver::default();
        assert_eq!(
            verify(&dns, &Message::parse(RAW_MSG)).await,
            ChainValidation::None
        );
    }

    #[tokio::test]
    async fn wrong_cv_fails() {
        // A first-hop seal must say cv=none; flip it to pass.
        let dir = tempfile::tempdir().unwrap();
        let (sealed, dns) = seal_fixture(dir.path()).await;
        let text = String::from_utf8(sealed)
            .unwrap()
            .replace("cv=none", "cv=pass");
        assert_eq!(
            verify(&dns, &Message::parse(text.as_bytes())).await,
            ChainValidation::Fail
        );
    }

    #[tokio::test]
    async fn incomplete_set_fails() {
        // Drop the AAR line: instance 1 is then incomplete (§4.2.1).
        let dir = tempfile::tempdir().unwrap();
        let (sealed, dns) = seal_fixture(dir.path()).await;
        let text = String::from_utf8(sealed).unwrap();
        // Remove the AAR field (its start line plus WSP-led folds).
        let start = text.find("ARC-Authentication-Results:").unwrap();
        let mut end = 0;
        for line in text[start..].split_inclusive("\r\n") {
            if end > 0 && !line.starts_with([' ', '\t']) {
                break;
            }
            end += line.len();
        }
        let cut = format!("{}{}", &text[..start], &text[start + end..]);
        assert_eq!(
            verify(&dns, &Message::parse(cut.as_bytes())).await,
            ChainValidation::Fail
        );
    }
}
