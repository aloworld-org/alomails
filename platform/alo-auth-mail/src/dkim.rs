//! DKIM — RFC 6376 signing and verification, with Ed25519 per RFC 8463.
//!
//! Verify: parse each `DKIM-Signature`, fetch the public key from
//! `selector._domainkey.domain` TXT, canonicalize (simple/relaxed),
//! check the body hash (`bh=`) and expiry (`x=`), then verify the
//! signature (`b=`) over the signed header set. Sign: build a
//! `DKIM-Signature`, hash, and sign with a key from the [`keystore`].
//!
//! Malformed input (bad tags, bad base64, unparseable keys, hash
//! mismatch) always yields a *fail verdict*, never a panic — these
//! bytes come from the open internet.

pub mod canon;
pub mod keystore;
pub mod rsa_public;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use sha2::{Digest, Sha256};

use crate::resolver::{DnsError, Resolver};
use canon::Canon;
use keystore::{KeyAlgorithm, KeyStore, SigningKey};

/// The outcome of verifying one DKIM signature (RFC 8601 dkim result).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DkimVerdict {
    /// Result token.
    pub result: DkimResult,
    /// The signing domain (`d=`), for Authentication-Results.
    pub domain: String,
    /// The selector (`s=`).
    pub selector: String,
}

/// DKIM result values (RFC 8601 §2.7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DkimResult {
    /// Signature verified.
    Pass,
    /// Signature present but did not verify (bad sig / body hash).
    Fail,
    /// The signature or key was syntactically invalid.
    PermError,
    /// A transient error (DNS) prevented verification.
    TempError,
    /// No key was published (key revoked / not found).
    Neutral,
}

impl DkimResult {
    /// Token for Received/Authentication-Results.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::PermError => "permerror",
            Self::TempError => "temperror",
            Self::Neutral => "neutral",
        }
    }
}

/// A message split into its header list and raw body.
pub struct Message<'a> {
    /// Headers in order: `(name, raw_value)`, value excluding the
    /// trailing CRLF, including any folding whitespace.
    pub headers: Vec<(&'a str, &'a str)>,
    /// Raw body bytes (after the empty separator line).
    pub body: &'a [u8],
}

impl<'a> Message<'a> {
    /// Parses a raw RFC 5322 message into headers + body. Header
    /// parsing is lenient (malformed or non-UTF-8 lines are skipped
    /// individually) — never panics.
    pub fn parse(raw: &'a [u8]) -> Self {
        // Find the header/body separator (CRLF CRLF).
        let sep = find_double_crlf(raw);
        let (header_bytes, body) = match sep {
            Some(idx) => (&raw[..idx], &raw[idx + 4..]),
            None => (raw, &raw[raw.len()..]),
        };
        let headers = split_headers(header_bytes);
        Message { headers, body }
    }
}

/// Splits a header block into `(name, value)` pairs, joining folded
/// continuation lines into the value verbatim (RFC 5322 §2.2.3).
///
/// Operates on raw bytes and validates each header's UTF-8 in
/// isolation: a stray 8-bit octet drops only the header that carries
/// it, not the entire block (which would silently erase the DKIM and
/// DMARC inputs for the whole message).
fn split_headers(block: &[u8]) -> Vec<(&str, &str)> {
    let n = block.len();
    let mut headers = Vec::new();
    let mut i = 0;
    while i < n {
        let end = line_end(block, i);
        // A blank line or a stray continuation (WSP-led line with no
        // header to attach to) starts nothing; skip it.
        if end == i || block[i] == b' ' || block[i] == b'\t' {
            i = next_line(end, n);
            continue;
        }
        let Some(rel_colon) = block[i..end].iter().position(|&b| b == b':') else {
            i = next_line(end, n);
            continue; // not a header line
        };
        let name = &block[i..i + rel_colon];
        let value_start = i + rel_colon + 1;
        // Extend the value over folded continuation lines (WSP-led),
        // keeping the intervening CRLF + WSP verbatim.
        let mut value_end = end;
        let mut scan = next_line(end, n);
        while scan < n && (block[scan] == b' ' || block[scan] == b'\t') {
            let cont_end = line_end(block, scan);
            value_end = cont_end;
            scan = next_line(cont_end, n);
        }
        let value = &block[value_start..value_end];
        // Per-header UTF-8 gate: skip only this header if either the
        // name or the value is not valid UTF-8.
        if let (Ok(name), Ok(value)) = (std::str::from_utf8(name), std::str::from_utf8(value)) {
            headers.push((name, value));
        }
        i = scan;
    }
    headers
}

/// The byte index of the CRLF that ends the line starting at `from`,
/// or `block.len()` when the line runs to the end without one.
fn line_end(block: &[u8], from: usize) -> usize {
    let mut j = from;
    while j + 1 < block.len() {
        if block[j] == b'\r' && block[j + 1] == b'\n' {
            return j;
        }
        j += 1;
    }
    block.len()
}

/// Steps past the CRLF at `end` (if any) to the next line start.
fn next_line(end: usize, n: usize) -> usize {
    if end < n { end + 2 } else { n }
}

fn find_double_crlf(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|w| w == b"\r\n\r\n")
}

/// The maximum number of `DKIM-Signature` headers verified on one
/// message. Each verification is a full-body hash + a DNS key lookup +
/// a public-key verify, so an attacker stuffing thousands of signature
/// headers is a CPU/DNS amplification vector; bound it (a legitimate
/// message never carries more than a handful).
const MAX_SIGNATURES: usize = 10;

/// Verifies each `DKIM-Signature` on `message` (up to [`MAX_SIGNATURES`]),
/// returning one verdict per verified signature (in header order). A
/// message with no signature yields an empty vec (`dkim=none`).
pub async fn verify<R: Resolver + ?Sized>(resolver: &R, message: &Message<'_>) -> Vec<DkimVerdict> {
    let mut verdicts = Vec::new();
    for (i, (name, raw_value)) in message.headers.iter().enumerate() {
        if !name.eq_ignore_ascii_case("DKIM-Signature") {
            continue;
        }
        if verdicts.len() >= MAX_SIGNATURES {
            tracing::warn!(
                "message carries more than {MAX_SIGNATURES} DKIM signatures; ignoring the rest"
            );
            break;
        }
        verdicts.push(verify_one(resolver, message, raw_value, i).await);
    }
    verdicts
}

async fn verify_one<R: Resolver + ?Sized>(
    resolver: &R,
    message: &Message<'_>,
    sig_value: &str,
    sig_index: usize,
) -> DkimVerdict {
    let sig = match Signature::parse(sig_value) {
        Some(sig) => sig,
        None => return verdict(DkimResult::PermError, "", ""),
    };
    let fail = |result: DkimResult| verdict(result, &sig.domain, &sig.selector);

    // §6.1.1: if `h=` does not cover the From header, the Verifier MUST
    // PERMFAIL — an unsigned From lets the visible sender be altered
    // while DKIM still reports pass, which is exactly what signing the
    // From prevents.
    if !sig
        .signed_headers
        .iter()
        .any(|h| h.eq_ignore_ascii_case("from"))
    {
        return fail(DkimResult::PermError);
    }

    // Expiry (§3.5 x=): a signature past its expiry does not verify.
    if let Some(expiry) = sig.expiry
        && jiff::Timestamp::now().as_second() as u64 > expiry
    {
        return fail(DkimResult::Fail);
    }

    // 1. Body hash (bh=). RFC 6376 §3.5/§3.7: `l=` counts *canonicalized*
    //    body octets, so canonicalize the whole body first, then take
    //    the first `l` octets. An `l` larger than the canonicalized body
    //    hashes everything (and simply fails to match if the signer
    //    counted more).
    let mut body_canon = canon::body(sig.body_canon, message.body);
    if let Some(l) = sig.body_length
        && l <= body_canon.len()
    {
        body_canon.truncate(l);
    }
    let computed_bh = Sha256::digest(&body_canon);
    if computed_bh.as_slice() != sig.body_hash.as_slice() {
        return fail(DkimResult::Fail);
    }

    // 2. Fetch the public key from DNS.
    let key_record = match fetch_key(resolver, &sig.selector, &sig.domain).await {
        Ok(Some(record)) => record,
        Ok(None) => return fail(DkimResult::Neutral), // revoked/empty p=
        Err(DnsError::NotFound { .. }) => return fail(DkimResult::PermError),
        Err(DnsError::Temporary { .. }) => return fail(DkimResult::TempError),
    };
    if key_record.algorithm != sig.algorithm {
        return fail(DkimResult::PermError);
    }

    // 3. The signed data (§3.7): the signed headers + the DKIM-Signature
    //    itself (b= emptied, no trailing CRLF).
    let header_input = build_header_hash_input(message, &sig, sig_index);

    // 4. Verify. RSASSA-PKCS1-v1_5 (rsa-sha256) hashes the data itself,
    //    so ring verifies over `header_input`. Ed25519 (RFC 8463) signs
    //    the SHA-256 hash of the data, so verify over that 32-byte hash.
    let ok = match sig.algorithm {
        KeyAlgorithm::RsaSha256 => {
            verify_rsa(&key_record.public_key, &header_input, &sig.signature)
        }
        KeyAlgorithm::Ed25519Sha256 => {
            let header_hash = Sha256::digest(&header_input);
            verify_ed25519(&key_record.public_key, &header_hash, &sig.signature)
        }
    };
    fail(if ok {
        DkimResult::Pass
    } else {
        DkimResult::Fail
    })
}

fn verdict(result: DkimResult, domain: &str, selector: &str) -> DkimVerdict {
    DkimVerdict {
        result,
        domain: domain.to_owned(),
        selector: selector.to_owned(),
    }
}

/// Builds the exact byte string hashed for the signature (§3.7): each
/// signed header canonicalized in `h=` order, then the DKIM-Signature
/// header canonicalized with an empty `b=` and NO trailing CRLF.
fn build_header_hash_input(message: &Message<'_>, sig: &Signature, sig_index: usize) -> Vec<u8> {
    let mut out = Vec::new();
    // For each name in h=, take the last not-yet-consumed instance from
    // the bottom of the header block (§5.4.2). Track consumption.
    let mut consumed = vec![false; message.headers.len()];
    for signed_name in &sig.signed_headers {
        // Search bottom-up for an unconsumed matching header (excluding
        // the DKIM-Signature being verified).
        let mut found = None;
        for i in (0..message.headers.len()).rev() {
            if i == sig_index || consumed[i] {
                continue;
            }
            if message.headers[i].0.eq_ignore_ascii_case(signed_name) {
                found = Some(i);
                break;
            }
        }
        if let Some(i) = found {
            consumed[i] = true;
            let (name, value) = message.headers[i];
            out.extend_from_slice(&canon::header(sig.header_canon, name, value));
        }
        // A signed header that is absent contributes nothing (an empty
        // canonicalization), per §3.7 — but a present-then-removed
        // header would break the hash, which is the intended behavior.
    }
    // Append the DKIM-Signature header with b= emptied and no CRLF.
    let (name, value) = message.headers[sig_index];
    let stripped = strip_b_tag(value);
    let mut canon_sig = canon::header(sig.header_canon, name, &stripped);
    // Remove the trailing CRLF that `canon::header` appended (§3.7).
    if canon_sig.ends_with(b"\r\n") {
        canon_sig.truncate(canon_sig.len() - 2);
    }
    out.extend_from_slice(&canon_sig);
    out
}

/// Removes the value of the `b=` tag from a DKIM-Signature value,
/// leaving `b=` present but empty (§3.7 — the signature covers the
/// header with its own `b=` value removed). Shared with [`crate::arc`]
/// (AMS/AS use the same self-exclusion rule, RFC 8617 §4.1.2/§4.1.3).
///
/// A single linear pass over the `;`-separated tag list: each tag is
/// examined exactly once, so an attacker cannot force super-linear work
/// with a large folding-whitespace run inside a multi-megabyte
/// signature value (the header block can be up to `max_message_size`).
pub(crate) fn strip_b_tag(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut first = true;
    for segment in value.split(';') {
        if !first {
            out.push(';');
        }
        first = false;
        // The tag name is everything before the first `=`, trimmed. Only
        // the bare `b` tag (not `bh`) has its value dropped.
        if let Some(eq) = segment.find('=')
            && segment[..eq].trim() == "b"
        {
            out.push_str(&segment[..=eq]); // keep `…b=`, drop the value
        } else {
            out.push_str(segment);
        }
    }
    out
}

/// A parsed DKIM-Signature (RFC 6376 §3.5).
struct Signature {
    algorithm: KeyAlgorithm,
    domain: String,
    selector: String,
    header_canon: Canon,
    body_canon: Canon,
    signed_headers: Vec<String>,
    body_hash: Vec<u8>,
    signature: Vec<u8>,
    body_length: Option<usize>,
    expiry: Option<u64>,
}

impl Signature {
    fn parse(value: &str) -> Option<Self> {
        let tags = parse_tag_list(value);
        let get = |k: &str| tags.iter().find(|(t, _)| t == k).map(|(_, v)| v.as_str());

        // v= must be 1.
        if get("v") != Some("1") {
            return None;
        }
        let algorithm = match get("a")? {
            "rsa-sha256" => KeyAlgorithm::RsaSha256,
            "ed25519-sha256" => KeyAlgorithm::Ed25519Sha256,
            _ => return None,
        };
        let domain = get("d")?.to_ascii_lowercase();
        let selector = get("s")?.to_owned();
        let (header_canon, body_canon) = parse_canon(get("c").unwrap_or("simple/simple"))?;
        let signed_headers: Vec<String> = get("h")?
            .split(':')
            .map(|h| h.trim().to_owned())
            .filter(|h| !h.is_empty())
            .collect();
        if signed_headers.is_empty() {
            return None;
        }
        let body_hash = decode_b64_ws(get("bh")?)?;
        let signature = decode_b64_ws(get("b")?)?;
        let body_length = match get("l") {
            Some(l) => Some(l.parse().ok()?),
            None => None,
        };
        let expiry = match get("x") {
            Some(x) => Some(x.parse().ok()?),
            None => None,
        };
        Some(Signature {
            algorithm,
            domain,
            selector,
            header_canon,
            body_canon,
            signed_headers,
            body_hash,
            signature,
            body_length,
            expiry,
        })
    }
}

pub(crate) fn parse_canon(c: &str) -> Option<(Canon, Canon)> {
    match c.split_once('/') {
        Some((h, b)) => Some((Canon::parse(h)?, Canon::parse(b)?)),
        // A bare `c=relaxed` means relaxed header, simple body (§3.5).
        None => Some((Canon::parse(c)?, Canon::Simple)),
    }
}

/// Parses a `tag=value; tag=value` list (RFC 6376 §3.2). Whitespace
/// (including folding) around tags and values is stripped. Shared with
/// [`crate::arc`] (ARC headers use the same tag-list syntax).
pub(crate) fn parse_tag_list(value: &str) -> Vec<(String, String)> {
    let mut tags = Vec::new();
    for part in value.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((name, val)) = part.split_once('=') {
            tags.push((name.trim().to_owned(), val.trim().to_owned()));
        }
    }
    tags
}

/// Base64-decodes after stripping all whitespace (b=/bh= may fold).
pub(crate) fn decode_b64_ws(s: &str) -> Option<Vec<u8>> {
    let clean: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    BASE64.decode(clean).ok()
}

/// A DKIM public-key record fetched from DNS (§3.6.1).
pub(crate) struct KeyRecord {
    pub(crate) algorithm: KeyAlgorithm,
    pub(crate) public_key: Vec<u8>,
}

/// Fetches and parses the public-key TXT at `selector._domainkey.domain`.
/// `Ok(None)` means an empty `p=` (revoked key). Shared with
/// [`crate::arc`] (ARC keys are ordinary DKIM keys, RFC 8617 §4.1.2).
pub(crate) async fn fetch_key<R: Resolver + ?Sized>(
    resolver: &R,
    selector: &str,
    domain: &str,
) -> Result<Option<KeyRecord>, DnsError> {
    let name = format!("{selector}._domainkey.{domain}");
    let txts = resolver.txt(&name).await?;
    // Use the first record that parses as a key.
    for txt in &txts {
        let tags = parse_tag_list(txt);
        let get = |k: &str| tags.iter().find(|(t, _)| t == k).map(|(_, v)| v.as_str());
        // k= default is rsa.
        let algorithm = match get("k").unwrap_or("rsa") {
            "rsa" => KeyAlgorithm::RsaSha256,
            "ed25519" => KeyAlgorithm::Ed25519Sha256,
            _ => continue,
        };
        let Some(p) = get("p") else { continue };
        if p.is_empty() {
            return Ok(None); // revoked
        }
        let Some(public_key) = decode_b64_ws(p) else {
            continue;
        };
        return Ok(Some(KeyRecord {
            algorithm,
            public_key,
        }));
    }
    Err(DnsError::NotFound { name, rtype: "TXT" })
}

/// Verifies an RSASSA-PKCS1-v1_5/SHA-256 signature over `data` (the raw
/// canonicalized headers — ring computes the hash). `spki_der` is the
/// DKIM `p=` key (SubjectPublicKeyInfo); ring wants the inner PKCS#1
/// `RSAPublicKey`, so we unwrap the SPKI first (defensively).
pub(crate) fn verify_rsa(spki_der: &[u8], data: &[u8], sig: &[u8]) -> bool {
    use ring::signature::{RSA_PKCS1_2048_8192_SHA256, UnparsedPublicKey};
    let Some(pkcs1) = spki_to_pkcs1_rsa(spki_der) else {
        return false;
    };
    UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, pkcs1)
        .verify(data, sig)
        .is_ok()
}

/// Extracts the PKCS#1 `RSAPublicKey` DER from a SubjectPublicKeyInfo
/// (RFC 5280 §4.1): `SEQUENCE { AlgorithmIdentifier, BIT STRING }`. The
/// BIT STRING's contents (after the unused-bits octet) are the PKCS#1
/// key. Minimal, bounds-checked DER walking — the input is hostile.
fn spki_to_pkcs1_rsa(spki: &[u8]) -> Option<Vec<u8>> {
    let (outer, _) = der_tlv(spki, 0x30)?; // outer SEQUENCE contents
    // Skip the AlgorithmIdentifier (a SEQUENCE).
    let after_algid = der_skip(outer, 0x30)?;
    // The subjectPublicKey is a BIT STRING.
    let (bitstring, _) = der_tlv(after_algid, 0x03)?;
    // First octet of a BIT STRING value is the count of unused bits;
    // for a key it is 0. The remainder is the PKCS#1 RSAPublicKey.
    let (&unused, rest) = bitstring.split_first()?;
    if unused != 0 {
        return None;
    }
    Some(rest.to_vec())
}

/// Reads one DER TLV of the expected `tag` at the start of `input`,
/// returning (value, bytes_consumed). Supports short and long-form
/// length. Rejects truncation.
fn der_tlv(input: &[u8], tag: u8) -> Option<(&[u8], usize)> {
    if input.first()? != &tag {
        return None;
    }
    let len_byte = *input.get(1)?;
    let (len, header) = if len_byte & 0x80 == 0 {
        (len_byte as usize, 2)
    } else {
        let n = (len_byte & 0x7f) as usize;
        if n == 0 || n > 4 {
            return None; // no indefinite form; cap at 4-byte lengths
        }
        let mut len = 0usize;
        for i in 0..n {
            len = (len << 8) | *input.get(2 + i)? as usize;
        }
        (len, 2 + n)
    };
    let end = header.checked_add(len)?;
    let value = input.get(header..end)?;
    Some((value, end))
}

/// Skips one DER TLV of `tag`, returning the bytes that follow it.
fn der_skip(input: &[u8], tag: u8) -> Option<&[u8]> {
    let (_, consumed) = der_tlv(input, tag)?;
    input.get(consumed..)
}

pub(crate) fn verify_ed25519(raw_key: &[u8], hash: &[u8], sig: &[u8]) -> bool {
    use ed25519_dalek::{Signature as EdSig, Verifier, VerifyingKey};
    let Ok(key_bytes): Result<[u8; 32], _> = raw_key.try_into() else {
        return false;
    };
    let Ok(key) = VerifyingKey::from_bytes(&key_bytes) else {
        return false;
    };
    let Ok(sig_bytes): Result<[u8; 64], _> = sig.try_into() else {
        return false;
    };
    let signature = EdSig::from_bytes(&sig_bytes);
    key.verify(hash, &signature).is_ok()
}

/// Parameters for producing a DKIM signature on an outbound message.
pub struct SignParams {
    /// Signing domain (`d=`).
    pub domain: String,
    /// Selector (`s=`).
    pub selector: String,
    /// Header names to sign (`h=`), in order.
    pub signed_headers: Vec<String>,
    /// Header canonicalization.
    pub header_canon: Canon,
    /// Body canonicalization.
    pub body_canon: Canon,
    /// Optional `l=` body-length limit (count of *canonicalized* body
    /// octets to sign). `None` (the default) signs the whole body — the
    /// safer choice, since `l=` lets content be appended after the
    /// signed portion.
    pub body_length: Option<usize>,
}

impl SignParams {
    /// Sensible defaults: relaxed/relaxed over the headers a signer
    /// should always cover, signing the whole body (no `l=`).
    pub fn new(domain: impl Into<String>, selector: impl Into<String>) -> Self {
        Self {
            domain: domain.into(),
            selector: selector.into(),
            signed_headers: ["From", "To", "Subject", "Date", "Message-ID"]
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            header_canon: Canon::Relaxed,
            body_canon: Canon::Relaxed,
            body_length: None,
        }
    }
}

/// Why signing failed.
#[derive(Debug, thiserror::Error)]
pub enum SignError {
    /// No usable key for (domain, selector).
    #[error("signing key unavailable: {0}")]
    Key(#[from] keystore::KeyStoreError),
    /// The key bytes could not be parsed for the declared algorithm.
    #[error("signing key could not be parsed")]
    BadKey,
    /// The signing operation itself failed.
    #[error("signature computation failed")]
    SignFailed,
}

/// Produces a `DKIM-Signature` header value (without the `DKIM-Signature:`
/// name or trailing CRLF) for `message`, using the key from `keys`.
///
/// # Errors
/// [`SignError`] when the key is unavailable/unparseable or signing
/// fails. Never panics on message content.
pub async fn sign<K: KeyStore + ?Sized>(
    keys: &K,
    message: &Message<'_>,
    params: &SignParams,
) -> Result<String, SignError> {
    let key = keys.get(&params.domain, &params.selector).await?;

    // Body hash — over the canonicalized body, truncated to `l=` octets
    // when a body length is configured (§3.7).
    let body_canon = canon::body(params.body_canon, message.body);
    let bh_input = match params.body_length {
        Some(l) if l <= body_canon.len() => &body_canon[..l],
        _ => &body_canon[..],
    };
    let bh = BASE64.encode(Sha256::digest(bh_input));

    // Assemble the DKIM-Signature with an empty b=, compute the header
    // hash over the signed headers + this signature, then fill in b=.
    let now = jiff::Timestamp::now().as_second();
    let c = format!(
        "{}/{}",
        canon_tag(params.header_canon),
        canon_tag(params.body_canon)
    );
    let h = params.signed_headers.join(":");
    let a = key.algorithm.tag();
    let l_tag = match params.body_length {
        Some(l) => format!(" l={l};"),
        None => String::new(),
    };
    let sig_value_no_b = format!(
        "v=1; a={a}; c={c}; d={}; s={}; t={now}; h={h};{l_tag} bh={bh}; b=",
        params.domain, params.selector
    );

    // Build the header-hash input: signed headers (bottom-up), then the
    // partial DKIM-Signature (b= empty, no trailing CRLF).
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
                input.extend_from_slice(&canon::header(params.header_canon, name, value));
                break;
            }
        }
    }
    let mut canon_sig = canon::header(
        params.header_canon,
        "DKIM-Signature",
        &format!(" {sig_value_no_b}"),
    );
    if canon_sig.ends_with(b"\r\n") {
        canon_sig.truncate(canon_sig.len() - 2);
    }
    input.extend_from_slice(&canon_sig);

    // RSA signs the data (ring hashes it); Ed25519 signs the SHA-256
    // hash of the data (RFC 8463).
    let signature_bytes = match key.algorithm {
        KeyAlgorithm::RsaSha256 => sign_rsa(&key, &input)?,
        KeyAlgorithm::Ed25519Sha256 => sign_ed25519(&key, &Sha256::digest(&input))?,
    };
    let b = BASE64.encode(signature_bytes);
    Ok(format!(
        "v=1; a={a}; c={c}; d={}; s={}; t={now}; h={h};{l_tag} bh={bh}; b={b}",
        params.domain, params.selector
    ))
}

fn canon_tag(c: Canon) -> &'static str {
    match c {
        Canon::Simple => "simple",
        Canon::Relaxed => "relaxed",
    }
}

/// Signs `data` (the raw canonicalized headers) with RSASSA-PKCS1-v1_5
/// SHA-256 via ring (constant-time; ring hashes the data internally).
pub(crate) fn sign_rsa(key: &SigningKey, data: &[u8]) -> Result<Vec<u8>, SignError> {
    use ring::rand::SystemRandom;
    use ring::signature::{RSA_PKCS1_SHA256, RsaKeyPair};
    let pair = RsaKeyPair::from_pkcs8(&key.pkcs8_der).map_err(|_| SignError::BadKey)?;
    let mut signature = vec![0u8; pair.public().modulus_len()];
    pair.sign(
        &RSA_PKCS1_SHA256,
        &SystemRandom::new(),
        data,
        &mut signature,
    )
    .map_err(|_| SignError::SignFailed)?;
    Ok(signature)
}

/// Signs the 32-byte SHA-256 `hash` of the headers with Ed25519
/// (RFC 8463).
pub(crate) fn sign_ed25519(key: &SigningKey, hash: &[u8]) -> Result<Vec<u8>, SignError> {
    use ed25519_dalek::Signer;
    use ed25519_dalek::SigningKey as EdSigningKey;
    use ed25519_dalek::pkcs8::DecodePrivateKey;
    let signing = EdSigningKey::from_pkcs8_der(&key.pkcs8_der).map_err(|_| SignError::BadKey)?;
    Ok(signing.sign(hash).to_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::resolver::fixture::FixtureResolver;
    use keystore::{FileKeyStore, KeyAlgorithm};

    fn write_key_pem(dir: &std::path::Path, name: &str, pkcs8_der: &[u8]) -> std::path::PathBuf {
        let b64 = BASE64.encode(pkcs8_der);
        let pem = format!("-----BEGIN PRIVATE KEY-----\n{b64}\n-----END PRIVATE KEY-----\n");
        let path = dir.join(name);
        std::fs::write(&path, pem).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        path
    }

    const RAW_MSG: &[u8] =
        b"From: alice@example.com\r\nTo: bob@example.org\r\nSubject: hi\r\nDate: today\r\nMessage-ID: <1@example.com>\r\n\r\nhello world\r\n";

    // Cross-implementation known-answer vector: a message signed by
    // **dkimpy** (an independent DKIM implementation), base64 of the raw
    // signed bytes, plus the matching SPKI public key. Our verifier must
    // accept it — this pins the §3.7 header/body-hash construction to a
    // foreign signer rather than only round-tripping our own sign path.
    // Note dkimpy's `h=` folds with spaces around the colons
    // (`from : to : subject : date`) and adds `i=`/`q=` tags, exercising
    // tolerant tag/header parsing.
    const KAT_SIGNED_B64: &str = "REtJTS1TaWduYXR1cmU6IHY9MTsgYT1yc2Etc2hhMjU2OyBjPXJlbGF4ZWQvcmVsYXhlZDsgZD1leGFtcGxlLmNvbTsNCiBpPUBleGFtcGxlLmNvbTsgcT1kbnMvdHh0OyBzPWthdDsgdD0xNzg1MTM4OTA0OyBoPWZyb20gOiB0byA6IHN1YmplY3QNCiA6IGRhdGU7IGJoPSs5MFdjTWRrMHkyWGFGcFROV2QxSDA5YklXZ2hFQ1I2RUFnQ3VDVTBGNlE9Ow0KIGI9R3F2M3k3R3R4RytJUTgwdS9wK0g5SCtoYkpqZDNlNkp3dTlBZDlVZTM3VFZqdFFqZzhmdFNsSjdRS1ovaDhOcFh1VDJiDQogRGZQM2Y2ZUVDL2ZVSndwZ1RRb24wZ2lKZll1dXZQSEFqSDl4UjdyZ1BEdWdPV0YxbDB3SmRDU2xXVUhIV09oN2tLd1REMTgNCiAzU2JleURkNFIrNFhHcXlPWjcvdEl0ZmZJQ2VPV1RMeGUrSWE3L2R0VFk4QUMwYXJRVzNoSlloM1dzZEV5dFRCNEZVRWFQWA0KIGlzdXgzN1E1ai95aGtrcXBZNDNtYmJkSS9zSmQ5azZqbStFcURzZzg2QUN3UkhMbEV5ZFpKQnFja0R5WnJSLzI4WnZ3SDlWDQogY0pXM0RoaDJkeXU5TUZVa2ZQNVdjVkRIUGU4QVlXWkF0cDhra3ZLeEx5ZlI4MGx4bXo2Q1VYdW9mUTR3PT0NCkZyb206IGFsaWNlQGV4YW1wbGUuY29tDQpUbzogYm9iQGV4YW1wbGUub3JnDQpTdWJqZWN0OiBLQVQNCkRhdGU6IE1vbiwgMjcgSnVsIDIwMjYgMDA6MDA6MDAgKzAwMDANCg0Ka25vd24gYW5zd2VyIHRlc3QgYm9keQ0K";
    const KAT_SPKI_B64: &str = "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAzKI7slCL74n+ZokHGvNBM0RX0sP9Ah+UChO2ACzg+sIHQZ1paLNANrZeSr4G0lSdrBg+nywtwGZkEyRaBVLeX7F4bxwCaXUgxnf0z7AzXrw1LIlm6gNofyIVxBMwhs3FquZuYkrq5NHfEHsUfLGb0ynUGvib2CpJW+onoxkrwAGf51fYkLuGf7GRP2kQUGo4jSc+3ciCwKIV7EDa76iFEWoO2LxHNk286vUE0friHOkzv0x77dg4VxbaWrW7K5TfsxWFuilgiF6BebXy0GMEF5vUcFATIKedCPA+XG0MaqyRJKJBGNurPf9wibcS9jN2mGRafOES4BO8QRL5G9GhKwIDAQAB";

    // The committed RSA-2048 fixture pair (see `keystore::fixture_keys`),
    // exercising the ring RSA path (sign + SPKI-unwrap + verify).
    use keystore::fixture_keys::{RSA_PKCS8_B64, RSA_SPKI_B64};

    #[tokio::test]
    async fn rsa_sign_then_verify_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let pkcs8 = BASE64.decode(RSA_PKCS8_B64).unwrap();
        let path = write_key_pem(dir.path(), "rsa.pem", &pkcs8);
        let keys =
            FileKeyStore::new().with_key("example.com", "rsa1", path, KeyAlgorithm::RsaSha256);
        let msg = Message::parse(RAW_MSG);
        let sig_value = sign(&keys, &msg, &SignParams::new("example.com", "rsa1"))
            .await
            .unwrap();

        let signed = prepend_header(&sig_value);
        // k=rsa is the default; p= is the SPKI public key.
        let dns = FixtureResolver::default().with_txt(
            "rsa1._domainkey.example.com",
            &[&format!("v=DKIM1; p={RSA_SPKI_B64}")],
        );
        let verdicts = verify(&dns, &Message::parse(&signed)).await;
        assert_eq!(verdicts.len(), 1);
        assert_eq!(verdicts[0].result, DkimResult::Pass, "{verdicts:?}");
    }

    #[tokio::test]
    async fn ed25519_sign_then_verify_roundtrip() {
        use ed25519_dalek::SigningKey as EdSigningKey;
        use ed25519_dalek::pkcs8::EncodePrivateKey;
        let dir = tempfile::tempdir().unwrap();

        // Generate an Ed25519 key; PKCS#8 DER for the signer, raw public
        // for the DNS record.
        let signing = EdSigningKey::from_bytes(&[7u8; 32]);
        let pkcs8 = signing.to_pkcs8_der().unwrap();
        let path = write_key_pem(dir.path(), "ed.pem", pkcs8.as_bytes());
        let public_b64 = BASE64.encode(signing.verifying_key().to_bytes());

        let keys =
            FileKeyStore::new().with_key("example.com", "sel1", path, KeyAlgorithm::Ed25519Sha256);
        let msg = Message::parse(RAW_MSG);
        let params = SignParams::new("example.com", "sel1");
        let sig_value = sign(&keys, &msg, &params).await.unwrap();

        // Re-parse the message WITH the new DKIM-Signature header and verify.
        let signed_raw = prepend_header(&sig_value);
        let signed_msg = Message::parse(&signed_raw);
        let dns = FixtureResolver::default().with_txt(
            "sel1._domainkey.example.com",
            &[&format!("v=DKIM1; k=ed25519; p={public_b64}")],
        );
        let verdicts = verify(&dns, &signed_msg).await;
        assert_eq!(verdicts.len(), 1);
        assert_eq!(verdicts[0].result, DkimResult::Pass, "{verdicts:?}");
        assert_eq!(verdicts[0].domain, "example.com");
    }

    #[tokio::test]
    async fn tampered_body_fails() {
        use ed25519_dalek::SigningKey as EdSigningKey;
        use ed25519_dalek::pkcs8::EncodePrivateKey;
        let dir = tempfile::tempdir().unwrap();
        let signing = EdSigningKey::from_bytes(&[9u8; 32]);
        let path = write_key_pem(
            dir.path(),
            "ed.pem",
            signing.to_pkcs8_der().unwrap().as_bytes(),
        );
        let public_b64 = BASE64.encode(signing.verifying_key().to_bytes());
        let keys =
            FileKeyStore::new().with_key("example.com", "s", path, KeyAlgorithm::Ed25519Sha256);
        let msg = Message::parse(RAW_MSG);
        let sig_value = sign(&keys, &msg, &SignParams::new("example.com", "s"))
            .await
            .unwrap();

        // Prepend the signature, then tamper the body.
        let mut signed = prepend_header(&sig_value);
        let idx = find_double_crlf(&signed).unwrap() + 4;
        signed[idx] = b'X'; // mutate first body byte
        let dns = FixtureResolver::default().with_txt(
            "s._domainkey.example.com",
            &[&format!("v=DKIM1; k=ed25519; p={public_b64}")],
        );
        let verdicts = verify(&dns, &Message::parse(&signed)).await;
        assert_eq!(verdicts[0].result, DkimResult::Fail);
    }

    #[tokio::test]
    async fn verifies_dkimpy_signed_message_known_answer() {
        // Independent cross-check: dkimpy produced this signature; our
        // verifier must return pass against the published SPKI key.
        let signed = BASE64.decode(KAT_SIGNED_B64).unwrap();
        let dns = FixtureResolver::default().with_txt(
            "kat._domainkey.example.com",
            &[&format!("v=DKIM1; k=rsa; p={KAT_SPKI_B64}")],
        );
        let verdicts = verify(&dns, &Message::parse(&signed)).await;
        assert_eq!(verdicts.len(), 1);
        assert_eq!(verdicts[0].result, DkimResult::Pass, "{verdicts:?}");
        assert_eq!(verdicts[0].domain, "example.com");
        assert_eq!(verdicts[0].selector, "kat");
    }

    #[tokio::test]
    async fn malformed_signature_is_permerror_not_panic() {
        let dns = FixtureResolver::default();
        let raw = b"DKIM-Signature: this is garbage\r\nFrom: a@b\r\n\r\nbody\r\n";
        let verdicts = verify(&dns, &Message::parse(raw)).await;
        assert_eq!(verdicts[0].result, DkimResult::PermError);
    }

    #[tokio::test]
    async fn no_signature_yields_empty() {
        let dns = FixtureResolver::default();
        let raw = b"From: a@b\r\nSubject: x\r\n\r\nbody\r\n";
        assert!(verify(&dns, &Message::parse(raw)).await.is_empty());
    }

    /// Helper: prepend a DKIM-Signature header to the fixed test message.
    fn prepend_header(sig_value: &str) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"DKIM-Signature: ");
        out.extend_from_slice(sig_value.as_bytes());
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(RAW_MSG);
        out
    }

    #[test]
    fn a_keys_algorithm_is_read_from_its_bytes_not_guessed() {
        // The RSA fixture reads as RSA, a generated Ed25519 key as Ed25519,
        // and bytes that are neither are nothing — the caller must refuse,
        // not default (a wrong `a=` tag signs garbage every receiver rejects).
        use ed25519_dalek::SigningKey as EdSigningKey;
        use ed25519_dalek::pkcs8::EncodePrivateKey;
        let rsa = BASE64.decode(RSA_PKCS8_B64).unwrap();
        assert_eq!(
            keystore::algorithm_of_pkcs8(&rsa),
            Some(KeyAlgorithm::RsaSha256)
        );
        let ed = EdSigningKey::from_bytes(&[11u8; 32])
            .to_pkcs8_der()
            .unwrap();
        assert_eq!(
            keystore::algorithm_of_pkcs8(ed.as_bytes()),
            Some(KeyAlgorithm::Ed25519Sha256)
        );
        assert_eq!(keystore::algorithm_of_pkcs8(b"not a key"), None);
        assert_eq!(keystore::algorithm_of_pkcs8(&[]), None);
    }

    #[test]
    fn strip_b_tag_empties_only_b() {
        let v = "v=1; a=rsa-sha256; bh=ABC; b=SIGDATA; h=from";
        let stripped = strip_b_tag(v);
        assert!(stripped.contains("bh=ABC"));
        assert!(stripped.contains("b=;") || stripped.trim_end().ends_with("b="));
        assert!(!stripped.contains("SIGDATA"));
    }

    #[test]
    fn strip_b_tag_handles_megabyte_whitespace_promptly() {
        // Hostile input: a DKIM-Signature value padded with a huge
        // folding-whitespace run before `b=`. The linear scan must strip
        // it without super-linear work (this returns instantly; the old
        // O(n²) scan would hang).
        let mut v = String::from("v=1; a=rsa-sha256; bh=ABC;");
        v.push_str(&" ".repeat(2_000_000));
        v.push_str("b=SIGDATA; h=from");
        let stripped = strip_b_tag(&v);
        assert!(!stripped.contains("SIGDATA"));
        assert!(stripped.contains("b=;"));
    }

    #[tokio::test]
    async fn signature_without_from_is_permerror() {
        // §6.1.1: a signature whose h= omits From must PERMFAIL even if
        // every other check would pass. We build a valid signature then
        // present it with an h= that excludes From.
        use ed25519_dalek::SigningKey as EdSigningKey;
        use ed25519_dalek::pkcs8::EncodePrivateKey;
        let dir = tempfile::tempdir().unwrap();
        let signing = EdSigningKey::from_bytes(&[3u8; 32]);
        let path = write_key_pem(
            dir.path(),
            "ed.pem",
            signing.to_pkcs8_der().unwrap().as_bytes(),
        );
        let public_b64 = BASE64.encode(signing.verifying_key().to_bytes());
        let keys =
            FileKeyStore::new().with_key("example.com", "s", path, KeyAlgorithm::Ed25519Sha256);
        let msg = Message::parse(RAW_MSG);
        // Sign only Subject/Date (no From).
        let mut params = SignParams::new("example.com", "s");
        params.signed_headers = vec!["Subject".to_owned(), "Date".to_owned()];
        let sig_value = sign(&keys, &msg, &params).await.unwrap();
        let signed = prepend_header(&sig_value);
        let dns = FixtureResolver::default().with_txt(
            "s._domainkey.example.com",
            &[&format!("v=DKIM1; k=ed25519; p={public_b64}")],
        );
        let verdicts = verify(&dns, &Message::parse(&signed)).await;
        assert_eq!(verdicts[0].result, DkimResult::PermError, "{verdicts:?}");
    }

    #[tokio::test]
    async fn body_length_truncates_canonicalized_body() {
        // A signature with l= set to the canonicalized body length must
        // still verify when trailing content is appended after signing
        // (RFC 6376 §3.7) — this exercises the canonicalize-then-truncate
        // path (the old truncate-then-canonicalize order mis-scored it).
        use ed25519_dalek::SigningKey as EdSigningKey;
        use ed25519_dalek::pkcs8::EncodePrivateKey;
        let dir = tempfile::tempdir().unwrap();
        let signing = EdSigningKey::from_bytes(&[5u8; 32]);
        let path = write_key_pem(
            dir.path(),
            "ed.pem",
            signing.to_pkcs8_der().unwrap().as_bytes(),
        );
        let public_b64 = BASE64.encode(signing.verifying_key().to_bytes());
        let keys =
            FileKeyStore::new().with_key("example.com", "s", path, KeyAlgorithm::Ed25519Sha256);
        let msg = Message::parse(RAW_MSG);

        // Sign with an explicit l= equal to the canonicalized body length.
        let canon_len = canon::body(Canon::Relaxed, msg.body).len();
        let mut params = SignParams::new("example.com", "s");
        params.body_length = Some(canon_len);
        let sig_value = sign(&keys, &msg, &params).await.unwrap();

        // Append extra body after signing — the l= limit must exclude it.
        let mut signed = prepend_header(&sig_value);
        signed.extend_from_slice(b"trailing tampering\r\n");
        let dns = FixtureResolver::default().with_txt(
            "s._domainkey.example.com",
            &[&format!("v=DKIM1; k=ed25519; p={public_b64}")],
        );
        let verdicts = verify(&dns, &Message::parse(&signed)).await;
        assert_eq!(verdicts[0].result, DkimResult::Pass, "{verdicts:?}");
    }

    #[test]
    fn non_utf8_octet_drops_only_its_own_header() {
        // A stray 8-bit byte in one header must not erase the others.
        let mut raw = Vec::new();
        raw.extend_from_slice(b"From: alice@example.com\r\n");
        raw.extend_from_slice(b"Subject: caf\xe9 time\r\n"); // Latin-1 é
        raw.extend_from_slice(b"To: bob@example.org\r\n\r\nbody\r\n");
        let msg = Message::parse(&raw);
        assert!(
            msg.headers
                .iter()
                .any(|(n, _)| n.eq_ignore_ascii_case("From"))
        );
        assert!(
            msg.headers
                .iter()
                .any(|(n, _)| n.eq_ignore_ascii_case("To"))
        );
        assert!(
            !msg.headers
                .iter()
                .any(|(n, _)| n.eq_ignore_ascii_case("Subject")),
            "the non-UTF-8 Subject is dropped, not the whole block"
        );
    }
}
