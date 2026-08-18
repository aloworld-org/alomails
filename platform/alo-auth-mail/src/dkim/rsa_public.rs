//! The public half of an RSA signing key, in the encoding a DKIM record
//! publishes.
//!
//! RFC 6376 §3.6.1 says a `k=rsa` record's `p=` carries the base64 of a
//! `SubjectPublicKeyInfo` (RFC 5280 §4.1.2.7) — the same bytes
//! `openssl rsa -pubout -outform DER` writes, which is what
//! `deploy/production/generate-dkim.sh` has always published. `ring` hands us
//! the *other* encoding: `RsaKeyPair::public()` serialises the PKCS#1
//! `RSAPublicKey` (`SEQUENCE { modulus, exponent }`) alone. This module is the
//! ~20 bytes of DER between the two.
//!
//! It exists as its own file because it is the one place in this crate that
//! writes DER by hand, and because getting it wrong is invisible: a malformed
//! record does not fail here, it fails at every receiver weeks later. The tests
//! pin the encoder byte-for-byte rather than checking it "looks right".
//!
//! Kept out of [`super::keystore`] (Law 3): that module's reason to change is
//! how keys are stored and loaded; this one's is an encoding fixed by an RFC.

/// DER object identifier for `rsaEncryption` (PKCS#1, 1.2.840.113549.1.1.1),
/// tag and length included.
const OID_RSA_ENCRYPTION: &[u8] = &[
    0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01,
];

/// DER `NULL` — the parameters of `rsaEncryption`, which RFC 4055 §1.2 requires
/// to be present and NULL rather than absent.
const DER_NULL: &[u8] = &[0x05, 0x00];

/// The `SubjectPublicKeyInfo` DER for an RSA private key in PKCS#8 DER — the
/// bytes a `k=rsa` DKIM record base64s into `p=`.
///
/// Returns `None` when the key is not a usable RSA private key. The key is only
/// read here; nothing secret is returned or retained.
pub fn spki_from_pkcs8(pkcs8_der: &[u8]) -> Option<Vec<u8>> {
    let pair = ring::signature::RsaKeyPair::from_pkcs8(pkcs8_der).ok()?;
    Some(spki_from_pkcs1(pair.public().as_ref()))
}

/// Wraps a PKCS#1 `RSAPublicKey` DER in a `SubjectPublicKeyInfo`:
///
/// ```text
/// SEQUENCE {
///   SEQUENCE { OBJECT IDENTIFIER rsaEncryption, NULL },
///   BIT STRING { 0 unused bits, <the PKCS#1 key> }
/// }
/// ```
fn spki_from_pkcs1(pkcs1: &[u8]) -> Vec<u8> {
    let mut algorithm = Vec::with_capacity(OID_RSA_ENCRYPTION.len() + DER_NULL.len());
    algorithm.extend_from_slice(OID_RSA_ENCRYPTION);
    algorithm.extend_from_slice(DER_NULL);

    // A BIT STRING's content is preceded by the count of unused bits in its
    // final octet; a whole number of bytes always has none.
    let mut bit_string_body = Vec::with_capacity(pkcs1.len() + 1);
    bit_string_body.push(0x00);
    bit_string_body.extend_from_slice(pkcs1);

    let mut body = tlv(0x30, &algorithm);
    body.extend_from_slice(&tlv(0x03, &bit_string_body));
    tlv(0x30, &body)
}

/// One DER tag-length-value.
fn tlv(tag: u8, value: &[u8]) -> Vec<u8> {
    let length = der_length(value.len());
    let mut out = Vec::with_capacity(1 + length.len() + value.len());
    out.push(tag);
    out.extend_from_slice(&length);
    out.extend_from_slice(value);
    out
}

/// DER definite length, in the short form below 128 and the minimal long form
/// above it (X.690 §8.1.3). Minimal matters: a non-minimal length is BER, not
/// DER, and a strict parser rejects it.
fn der_length(len: usize) -> Vec<u8> {
    if len < 0x80 {
        return vec![len as u8];
    }
    let bytes = len.to_be_bytes();
    let first = bytes
        .iter()
        .position(|&b| b != 0)
        .unwrap_or(bytes.len() - 1);
    let significant = &bytes[first..];
    let mut out = Vec::with_capacity(significant.len() + 1);
    // The high bit marks the long form; the rest is how many length bytes
    // follow. `usize` is at most 8 bytes, so this never overflows 0x7f.
    out.push(0x80 | significant.len() as u8);
    out.extend_from_slice(significant);
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn short_form_lengths_are_one_byte() {
        assert_eq!(der_length(0), vec![0x00]);
        assert_eq!(der_length(1), vec![0x01]);
        assert_eq!(der_length(127), vec![0x7f]);
    }

    #[test]
    fn long_form_lengths_are_minimal() {
        // 128 is the first length needing the long form, and it must be one
        // byte of content rather than a padded two.
        assert_eq!(der_length(128), vec![0x81, 0x80]);
        assert_eq!(der_length(255), vec![0x81, 0xff]);
        assert_eq!(der_length(256), vec![0x82, 0x01, 0x00]);
        // The size an RSA-2048 SubjectPublicKeyInfo actually lands on.
        assert_eq!(der_length(270), vec![0x82, 0x01, 0x0e]);
        assert_eq!(der_length(65_535), vec![0x82, 0xff, 0xff]);
        assert_eq!(der_length(65_536), vec![0x83, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn the_wrapper_is_byte_exact_for_a_known_body() {
        // A stand-in PKCS#1 body: what matters is the 24 bytes of structure
        // around it, and those are the same whatever the key is.
        let pkcs1 = [0xaa, 0xbb, 0xcc];
        let spki = spki_from_pkcs1(&pkcs1);
        assert_eq!(
            spki,
            vec![
                0x30, 0x15, // SEQUENCE, 21 bytes
                0x30, 0x0d, // SEQUENCE, 13 bytes (the AlgorithmIdentifier)
                0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01,
                0x01, // rsaEncryption
                0x05, 0x00, // NULL parameters
                0x03, 0x04, 0x00, // BIT STRING, 4 bytes, 0 of them unused bits
                0xaa, 0xbb, 0xcc,
            ]
        );
    }

    #[test]
    fn a_long_body_crosses_into_the_long_form_in_both_places() {
        // An RSA-2048 PKCS#1 key is 270 bytes, so both the outer SEQUENCE and
        // the BIT STRING need multi-byte lengths — the case a short fixture
        // would never reach.
        let pkcs1 = vec![0x7f; 270];
        let spki = spki_from_pkcs1(&pkcs1);
        // outer: tag + 3 length bytes; inner AlgorithmIdentifier: 15 bytes;
        // BIT STRING: tag + 3 length bytes + 1 unused-bit byte + body.
        assert_eq!(spki.len(), 4 + 15 + 4 + 1 + 270);
        assert_eq!(&spki[..4], &[0x30, 0x82, 0x01, 0x22]);
        assert_eq!(&spki[19..23], &[0x03, 0x82, 0x01, 0x0f]);
        assert_eq!(spki[23], 0x00, "unused-bit count");
        assert_eq!(&spki[24..], &pkcs1[..]);
    }

    #[test]
    fn a_key_that_is_not_rsa_yields_nothing() {
        assert!(spki_from_pkcs8(b"not a key").is_none());
        assert!(spki_from_pkcs8(&[]).is_none());
        // A well-formed Ed25519 PKCS#8 key is not an RSA one.
        let ed = super::super::keystore::generate_ed25519_key().expect("keygen");
        let der = super::super::keystore::ed25519_signing_key_from_seed(ed.seed.as_ref())
            .expect("from seed")
            .pkcs8_der;
        assert!(spki_from_pkcs8(&der).is_none());
    }
}
